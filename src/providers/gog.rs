//! Adaptateur GOG limité aux commandes réellement exposées par gogdl 1.4.0.
//!
//! gogdl n'expose pas de commande CLI de découverte/liste de bibliothèque :
//! l'adaptateur ne déduit donc jamais une bibliothèque possédée de `info`.

use super::model::CapabilityState;
use super::runner::{CommandOutput, ProviderCommand};
use super::tools::ManagedToolId;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const PINNED_VERSION: &str = "1.4.0";

/// Page d'autorisation GOG utilisée par gogdl (client public de GOG Galaxy).
/// Après la connexion, l'adresse de retour contient `?code=…`.
pub const LOGIN_URL: &str = "https://auth.gog.com/auth?client_id=46899977096215655\
&redirect_uri=https%3A%2F%2Fembed.gog.com%2Fon_login_success%3Forigin%3Dclient\
&response_type=code&layout=galaxy";

/// Fichier de jetons gogdl, séparé de `credentials.json`.
pub fn auth_config_path() -> PathBuf {
    crate::config::data_dir().join("gogdl-auth.json")
}

#[derive(Debug, Clone)]
pub struct GogAdapter {
    pub executable: PathBuf,
    pub auth_config: PathBuf,
    pub timeout: Duration,
}

impl GogAdapter {
    pub fn new(auth_config: PathBuf) -> Self {
        Self {
            executable: ManagedToolId::Gogdl.installed_path(),
            auth_config,
            timeout: Duration::from_secs(60),
        }
    }

    pub fn command(&self, args: impl IntoIterator<Item = String>) -> ProviderCommand {
        let mut complete = vec![
            "--auth-config-path".into(),
            self.auth_config.display().to_string(),
        ];
        complete.extend(args);
        ProviderCommand {
            program: PathBuf::from("python3"),
            args: std::iter::once(self.executable.display().to_string())
                .chain(complete)
                .map(Into::into)
                .collect(),
            env: vec![],
            cwd: None,
            // gogdl auth output may contain access/refresh tokens; callers that
            // perform auth must set discard_output before running this command.
            secrets: vec![],
            discard_output: false,
        }
    }

    /// Échange le code copié depuis l'adresse de retour GOG contre des jetons.
    ///
    /// gogdl imprime alors les jetons sur stdout et sort avec le code 0 même en
    /// cas de refus : la sortie est donc analysée ici, jamais affichée ni
    /// journalisée.
    pub fn login_with_code(&self, code: &str) -> Result<(), String> {
        let before = std::fs::read(&self.auth_config).ok();
        let mut command = self.command(Self::login_args(code));
        command.discard_output = true;
        command.secrets = vec![code.to_string()];
        let output = super::runner::run(command, self.timeout, None)?;
        if output.exit_code != Some(0) {
            return Err("gogdl a échoué pendant la connexion GOG".to_string());
        }
        let after = std::fs::read(&self.auth_config).ok();
        if super::session_written(before.as_deref(), after.as_deref())
            && Self::auth_config_has_tokens(&self.auth_config)
        {
            Ok(())
        } else {
            Err("gogdl a refusé l'autorisation GOG (code expiré ou invalide)".to_string())
        }
    }

    pub fn is_logged_in() -> bool {
        Self::auth_config_has_tokens(&auth_config_path())
    }

    fn auth_config_has_tokens(path: &Path) -> bool {
        std::fs::read_to_string(path)
            .map(|text| text.contains("access_token") && text.contains("refresh_token"))
            .unwrap_or(false)
    }

    pub fn run(
        &self,
        args: impl IntoIterator<Item = String>,
        cancellation: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<CommandOutput, String> {
        super::runner::run(self.command(args), self.timeout, cancellation)
    }

    pub fn sync_library(&self) -> Result<Vec<super::model::ProviderGame>, String> {
        Err("gogdl 1.4.0 ne fournit pas de commande de découverte de bibliothèque".into())
    }

    pub fn login_args(code: &str) -> Vec<String> {
        vec!["auth".into(), "--code".into(), code.into()]
    }

    pub fn library_capability() -> CapabilityState {
        CapabilityState::Unsupported(
            "gogdl 1.4.0 n'expose pas de commande CLI de liste de bibliothèque".into(),
        )
    }

    pub fn install_args(game_id: &str, destination: &Path, platform: &str) -> Vec<String> {
        vec![
            "download".into(),
            game_id.into(),
            "--path".into(),
            destination.display().to_string(),
            "--platform".into(),
            platform.into(),
        ]
    }

    pub fn update_args(game_id: &str, destination: &Path, platform: &str) -> Vec<String> {
        let mut args = Self::install_args(game_id, destination, platform);
        args[0] = "update".into();
        args
    }

    pub fn repair_args(game_id: &str, destination: &Path, platform: &str) -> Vec<String> {
        let mut args = Self::install_args(game_id, destination, platform);
        args[0] = "repair".into();
        args
    }

    pub fn launch_args(game_id: &str, install_path: &Path, platform: &str) -> Vec<String> {
        vec![
            "launch".into(),
            install_path.display().to_string(),
            game_id.into(),
            "--platform".into(),
            platform.into(),
        ]
    }

    pub fn save_sync_args(
        save_path: &Path,
        game_id: &str,
        timestamp: i64,
        platform: &str,
    ) -> Vec<String> {
        vec![
            "save-sync".into(),
            save_path.display().to_string(),
            game_id.into(),
            "--ts".into(),
            timestamp.to_string(),
            "--platform".into(),
            platform.into(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gogdl_does_not_claim_a_library_command_that_upstream_does_not_provide() {
        assert!(matches!(
            GogAdapter::library_capability(),
            CapabilityState::Unsupported(reason) if reason.contains("liste de bibliothèque")
        ));
    }

    #[test]
    fn gogdl_install_keeps_game_id_and_destination_as_separate_arguments() {
        assert_eq!(
            GogAdapter::install_args(
                "game; $(touch x)",
                Path::new("/Games With Space"),
                "windows"
            ),
            vec![
                "download",
                "game; $(touch x)",
                "--path",
                "/Games With Space",
                "--platform",
                "windows"
            ]
        );
    }

    #[test]
    fn gogdl_runs_zipapp_through_python_and_places_auth_config_first() {
        let adapter = GogAdapter::new(PathBuf::from("/private/auth config.json"));
        let command = adapter.command(GogAdapter::login_args("code"));
        assert_eq!(command.program, PathBuf::from("python3"));
        assert_eq!(command.args[0], adapter.executable.as_os_str());
        assert_eq!(command.args[1], "--auth-config-path");
        assert_eq!(command.args[2], "/private/auth config.json");
        assert_eq!(command.args[3], "auth");
        assert_eq!(command.args[4], "--code");
        assert_eq!(command.args[5], "code");
        assert!(!command.discard_output);
    }

    #[cfg(unix)]
    #[test]
    fn la_connexion_gog_exige_des_jetons_ecrits_meme_avec_un_code_de_sortie_nul() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "quark-gog-login-{}-{}",
            std::process::id(),
            crate::config::now_ts()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let auth = root.join("gogdl-auth.json");
        let adapter = GogAdapter {
            executable: root.join("gogdl.pyz"),
            auth_config: auth.clone(),
            timeout: Duration::from_secs(5),
        };

        // gogdl imprime {"error": true} et sort en 0 quand le code est refusé.
        std::fs::write(&adapter.executable, "pass\n").unwrap();
        std::fs::set_permissions(&adapter.executable, std::fs::Permissions::from_mode(0o700))
            .unwrap();
        assert!(adapter.login_with_code("code").is_err());

        std::fs::write(
            &adapter.executable,
            format!(
                "open({:?}, 'w').write('{{\"access_token\":\"a\",\"refresh_token\":\"r\"}}')\n",
                auth.display().to_string()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&adapter.executable, std::fs::Permissions::from_mode(0o700))
            .unwrap();
        assert!(adapter.login_with_code("code").is_ok());
        assert!(GogAdapter::auth_config_has_tokens(&auth));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn la_page_de_connexion_gog_est_l_url_officielle_du_client_public() {
        assert!(LOGIN_URL.starts_with("https://auth.gog.com/auth?client_id=46899977096215655"));
        assert!(LOGIN_URL.contains("response_type=code"));
    }
}
