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

    pub fn login_command(&self) -> ProviderCommand {
        let mut command = self.command(Self::login_args());
        command.discard_output = true;
        command
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

    pub fn login_args() -> Vec<String> {
        vec!["auth".into()]
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
        let command = adapter.login_command();
        assert_eq!(command.program, PathBuf::from("python3"));
        assert_eq!(command.args[0], adapter.executable.as_os_str());
        assert_eq!(command.args[1], "--auth-config-path");
        assert_eq!(command.args[2], "/private/auth config.json");
        assert_eq!(command.args[3], "auth");
        assert!(command.discard_output);
    }
}
