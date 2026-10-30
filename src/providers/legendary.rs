//! Adaptateur Epic Games fondé sur le CLI Legendary (0.21.1).

use super::model::{CapabilityState, InstallState, Provider, ProviderGame, ProviderGameKey};
use super::runner::{CommandOutput, ProviderCommand};
use super::tools::ManagedToolId;
use std::path::PathBuf;
use std::time::Duration;

pub const PINNED_VERSION: &str = "0.21.1";

#[derive(Debug, Clone)]
pub struct LegendaryAdapter {
    pub executable: PathBuf,
    pub timeout: Duration,
}

impl LegendaryAdapter {
    pub fn new() -> Self {
        Self {
            executable: ManagedToolId::Legendary.installed_path(),
            timeout: Duration::from_secs(60),
        }
    }

    pub fn command(&self, args: impl IntoIterator<Item = String>) -> ProviderCommand {
        ProviderCommand {
            program: self.executable.clone(),
            args: args.into_iter().map(Into::into).collect(),
            env: vec![],
            cwd: None,
            secrets: vec![],
            discard_output: false,
        }
    }

    pub fn run(
        &self,
        args: impl IntoIterator<Item = String>,
        cancellation: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<CommandOutput, String> {
        super::runner::run(self.command(args), self.timeout, cancellation)
    }

    pub fn sync_owned(
        &self,
        fetched_at: i64,
        cancellation: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<Vec<ProviderGame>, String> {
        let output = self.run(Self::library_args(), cancellation)?;
        self.owned_json(&output, fetched_at)
    }

    pub fn sync_installed(
        &self,
        fetched_at: i64,
        cancellation: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<Vec<ProviderGame>, String> {
        let output = self.run(Self::installed_args(), cancellation)?;
        self.installed_json(&output, fetched_at)
    }

    pub fn login_status_args() -> Vec<String> {
        vec!["status".into(), "--json".into()]
    }
    pub fn library_args() -> Vec<String> {
        vec!["list".into(), "--json".into()]
    }
    pub fn installed_args() -> Vec<String> {
        vec!["list-installed".into(), "--json".into()]
    }

    pub fn install_args(app_name: &str, root: &std::path::Path) -> Vec<String> {
        vec![
            "install".into(),
            app_name.into(),
            "--base-path".into(),
            root.display().to_string(),
        ]
    }

    pub fn update_args(app_name: &str) -> Vec<String> {
        vec!["update".into(), app_name.into()]
    }
    pub fn repair_args(app_name: &str) -> Vec<String> {
        vec!["repair".into(), app_name.into()]
    }
    pub fn launch_args(app_name: &str) -> Vec<String> {
        vec!["launch".into(), app_name.into()]
    }
    pub fn achievements_args(app_name: &str) -> Vec<String> {
        vec!["achievements".into(), app_name.into(), "--json".into()]
    }
    pub fn cloud_saves_args(app_name: &str) -> Vec<String> {
        vec!["sync-saves".into(), app_name.into()]
    }

    pub fn owned_json(
        &self,
        output: &CommandOutput,
        fetched_at: i64,
    ) -> Result<Vec<ProviderGame>, String> {
        ensure_success(output)?;
        parse_owned_json(&output.stdout, fetched_at)
    }

    pub fn installed_json(
        &self,
        output: &CommandOutput,
        fetched_at: i64,
    ) -> Result<Vec<ProviderGame>, String> {
        ensure_success(output)?;
        parse_installed_json(&output.stdout, fetched_at)
    }

    pub fn login_state(&self, _output: &CommandOutput) -> LoginState {
        // status --json output fields are not pinned/documented sufficiently to parse safely.
        LoginState::Unknown
    }

    pub fn supports_achievements(&self) -> CapabilityState {
        CapabilityState::Unknown
    }

    pub fn supports_cloud_saves(&self) -> CapabilityState {
        CapabilityState::Unknown
    }
}

impl Default for LegendaryAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginState {
    Connected,
    SignedOut,
    Unknown,
}

pub fn parse_owned_json(json: &str, fetched_at: i64) -> Result<Vec<ProviderGame>, String> {
    #[derive(serde::Deserialize)]
    struct EpicTitle {
        app_name: String,
        app_title: String,
    }
    let entries: Vec<EpicTitle> =
        serde_json::from_str(json).map_err(|_| "sortie JSON Legendary invalide".to_string())?;
    if entries.is_empty() {
        return Err("bibliothèque Epic vide ou compte Legendary indisponible; données précédentes conservées".into());
    }
    let mut seen = std::collections::HashSet::new();
    let mut games = Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.app_name.is_empty() || entry.app_title.trim().is_empty() {
            return Err("entrée de jeu Legendary incomplète".to_string());
        }
        if !seen.insert(entry.app_name.clone()) {
            continue;
        }
        games.push(ProviderGame {
            key: ProviderGameKey {
                provider: Provider::Epic,
                product_id: entry.app_name,
            },
            title: entry.app_title,
            owned: true,
            install_path: None,
            install_state: InstallState::NotInstalled,
            quark_game_id: None,
            capabilities: Default::default(),
            last_synced: Some(fetched_at),
        });
    }
    Ok(games)
}

pub fn parse_installed_json(json: &str, fetched_at: i64) -> Result<Vec<ProviderGame>, String> {
    #[derive(serde::Deserialize)]
    struct InstalledTitle {
        app_name: String,
        title: String,
        install_path: PathBuf,
    }
    let entries: Vec<InstalledTitle> = serde_json::from_str(json)
        .map_err(|_| "sortie JSON d'installations Legendary invalide".to_string())?;
    let mut seen = std::collections::HashSet::new();
    let mut games = Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.app_name.is_empty()
            || entry.title.trim().is_empty()
            || !entry.install_path.is_absolute()
        {
            return Err("entrée d'installation Legendary invalide".to_string());
        }
        if !seen.insert(entry.app_name.clone()) {
            continue;
        }
        games.push(ProviderGame {
            key: ProviderGameKey {
                provider: Provider::Epic,
                product_id: entry.app_name,
            },
            title: entry.title,
            owned: true,
            install_path: Some(entry.install_path),
            install_state: InstallState::Installed,
            quark_game_id: None,
            capabilities: Default::default(),
            last_synced: Some(fetched_at),
        });
    }
    Ok(games)
}

pub fn merge_epic(existing: &[ProviderGame], refreshed_epic: &[ProviderGame]) -> Vec<ProviderGame> {
    existing
        .iter()
        .filter(|game| game.key.provider != Provider::Epic)
        .cloned()
        .chain(refreshed_epic.iter().cloned())
        .collect()
}

fn ensure_success(output: &CommandOutput) -> Result<(), String> {
    if output.exit_code == Some(0) {
        Ok(())
    } else {
        Err("commande Legendary échouée; bibliothèque connue conservée".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legendary_list_json_maps_owned_games_without_using_titles_as_install_paths() {
        let json = r#"[{"app_name":"app_123","app_title":"Example Game","metadata":{"mainGameAppName":null},"asset_infos":{},"base_urls":[],"sidecar":null,"achievements":null,"dlcs":[]}]"#;
        let games = parse_owned_json(json, 1234).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].key.provider, Provider::Epic);
        assert_eq!(games[0].key.product_id, "app_123");
        assert_eq!(games[0].title, "Example Game");
        assert!(games[0].install_path.is_none());
        assert_eq!(games[0].install_state, InstallState::NotInstalled);
        assert_eq!(games[0].last_synced, Some(1234));
    }

    #[test]
    fn malformed_or_ambiguous_legendary_output_is_not_an_empty_success() {
        assert!(parse_owned_json("not json", 1).is_err());
        assert!(parse_owned_json("{}", 1).is_err());
        assert!(parse_owned_json("[]", 1).is_err());
    }

    #[test]
    fn legendary_installed_json_keeps_provider_path_and_matches_app_name() {
        let json = r#"[{"app_name":"app_123","title":"Example Game","install_path":"/games/example","version":"1","base_urls":[],"can_run_offline":true,"egl_guid":"x","executable":"game.exe","install_size":42,"install_tags":[],"is_dlc":false,"launch_parameters":[],"manifest_path":"/games/example/manifest","needs_verification":false,"platform":"Windows","prereq_info":{},"uninstaller":null,"requires_ot":false,"save_path":"/games/example/save","is_preloaded":false}]"#;
        let games = parse_installed_json(json, 1234).unwrap();
        assert_eq!(games[0].key.product_id, "app_123");
        assert_eq!(
            games[0].install_path.as_deref(),
            Some(std::path::Path::new("/games/example"))
        );
        assert_eq!(games[0].install_state, InstallState::Installed);
    }

    #[test]
    fn failed_legendary_command_never_becomes_an_empty_owned_library() {
        let adapter = LegendaryAdapter::new();
        let output = CommandOutput {
            stdout: "[]".into(),
            stderr: "private".into(),
            exit_code: Some(1),
        };
        assert!(adapter.owned_json(&output, 1).is_err());
    }

    #[test]
    fn epic_refresh_replaces_only_epic_records_and_preserves_other_providers() {
        use super::super::model::{Provider, ProviderGameKey};
        let steam = ProviderGame {
            key: ProviderGameKey {
                provider: Provider::Steam,
                product_id: "620".into(),
            },
            title: "Portal 2".into(),
            owned: true,
            install_path: None,
            install_state: InstallState::Unknown,
            quark_game_id: None,
            capabilities: Default::default(),
            last_synced: None,
        };
        let previous_epic = ProviderGame {
            key: ProviderGameKey {
                provider: Provider::Epic,
                product_id: "old".into(),
            },
            title: "Old".into(),
            owned: true,
            install_path: None,
            install_state: InstallState::Unknown,
            quark_game_id: None,
            capabilities: Default::default(),
            last_synced: None,
        };
        let fresh = parse_owned_json(r#"[{"app_name":"new","app_title":"New Game"}]"#, 5).unwrap();
        let merged = merge_epic(&[steam.clone(), previous_epic], &fresh);
        assert_eq!(merged.len(), 2);
        assert!(merged.iter().any(|game| game.key == steam.key));
        assert!(merged.iter().any(|game| game.key.product_id == "new"));
        assert!(!merged.iter().any(|game| game.key.product_id == "old"));
    }

    #[test]
    fn legendary_commands_pass_title_and_root_as_separate_arguments() {
        assert_eq!(
            LegendaryAdapter::install_args(
                "Game; $(touch x)",
                PathBuf::from("/Games With Space").as_path()
            ),
            vec![
                "install",
                "Game; $(touch x)",
                "--base-path",
                "/Games With Space"
            ]
        );
    }
}
