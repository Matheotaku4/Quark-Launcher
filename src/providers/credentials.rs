//! Stockage local des identifiants de boutiques.
//!
//! Ce fichier est protégé par les permissions du compte (0600 sous Unix), mais
//! n'est pas chiffré. Les erreurs ne reprennent jamais son contenu.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProviderTokens {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Credentials {
    pub steam_web_api_key: Option<String>,
    pub steam_id: Option<String>,
    pub epic: Option<ProviderTokens>,
    pub gog: Option<ProviderTokens>,
}

/// Lit les identifiants. Un fichier absent signifie que les boutiques sont
/// déconnectées ; un fichier invalide est signalé et laissé intact.
pub fn load(path: &Path) -> Result<Credentials, String> {
    #[cfg(unix)]
    if path.exists() {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|_| "impossible de protéger le fichier local des identifiants".to_string())?;
    }
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Credentials::default());
        }
        Err(_) => return Err("impossible de lire le fichier local des identifiants".to_string()),
    };
    serde_json::from_str(&text)
        .map_err(|_| "le fichier local des identifiants est invalide".to_string())
}

/// Écrit de façon atomique et avec les droits utilisateur uniquement.
pub fn save_atomic(path: &Path, credentials: &Credentials) -> Result<(), String> {
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|_| "impossible de créer le dossier des identifiants".to_string())?;
    let bytes = serde_json::to_vec_pretty(credentials)
        .map_err(|_| "impossible de sérialiser les identifiants".to_string())?;

    let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(
        ".quark-credentials-{}-{nonce}.tmp",
        std::process::id()
    ));
    let result = write_temp_and_replace(&temp, path, &bytes);
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn write_temp_and_replace(temp: &Path, destination: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;

    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(temp)
            .map_err(|_| "impossible de créer le fichier temporaire des identifiants".to_string())?
    };
    #[cfg(not(unix))]
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp)
        .map_err(|_| "impossible de créer le fichier temporaire des identifiants".to_string())?;

    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| "impossible d'écrire le fichier local des identifiants".to_string())?;
    drop(file);
    std::fs::rename(temp, destination)
        .map_err(|_| "impossible de remplacer atomiquement les identifiants".to_string())?;

    #[cfg(unix)]
    if let Some(parent) = destination.parent() {
        if let Ok(directory) = std::fs::File::open(parent) {
            let _ = directory.sync_all();
        }
    }
    Ok(())
}

/// Remplace les valeurs de secrets connues par un marqueur de journalisation.
pub fn redact_secrets(text: &str, secrets: &[String]) -> String {
    let mut ordered: Vec<&str> = secrets
        .iter()
        .map(String::as_str)
        .filter(|secret| !secret.is_empty())
        .collect();
    ordered.sort_by_key(|secret| std::cmp::Reverse(secret.len()));
    ordered
        .into_iter()
        .fold(text.to_string(), |redacted, secret| {
            redacted.replace(secret, "[REDACTED]")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "quark-credentials-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn fichier_absent_est_un_compte_deconnecte() {
        let path = temp_path("missing");
        let credentials = load(&path).unwrap();
        assert_eq!(credentials, Credentials::default());
        assert!(!path.exists());
    }

    #[test]
    fn credentials_sont_ecrits_en_json_prive_et_recharges() {
        let path = temp_path("roundtrip");
        let credentials = Credentials {
            steam_web_api_key: Some("steam-secret".to_string()),
            steam_id: Some("76561198000000000".to_string()),
            epic: Some(ProviderTokens {
                access_token: "epic-access".to_string(),
                refresh_token: "epic-refresh".to_string(),
            }),
            gog: None,
        };

        save_atomic(&path, &credentials).unwrap();
        assert_eq!(load(&path).unwrap(), credentials);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn json_invalide_est_signale_sans_remplacer_le_fichier() {
        let path = temp_path("invalid");
        std::fs::write(&path, b"not json").unwrap();
        assert!(load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"not json");
        let _ = std::fs::remove_file(path);
    }

    #[cfg(unix)]
    #[test]
    fn un_fichier_existant_trop_ouvert_est_restreint_avant_lecture() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_path("permissions");
        std::fs::write(&path, b"{} ").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        load(&path).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn les_jetons_et_cles_sont_rediges_des_journaux() {
        let message = "authorization_code=abc steam-key=def epic=ghi";
        let redacted = redact_secrets(
            message,
            &["abc".into(), "def".into(), "ghi".into(), "ab".into()],
        );
        assert_eq!(
            redacted,
            "authorization_code=[REDACTED] steam-key=[REDACTED] epic=[REDACTED]"
        );
    }

    #[test]
    fn le_secret_le_plus_long_est_redige_en_premier() {
        assert_eq!(
            redact_secrets("token=abcd", &["abc".into(), "abcd".into()]),
            "token=[REDACTED]"
        );
    }
}
