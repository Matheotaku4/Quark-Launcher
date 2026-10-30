//! Téléchargement vérifié des exécutables officiels gérés par Quark.
//!
//! Les digests viennent des métadonnées GitHub Releases : ils protègent
//! l'intégrité de transfert, mais ne sont pas une signature indépendante.

use reqwest::blocking::Client;
use serde::Deserialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ManagedToolId {
    Legendary,
    Gogdl,
    Ludusavi,
}

impl ManagedToolId {
    pub const ALL: [Self; 3] = [Self::Legendary, Self::Gogdl, Self::Ludusavi];

    pub fn label(self) -> &'static str {
        match self {
            Self::Legendary => "Legendary · Epic Games",
            Self::Gogdl => "gogdl · GOG",
            Self::Ludusavi => "Ludusavi · sauvegardes locales",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::Legendary => 0,
            Self::Gogdl => 1,
            Self::Ludusavi => 2,
        }
    }

    pub fn installed_path(self) -> PathBuf {
        crate::config::data_dir()
            .join("tools")
            .join(self.installed_name())
    }

    fn repo(self) -> &'static str {
        match self {
            Self::Legendary => "legendary-gl/legendary",
            Self::Gogdl => "Heroic-Games-Launcher/heroic-gogdl",
            Self::Ludusavi => "mtkennerly/ludusavi",
        }
    }

    fn asset_name(self, tag: &str) -> Option<String> {
        match self {
            Self::Legendary => Some("legendary_linux_x64".to_string()),
            Self::Gogdl => Some("gogdl_linux_x86_64".to_string()),
            Self::Ludusavi => {
                let version = tag.strip_prefix('v').unwrap_or(tag);
                Some(format!("ludusavi-v{version}-linux.tar.gz"))
            }
        }
    }

    fn installed_name(self) -> &'static str {
        match self {
            Self::Legendary => "legendary",
            Self::Gogdl => "gogdl.pyz",
            Self::Ludusavi => "ludusavi",
        }
    }

    fn is_python_zipapp(self) -> bool {
        self == Self::Gogdl
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub release_tag: String,
}

impl ReleaseAsset {
    pub fn is_official(&self, tool: ManagedToolId) -> bool {
        let prefix = format!("https://github.com/{}/releases/download/", tool.repo());
        let expected = tool.asset_name(&self.release_tag);
        let suffix = self.url.strip_prefix(&prefix);
        let expected_tag = self.release_tag.as_str();
        suffix.is_some_and(|suffix| {
            let mut segments = suffix.split('/');
            let tag = segments.next().unwrap_or_default();
            let asset = segments.next().unwrap_or_default();
            tag == expected_tag
                && !asset.is_empty()
                && segments.next().is_none()
                && expected.as_deref() == Some(self.name.as_str())
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRelease {
    pub tag: String,
    assets: Vec<ReleaseAsset>,
}

impl ToolRelease {
    pub fn asset(&self, tool: ManagedToolId) -> Result<&ReleaseAsset, String> {
        self.assets
            .iter()
            .find(|asset| asset.is_official(tool))
            .ok_or_else(|| format!("asset Linux x86_64 vérifié indisponible pour {tool:?}"))
    }
}

#[derive(Deserialize)]
struct RawRelease {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

#[derive(Deserialize)]
struct RawAsset {
    #[serde(default)]
    name: String,
    #[serde(default)]
    browser_download_url: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

pub fn parse_release(json: &str) -> Result<ToolRelease, String> {
    let raw: RawRelease =
        serde_json::from_str(json).map_err(|_| "métadonnées GitHub invalides".to_string())?;
    if raw.tag_name.is_empty() {
        return Err("la release GitHub ne contient pas de version".to_string());
    }
    let release_tag = raw.tag_name.clone();
    let assets = raw
        .assets
        .into_iter()
        .filter_map(|asset| {
            let sha256 = asset.digest?.strip_prefix("sha256:")?.to_ascii_lowercase();
            if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return None;
            }
            Some(ReleaseAsset {
                name: asset.name,
                url: asset.browser_download_url,
                size: asset.size,
                sha256,
                release_tag: release_tag.clone(),
            })
        })
        .collect();
    Ok(ToolRelease {
        tag: raw.tag_name,
        assets,
    })
}

pub fn latest_release(client: &Client, tool: ManagedToolId) -> Result<ToolRelease, String> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        tool.repo()
    );
    let response = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .map_err(|_| "connexion aux releases officielles impossible".to_string())?;
    if !response.status().is_success() {
        return Err(format!("GitHub a répondu {}", response.status().as_u16()));
    }
    let bytes = read_bounded(response, 2 * 1024 * 1024)
        .map_err(|_| "réponse de release trop volumineuse ou illisible".to_string())?;
    parse_release(
        std::str::from_utf8(&bytes).map_err(|_| "réponse de release non UTF-8".to_string())?,
    )
}

pub fn verify_digest(bytes: &[u8], expected_hex: &str) -> Result<(), String> {
    let expected =
        decode_hex(expected_hex).ok_or_else(|| "digest SHA-256 mal formé".to_string())?;
    let actual = ring::digest::digest(&ring::digest::SHA256, bytes);
    if constant_time_equal(actual.as_ref(), &expected) {
        Ok(())
    } else {
        Err("le digest SHA-256 de l'outil ne correspond pas".to_string())
    }
}

fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    if hex.len() != 64 {
        return None;
    }
    let mut bytes = Vec::with_capacity(32);
    for pair in hex.as_bytes().chunks_exact(2) {
        let hi = (pair[0] as char).to_digit(16)? as u8;
        let lo = (pair[1] as char).to_digit(16)? as u8;
        bytes.push((hi << 4) | lo);
    }
    Some(bytes)
}

fn read_bounded(mut reader: impl Read, max_bytes: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| "lecture de la réponse impossible".to_string())?;
    if bytes.len() as u64 > max_bytes {
        return Err("réponse trop volumineuse".to_string());
    }
    Ok(bytes)
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0u8, |different, (a, b)| different | (a ^ b))
            == 0
}

pub fn install_verified(
    client: &Client,
    tool: ManagedToolId,
    asset: &ReleaseAsset,
    destination_dir: &Path,
) -> Result<PathBuf, String> {
    if !asset.is_official(tool) || asset.size == 0 || asset.size > MAX_ASSET_BYTES {
        return Err("asset non officiel ou taille invalide".to_string());
    }
    std::fs::create_dir_all(destination_dir)
        .map_err(|_| "impossible de créer le dossier des outils".to_string())?;
    let response = client
        .get(&asset.url)
        .send()
        .map_err(|_| "téléchargement de l'outil impossible".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "téléchargement refusé ({})",
            response.status().as_u16()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_ASSET_BYTES)
    {
        return Err("outil téléchargé au-delà de la taille maximale".to_string());
    }
    let mut bytes = Vec::with_capacity(asset.size.min(MAX_ASSET_BYTES) as usize);
    response
        .take(MAX_ASSET_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "téléchargement interrompu".to_string())?;
    if bytes.len() as u64 != asset.size || bytes.len() as u64 > MAX_ASSET_BYTES {
        return Err("taille téléchargée différente de la release".to_string());
    }
    verify_digest(&bytes, &asset.sha256)?;

    let executable = if tool == ManagedToolId::Ludusavi {
        decompress_ludusavi(&bytes)?
    } else {
        bytes
    };
    if !tool.is_python_zipapp() && tool != ManagedToolId::Ludusavi {
        verify_elf_x86_64(&executable)?;
    }
    if tool.is_python_zipapp() && !executable.starts_with(b"#!/usr/bin/env python3\n") {
        return Err("gogdl n'est pas le zipapp Python officiel attendu".to_string());
    }
    let temp = destination_dir.join(format!(
        ".{}.{}-{}.tmp",
        tool.installed_name(),
        std::process::id(),
        asset.sha256.get(..12).unwrap_or("digest")
    ));
    let target = destination_dir.join(tool.installed_name());
    let result = write_and_verify(&temp, tool, &executable, &asset.release_tag);
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result?;
    std::fs::rename(&temp, &target)
        .map_err(|_| "impossible de remplacer atomiquement l'outil".to_string())?;
    Ok(target)
}

fn decompress_ludusavi(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let decoder = flate2::read::GzDecoder::new(bytes);
    let mut tar_bytes = Vec::new();
    decoder
        .take(MAX_ASSET_BYTES + 1)
        .read_to_end(&mut tar_bytes)
        .map_err(|_| "archive gzip Ludusavi invalide".to_string())?;
    if tar_bytes.len() as u64 > MAX_ASSET_BYTES {
        return Err("archive décompressée trop volumineuse".to_string());
    }
    extract_ludusavi(&tar_bytes)
}

fn extract_ludusavi(bytes: &[u8]) -> Result<Vec<u8>, String> {
    // Parse tar headers in memory; refuse paths other than the expected binary.
    let mut offset = 0usize;
    while offset + 512 <= bytes.len() {
        let header = &bytes[offset..offset + 512];
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        let name_end = header[..100]
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(100);
        let size_text = std::str::from_utf8(&header[124..136])
            .map_err(|_| "archive Ludusavi invalide".to_string())?
            .trim_matches(|ch| ch == '\0' || ch == ' ');
        let size =
            usize::from_str_radix(size_text, 8).map_err(|_| "taille tar invalide".to_string())?;
        let body = offset + 512;
        let end = body
            .checked_add(size)
            .ok_or_else(|| "archive tar invalide".to_string())?;
        if end > bytes.len() {
            return Err("archive tar tronquée".to_string());
        }
        let entry_type = header[156];
        let stored_checksum = std::str::from_utf8(&header[148..156])
            .ok()
            .and_then(|text| {
                usize::from_str_radix(text.trim_matches(|ch| ch == '\0' || ch == ' '), 8).ok()
            })
            .ok_or_else(|| "checksum tar invalide".to_string())?;
        let actual_checksum: usize = header
            .iter()
            .enumerate()
            .map(|(index, byte)| {
                if (148..156).contains(&index) {
                    b' ' as usize
                } else {
                    *byte as usize
                }
            })
            .sum();
        if stored_checksum != actual_checksum {
            return Err("checksum tar invalide".to_string());
        }
        let prefix_end = header[345..500]
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(155);
        let mut full_name = header[345..345 + prefix_end].to_vec();
        if !full_name.is_empty() {
            full_name.push(b'/');
        }
        full_name.extend_from_slice(&header[..name_end]);
        let full_name =
            std::str::from_utf8(&full_name).map_err(|_| "archive Ludusavi invalide".to_string())?;
        if full_name.starts_with('/') || full_name.split('/').any(|part| part == "..") {
            return Err("chemin dangereux dans l'archive Ludusavi".to_string());
        }
        if (entry_type == 0 || entry_type == b'0') && full_name == "ludusavi" {
            return Ok(bytes[body..end].to_vec());
        }
        offset = body + size.div_ceil(512) * 512;
    }
    Err("binaire ludusavi absent de l'archive officielle".to_string())
}

fn verify_elf_x86_64(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 20
        || &bytes[..4] != b"\x7fELF"
        || bytes[4] != 2
        || bytes[5] != 1
        || bytes[18..20] != [0x3e, 0x00]
    {
        return Err("l'outil téléchargé n'est pas un exécutable ELF x86_64".to_string());
    }
    Ok(())
}

fn write_and_verify(
    temp: &Path,
    tool: ManagedToolId,
    bytes: &[u8],
    expected_tag: &str,
) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp)
        .map_err(|_| "impossible de créer le binaire temporaire".to_string())?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| "impossible d'écrire le binaire téléchargé".to_string())?;
    drop(file);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = 0o700;
        std::fs::set_permissions(temp, std::fs::Permissions::from_mode(mode))
            .map_err(|_| "impossible de définir les droits de l'outil".to_string())?;
    }
    inspect_version(temp, tool, expected_tag)
}

fn command_for_tool(tool: ManagedToolId, path: &Path) -> (PathBuf, Vec<String>) {
    if tool.is_python_zipapp() {
        (
            PathBuf::from("python3"),
            vec![path.display().to_string(), "--version".to_string()],
        )
    } else {
        (path.to_path_buf(), vec!["--version".to_string()])
    }
}

fn inspect_version(path: &Path, tool: ManagedToolId, expected_tag: &str) -> Result<(), String> {
    let (program, args) = command_for_tool(tool, path);
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|_| "l'outil validé ne démarre pas".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "sortie version indisponible".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "sortie version indisponible".to_string())?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut output = Vec::new();
        let mut stdout = stdout;
        let mut stderr = stderr;
        let result = stdout
            .read_to_end(&mut output)
            .and_then(|_| stderr.read_to_end(&mut output))
            .map(|_| output);
        let _ = tx.send(result);
    });
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|_| "la vérification de version de l'outil a échoué".to_string())?
        {
            break status;
        }
        if started.elapsed() >= VERSION_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err("l'outil dépasse le délai de vérification de version".to_string());
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let output = rx
        .recv_timeout(VERSION_TIMEOUT)
        .map_err(|_| "sortie version bloquée".to_string())?
        .map_err(|_| "sortie de version illisible".to_string())?;
    let output = String::from_utf8_lossy(&output).to_ascii_lowercase();
    let expected = expected_tag
        .trim_start_matches(['v', 'V'])
        .to_ascii_lowercase();
    let has_exact_version = output
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '.')
        .any(|token| token.trim_start_matches(['v', 'V']) == expected);
    if !status.success() || expected.is_empty() || !has_exact_version {
        return Err("la version du binaire ne correspond pas à la release".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_zipapp_gogdl_utilise_python_meme_pour_le_fichier_temporaire() {
        let temporary = Path::new("/tools/.gogdl.123-digest.tmp");
        let (program, args) = command_for_tool(ManagedToolId::Gogdl, temporary);
        assert_eq!(program, PathBuf::from("python3"));
        assert_eq!(
            args,
            vec![temporary.display().to_string(), "--version".to_string()]
        );
    }

    #[test]
    fn les_metadonnees_de_release_sont_lues_sous_un_plafond() {
        assert_eq!(read_bounded(&b"petit"[..], 5).unwrap(), b"petit");
        assert!(read_bounded(&b"trop long"[..], 5).is_err());
    }

    #[test]
    fn gogdl_est_un_zipapp_lance_via_python_sans_shell() {
        let (program, args) = command_for_tool(ManagedToolId::Gogdl, Path::new("/tools/gogdl.pyz"));
        assert_eq!(program, PathBuf::from("python3"));
        assert_eq!(args, vec!["/tools/gogdl.pyz", "--version"]);
    }

    #[test]
    fn release_github_selectionne_les_assets_linux_x86_64_exacts() {
        let json = r#"{
            "tag_name": "0.21.1",
            "assets": [
                {"name":"legendary_linux_arm64","browser_download_url":"https://github.com/legendary-gl/legendary/releases/download/0.21.1/legendary_linux_arm64","size":5,"digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000"},
                {"name":"legendary_linux_x64","browser_download_url":"https://github.com/legendary-gl/legendary/releases/download/0.21.1/legendary_linux_x64","size":6,"digest":"sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}
            ]
        }"#;
        let release = parse_release(json).unwrap();
        let asset = release.asset(ManagedToolId::Legendary).unwrap();
        assert_eq!(asset.name, "legendary_linux_x64");
        assert_eq!(
            asset.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn une_release_sans_digest_est_refusee() {
        let json = r#"{"tag_name":"v1","assets":[{"name":"gogdl_linux_x86_64","browser_download_url":"https://github.com/Heroic-Games-Launcher/heroic-gogdl/releases/download/v1/gogdl_linux_x86_64","size":9}]}"#;
        let release = parse_release(json).unwrap();
        assert!(release.asset(ManagedToolId::Gogdl).is_err());
    }

    #[test]
    fn le_digest_sha256_est_verifie_sur_les_octets() {
        let correct = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify_digest(b"abc", correct).is_ok());
        assert!(verify_digest(b"abd", correct).is_err());
    }
    #[test]
    fn les_tags_v_prefixes_sont_conserves_dans_les_urls_officielles() {
        let asset = ReleaseAsset {
            name: "gogdl_linux_x86_64".to_string(),
            url: "https://github.com/Heroic-Games-Launcher/heroic-gogdl/releases/download/v1.4.0/gogdl_linux_x86_64".to_string(),
            size: 1,
            sha256: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            release_tag: "v1.4.0".to_string(),
        };
        assert!(asset.is_official(ManagedToolId::Gogdl));
    }

    #[test]
    fn les_adresses_d_assets_sont_limitees_aux_releases_officielles() {
        let asset = ReleaseAsset {
            name: "legendary_linux_x64".to_string(),
            url: "https://example.com/tool".to_string(),
            size: 3,
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_string(),
            release_tag: "1".to_string(),
        };
        assert!(!asset.is_official(ManagedToolId::Legendary));
        let asset = ReleaseAsset {
            url:
                "https://github.com/legendary-gl/legendary/releases/download/1/legendary_linux_x64"
                    .to_string(),
            ..asset
        };
        assert!(asset.is_official(ManagedToolId::Legendary));
        let wrong_tag = ReleaseAsset {
            url:
                "https://github.com/legendary-gl/legendary/releases/download/2/legendary_linux_x64"
                    .to_string(),
            ..asset
        };
        assert!(!wrong_tag.is_official(ManagedToolId::Legendary));
    }

    #[test]
    fn les_binaires_arm_ou_non_elf_sont_refuses() {
        let mut arm = vec![0u8; 20];
        arm[..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
        arm[4] = 2;
        arm[5] = 1;
        arm[18..20].copy_from_slice(&[0xb7, 0x00]);
        assert!(verify_elf_x86_64(&arm).is_err());
        assert!(verify_elf_x86_64(b"not an ELF").is_err());
    }

    #[test]
    fn une_archive_ludusavi_invalide_est_rejetee() {
        let mut header = [0u8; 512];
        header[..8].copy_from_slice(b"ludusavi");
        header[148..156].fill(b'0');
        header[156] = b'0';
        assert!(extract_ludusavi(&header).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn une_version_plus_longue_ne_correspond_pas_au_tag_attendu() {
        use std::os::unix::fs::PermissionsExt;
        let path =
            std::env::temp_dir().join(format!("quark-tool-prefix-version-{}", std::process::id()));
        std::fs::write(&path, "#!/bin/sh\necho Legendary 0.21.10\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(inspect_version(&path, ManagedToolId::Legendary, "0.21.1").is_err());
        let _ = std::fs::remove_file(path);
    }

    #[cfg(unix)]
    #[test]
    fn un_binaire_de_mauvaise_version_ne_remplace_pas_l_installation_existante() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("quark-tool-version-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let old = root.join("legendary");
        std::fs::write(&old, b"previous working binary").unwrap();
        let candidate = root.join("candidate");
        std::fs::write(&candidate, "#!/bin/sh\\necho Legendary 9.0\\n").unwrap();
        std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o700)).unwrap();

        assert!(inspect_version(&candidate, ManagedToolId::Legendary, "0.21.1").is_err());
        assert_eq!(std::fs::read(&old).unwrap(), b"previous working binary");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn archive_tar_valide_ne_restitue_que_le_binaire_attendu() {
        let mut archive = vec![0u8; 1024];
        let header = &mut archive[..512];
        header[..8].copy_from_slice(b"ludusavi");
        header[100..108].fill(b'0');
        header[106] = b'7';
        header[124..136].fill(0);
        header[134] = b'4';
        header[156] = b'0';
        header[148..156].fill(b' ');
        let checksum: usize = header.iter().map(|byte| *byte as usize).sum();
        let encoded = format!("{checksum:06o}");
        header[148..154].copy_from_slice(encoded.as_bytes());
        header[154] = 0;
        header[155] = b' ';
        archive[512..516].copy_from_slice(b"ELF!");
        assert_eq!(extract_ludusavi(&archive).unwrap(), b"ELF!");
    }
}
