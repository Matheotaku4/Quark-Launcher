//! Mises à jour du launcher : dernière release GitHub, comparaison de versions
//! et téléchargement de l'archive.
//!
//! Rien n'est installé à la place du binaire courant : l'archive est
//! téléchargée dans `~/.local/share/quark/updates/` et l'utilisateur décide.

use reqwest::blocking::Client;
use serde::Deserialize;
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
    response
        .json::<Release>()
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

/// Télécharge une archive de release dans `dir` et renvoie son chemin.
pub fn download(client: &Client, asset: &Asset, dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    let mut response = client
        .get(&asset.browser_download_url)
        .send()
        .map_err(|err| format!("réseau : {err}"))?;
    if !response.status().is_success() {
        return Err(format!("GitHub a répondu {}", response.status().as_u16()));
    }
    // Nom de fichier de l'archive seule, jamais le chemin fourni par le serveur.
    let name = asset
        .name
        .rsplit('/')
        .next()
        .unwrap_or("quark.tar.gz")
        .to_string();
    let path = dir.join(name);
    let mut file = std::fs::File::create(&path).map_err(|err| err.to_string())?;
    std::io::copy(&mut response, &mut file).map_err(|err| err.to_string())?;
    Ok(path)
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
