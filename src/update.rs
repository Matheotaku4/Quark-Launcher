//! Mises à jour du launcher : dernière release GitHub, comparaison de versions
//! et téléchargement de l'archive.
//!
//! Rien n'est installé à la place du binaire courant : l'archive est
//! téléchargée dans `~/.local/share/quark/updates/` et l'utilisateur décide.

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
