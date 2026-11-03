//! Mises à jour du launcher : dernière release GitHub, comparaison de versions,
//! téléchargement vérifié et remplacement du binaire courant.
//!
//! L'archive est téléchargée dans `~/.local/share/quark/updates/`, son membre
//! `quark/quark` est vérifié (SHA-256, ELF x86_64 et version `--help`), puis le
//! binaire est remplacé atomiquement. Une sauvegarde de l'ancien binaire reste
//! à côté jusqu'au prochain démarrage réussi.

use reqwest::blocking::Client;
use serde::Deserialize;
use std::io::Read as _;
use std::path::{Path, PathBuf};

/// Dépôt dont on suit les releases.
pub const REPO: &str = "Matheotaku4/Quark-Launcher";

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    #[serde(default)]
    pub tag_name: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

const MAX_DECOMPRESSED: u64 = 256 * 1024 * 1024;

/// Mise à jour téléchargée et vérifiée, prête à remplacer le binaire courant.
#[derive(Debug)]
pub struct StagedUpdate {
    pub target: PathBuf,
    pub binary: PathBuf,
    pub archive: PathBuf,
}

/// Résultat du remplacement atomique, conservé pour pouvoir revenir en arrière
/// si `exec` échoue.
#[derive(Debug)]
pub struct InstalledUpdate {
    pub target: PathBuf,
    pub backup: PathBuf,
}

impl Release {
    /// Archive `tar.gz` de la release, s'il y en a une.
    pub fn archive(&self) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name.ends_with(".tar.gz"))
    }

    /// Somme SHA-256 publiée à côté de l'archive par le workflow de release.
    pub fn checksum_for(&self, archive: &Asset) -> Option<&Asset> {
        let expected = format!("{}.sha256", archive.name);
        self.assets
            .iter()
            .find(|asset| asset.name == expected || asset.name.ends_with(&expected))
    }
}

/// Version du binaire en cours d'exécution.
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Dernière release publiée (API publique GitHub, sans authentification).
pub fn latest(client: &Client) -> Result<Release, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let response = client
        .get(&url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .map_err(|err| format!("réseau : {err}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            403 | 429 => "GitHub limite temporairement les requêtes".to_string(),
            404 => "aucune release publiée".to_string(),
            code => format!("GitHub a répondu {code}"),
        });
    }
    crate::net::json(response, crate::net::MAX_JSON, "réponse GitHub")
        .map_err(|err| format!("réponse illisible : {err}"))
}

/// `true` si `candidate` est plus récente que `current` (« v0.4.0 » ou « 0.4.0 »).
pub fn is_newer(candidate: &str, current: &str) -> bool {
    fn parts(version: &str) -> Vec<u64> {
        version
            .trim()
            .trim_start_matches(['v', 'V'])
            .split(|c: char| !c.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .map(|part| part.parse().unwrap_or(0))
            .collect()
    }

    let newer = parts(candidate);
    let installed = parts(current);
    for index in 0..newer.len().max(installed.len()) {
        let left = newer.get(index).copied().unwrap_or(0);
        let right = installed.get(index).copied().unwrap_or(0);
        if left != right {
            return left > right;
        }
    }
    false
}

/// Télécharge une archive de release dans `dir`, vérifie sa taille et sa somme
/// SHA-256 publiée, puis renvoie son chemin. Toute archive partielle ou
/// invalide est supprimée.
pub fn download(
    client: &Client,
    release: &Release,
    asset: &Asset,
    dir: &Path,
) -> Result<PathBuf, String> {
    const MAX_ARCHIVE: u64 = 512 * 1024 * 1024;
    std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    if asset.size > MAX_ARCHIVE {
        return Err("archive de mise à jour trop volumineuse".to_string());
    }
    let checksum = release
        .checksum_for(asset)
        .ok_or_else(|| "la release ne publie pas de somme SHA-256".to_string())?;
    let expected = fetch_checksum(client, checksum)?;
    // Nom de fichier de l'archive seule, jamais le chemin fourni par le serveur.
    let name = asset
        .name
        .rsplit('/')
        .next()
        .unwrap_or("quark.tar.gz")
        .to_string();
    let path = dir.join(name);
    let result = (|| -> Result<(), String> {
        let response = client
            .get(&asset.browser_download_url)
            .send()
            .map_err(|err| format!("réseau : {err}"))?;
        if !response.status().is_success() {
            return Err(format!("GitHub a répondu {}", response.status().as_u16()));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_ARCHIVE)
        {
            return Err("archive de mise à jour trop volumineuse".to_string());
        }
        let mut file = std::fs::File::create(&path).map_err(|err| err.to_string())?;
        let copied = std::io::copy(&mut response.take(MAX_ARCHIVE + 1), &mut file)
            .map_err(|err| err.to_string())?;
        if copied > MAX_ARCHIVE {
            return Err("archive de mise à jour trop volumineuse".to_string());
        }
        if asset.size != 0 && copied != asset.size {
            return Err(format!(
                "archive incomplète : {copied} octets reçus, {} annoncés",
                asset.size
            ));
        }
        drop(file);
        let actual = file_sha256(&path)?;
        if actual != expected {
            return Err("somme SHA-256 de l'archive invalide".to_string());
        }
        Ok(())
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&path);
        return Err(error);
    }
    Ok(path)
}

/// Vérifie que le dossier du binaire courant est inscriptible avant de
/// télécharger 11 Mo pour rien. Le remplacement se fait par renommage dans ce
/// même dossier, donc un binaire dans `/usr/bin` sans droits échoue ici.
pub fn can_replace(target: &Path) -> Result<(), String> {
    let parent = target
        .parent()
        .ok_or_else(|| "chemin du binaire sans dossier parent".to_string())?;
    if !parent.is_dir() {
        return Err(format!(
            "dossier du binaire introuvable : {}",
            parent.display()
        ));
    }
    let probe = parent.join(format!(
        ".quark-write-test-{}-{}",
        std::process::id(),
        crate::config::now_ts()
    ));
    let result = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe);
    let _ = std::fs::remove_file(&probe);
    result
        .map(|_| ())
        .map_err(|err| format!("le dossier du binaire n'est pas inscriptible : {err}"))
}

/// Télécharge l'archive, en extrait `quark/quark`, vérifie son architecture et
/// sa version, puis l'écrit à côté du binaire courant sans le remplacer encore.
/// Le nom `quark/quark` est celui produit par le workflow de release.
pub fn stage(
    client: &Client,
    release: &Release,
    asset: &Asset,
    dir: &Path,
    target: &Path,
) -> Result<StagedUpdate, String> {
    can_replace(target)?;
    let archive = download(client, release, asset, dir)?;
    let tar_bytes = decompress_archive(&archive)?;
    let bytes = crate::archive::extract_member(&tar_bytes, "quark/quark")?;
    verify_elf_x86_64(&bytes)?;

    let parent = target
        .parent()
        .ok_or_else(|| "chemin du binaire sans dossier parent".to_string())?;
    let name = target
        .file_name()
        .ok_or_else(|| "nom du binaire introuvable".to_string())?;
    let binary = parent.join(format!(
        ".{}.update-{}-{}",
        name.to_string_lossy(),
        std::process::id(),
        crate::config::now_ts()
    ));
    let _ = std::fs::remove_file(&binary);
    write_executable(&binary, &bytes)?;

    let expected = release.tag_name.trim().trim_start_matches(['v', 'V']);
    if expected.is_empty() {
        let _ = std::fs::remove_file(&binary);
        return Err("la release n'a pas de numéro de version".to_string());
    }
    if let Err(error) = verify_binary_version(&binary, expected) {
        let _ = std::fs::remove_file(&binary);
        return Err(error);
    }

    Ok(StagedUpdate {
        target: target.to_path_buf(),
        binary,
        archive,
    })
}

fn decompress_archive(archive: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(archive)
        .map_err(|err| format!("archive de mise à jour illisible : {err}"))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut bytes = Vec::new();
    decoder
        .take(MAX_DECOMPRESSED + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "archive de mise à jour invalide".to_string())?;
    if bytes.len() as u64 > MAX_DECOMPRESSED {
        return Err("archive de mise à jour décompressée trop volumineuse".to_string());
    }
    Ok(bytes)
}

fn verify_elf_x86_64(bytes: &[u8]) -> Result<(), String> {
    // e_ident : ELF, 64 bits, little-endian ; e_machine : x86_64.
    if bytes.len() < 64
        || !bytes.starts_with(b"\x7fELF")
        || bytes[4] != 2
        || bytes[5] != 1
        || u16::from_le_bytes([bytes[18], bytes[19]]) != 0x3e
    {
        return Err("le binaire de la release n'est pas un ELF x86_64".to_string());
    }
    Ok(())
}

#[cfg(unix)]
fn write_executable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o755)
        .open(path)
        .map_err(|err| format!("écriture du nouveau binaire impossible : {err}"))?;
    file.write_all(bytes)
        .map_err(|err| format!("écriture du nouveau binaire impossible : {err}"))?;
    file.sync_all()
        .map_err(|err| format!("synchronisation du nouveau binaire impossible : {err}"))?;
    Ok(())
}

/// Le tag GitHub et la version compilée dans le binaire doivent coïncider :
/// cela évite d'installer une archive d'une autre release ou un faux asset.
fn verify_binary_version(path: &Path, expected: &str) -> Result<(), String> {
    let output = std::process::Command::new(path)
        .arg("--help")
        .env("QUARK_NO_AUTO_UPDATE", "1")
        .output()
        .map_err(|err| format!("le nouveau binaire ne démarre pas : {err}"))?;
    let first = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if output.status.success() && first == format!("Quark {expected}") {
        Ok(())
    } else {
        Err(format!(
            "la version du binaire ({first}) ne correspond pas à la release v{expected}"
        ))
    }
}

/// Remplace le binaire par la version préparée. L'ancien binaire est renommé
/// dans le même dossier, ce qui rend l'opération atomique et réversible.
pub fn apply(staged: &StagedUpdate) -> Result<InstalledUpdate, String> {
    if !staged.binary.is_file() {
        return Err("le nouveau binaire préparé est absent".to_string());
    }
    if !staged.target.is_file() {
        return Err("le binaire Quark courant est introuvable".to_string());
    }
    let parent = staged
        .target
        .parent()
        .ok_or_else(|| "chemin du binaire sans dossier parent".to_string())?;
    let name = staged
        .target
        .file_name()
        .ok_or_else(|| "nom du binaire introuvable".to_string())?;
    let backup = parent.join(format!(
        ".{}.old-{}",
        name.to_string_lossy(),
        std::process::id()
    ));
    let _ = std::fs::remove_file(&backup);
    std::fs::rename(&staged.target, &backup)
        .map_err(|err| format!("sauvegarde de l'ancien binaire impossible : {err}"))?;
    if let Err(err) = std::fs::rename(&staged.binary, &staged.target) {
        let _ = std::fs::rename(&backup, &staged.target);
        return Err(format!("remplacement du binaire impossible : {err}"));
    }
    Ok(InstalledUpdate {
        target: staged.target.clone(),
        backup,
    })
}

/// Remplace le processus courant par le nouveau binaire. `exec` ne revient
/// qu'en cas d'erreur : dans ce cas l'appelant peut restaurer la sauvegarde.
pub fn relaunch(target: &Path) -> Result<(), String> {
    use std::os::unix::process::CommandExt as _;
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let error = std::process::Command::new(target).args(args).exec();
    Err(format!("redémarrage de Quark impossible : {error}"))
}

pub fn rollback(installed: &InstalledUpdate) -> Result<(), String> {
    let _ = std::fs::remove_file(&installed.target);
    std::fs::rename(&installed.backup, &installed.target)
        .map_err(|err| format!("restauration de l'ancien binaire impossible : {err}"))
}

/// Nettoie les sauvegardes et fichiers de préparation laissés par une mise à
/// jour interrompue. Ne touche à rien si le binaire courant a disparu : la
/// sauvegarde est alors le seul moyen de restaurer Quark.
pub fn cleanup_stale() {
    let Ok(target) = std::env::current_exe() else {
        return;
    };
    if !target.is_file() {
        return;
    }
    let (Some(parent), Some(name)) = (target.parent(), target.file_name()) else {
        return;
    };
    let name = name.to_string_lossy();
    let old_prefix = format!(".{name}.old-");
    let update_prefix = format!(".{name}.update-");
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if !file_name.starts_with(&old_prefix) && !file_name.starts_with(&update_prefix) {
            continue;
        }
        // Une sauvegarde toute fraîche peut être celle de la mise à jour qui
        // vient de démarrer ce processus : on la garde au moins une minute.
        let old_enough = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age >= std::time::Duration::from_secs(60));
        if old_enough {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Une mise à jour automatique n'a de sens que pour un binaire installé hors
/// d'un dossier de compilation. `QUARK_NO_AUTO_UPDATE=1` la désactive aussi.
pub fn automatic_update_allowed() -> bool {
    if std::env::var_os("QUARK_NO_AUTO_UPDATE").is_some() {
        return false;
    }
    if cfg!(debug_assertions) {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    !exe.components()
        .any(|part| part.as_os_str() == std::ffi::OsStr::new("target"))
}

fn fetch_checksum(client: &Client, asset: &Asset) -> Result<String, String> {
    let response = client
        .get(&asset.browser_download_url)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .map_err(|err| format!("somme SHA-256 injoignable : {err}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "somme SHA-256 refusée par GitHub ({})",
            response.status().as_u16()
        ));
    }
    let text = crate::net::text(response, 4096, "somme SHA-256")?;
    let digest = text
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("somme SHA-256 illisible".to_string());
    }
    Ok(digest)
}

fn file_sha256(path: &Path) -> Result<String, String> {
    use std::io::Read as _;
    let mut file = std::fs::File::open(path).map_err(|err| err.to_string())?;
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|err| err.to_string())?;
        if read == 0 {
            break;
        }
        context.update(&buffer[..read]);
    }
    Ok(context
        .finish()
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_les_versions() {
        assert!(is_newer("v0.4.0", "0.3.0"));
        assert!(is_newer("0.3.1", "0.3.0"));
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("v0.3.0", "0.3.0"));
        assert!(!is_newer("v0.2.3", "0.3.0"));
        assert!(!is_newer("pas-une-version", "0.3.0"));
    }

    #[test]
    fn trouve_la_somme_publiee_a_cote_de_larchive() {
        let archive = Asset {
            name: "quark-1.0.9-linux-x86_64.tar.gz".to_string(),
            browser_download_url: "https://exemple/archive".to_string(),
            size: 8_000_000,
        };
        let release = Release {
            tag_name: "v1.0.9".to_string(),
            name: "Quark v1.0.9".to_string(),
            html_url: String::new(),
            assets: vec![
                archive.clone(),
                Asset {
                    name: "quark-1.0.9-linux-x86_64.tar.gz.sha256".to_string(),
                    browser_download_url: "https://exemple/sha".to_string(),
                    size: 96,
                },
            ],
        };
        assert_eq!(
            release
                .checksum_for(&archive)
                .map(|asset| asset.browser_download_url.as_str()),
            Some("https://exemple/sha")
        );
    }

    #[test]
    fn la_somme_sha256_d_un_fichier_est_calculee() {
        let path = std::env::temp_dir().join(format!(
            "quark-sha-{}-{}",
            std::process::id(),
            crate::config::now_ts()
        ));
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            file_sha256(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn remplace_le_binaire_et_restaure_en_cas_d_echec_de_relance() {
        let dir = std::env::temp_dir().join(format!("quark-update-replace-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("quark");
        let binary = dir.join(".quark.update-test");
        let archive = dir.join("quark.tar.gz");
        std::fs::write(&target, b"ancien").unwrap();
        std::fs::write(&binary, b"nouveau").unwrap();
        std::fs::write(&archive, b"archive").unwrap();
        let staged = StagedUpdate {
            target: target.clone(),
            binary,
            archive,
        };
        let installed = apply(&staged).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"nouveau");
        assert_eq!(std::fs::read(&installed.backup).unwrap(), b"ancien");
        rollback(&installed).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"ancien");
        assert!(!installed.backup.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn la_version_du_binaire_est_controlee() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!(
            "quark-update-version-{}-{}",
            std::process::id(),
            crate::config::now_ts()
        ));
        std::fs::write(&path, "#!/bin/sh\nprintf 'Quark 1.1.4\\n'\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(verify_binary_version(&path, "1.1.4").is_ok());
        assert!(verify_binary_version(&path, "1.1.3").is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn trouve_larchive_de_la_release() {
        let release = Release {
            tag_name: "v0.4.0".to_string(),
            name: "Quark v0.4.0".to_string(),
            html_url: String::new(),
            assets: vec![
                Asset {
                    name: "quark-0.4.0-linux-x86_64.tar.gz.sha256".to_string(),
                    browser_download_url: "https://exemple/sha".to_string(),
                    size: 64,
                },
                Asset {
                    name: "quark-0.4.0-linux-x86_64.tar.gz".to_string(),
                    browser_download_url: "https://exemple/archive".to_string(),
                    size: 8_000_000,
                },
            ],
        };
        assert_eq!(
            release.archive().map(|asset| asset.name.as_str()),
            Some("quark-0.4.0-linux-x86_64.tar.gz")
        );
    }
}
