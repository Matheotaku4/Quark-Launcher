//! Adaptateur Epic Games fondé sur le CLI Legendary (0.21.1).

use super::model::{CapabilityState, InstallState, Provider, ProviderGame, ProviderGameKey};
use super::runner::{CommandOutput, ProviderCommand};
use super::tools::ManagedToolId;
use std::path::PathBuf;
use std::time::Duration;

pub const PINNED_VERSION: &str = "0.21.1";

/// Page de connexion manuelle de `legendary auth --disable-webview` : elle
/// affiche la réponse JSON dont il faut copier la valeur `authorizationCode`.
pub const LOGIN_URL: &str = "https://legendary.gl/epiclogin";

/// Une installation télécharge plusieurs gigaoctets : le délai des commandes
/// d'inventaire (60 s) tuerait le processus en pleine écriture.
pub const INSTALL_TIMEOUT: Duration = Duration::from_secs(6 * 60 * 60);

/// Une liste de bibliothèque rafraîchit les métadonnées Epic quand le cache est
/// vide : une grosse collection dépasse largement la minute.
pub const LIBRARY_TIMEOUT: Duration = Duration::from_secs(5 * 60);

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

    pub fn run_install(
        &self,
        args: impl IntoIterator<Item = String>,
        cancellation: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<CommandOutput, String> {
        super::runner::run(self.command(args), INSTALL_TIMEOUT, cancellation)
    }

    pub fn sync_owned(
        &self,
        fetched_at: i64,
        cancellation: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<Vec<ProviderGame>, String> {
        let output = super::runner::run(
            self.command(Self::library_args()),
            LIBRARY_TIMEOUT,
            cancellation,
        )?;
        self.owned_listing(&output, fetched_at)
    }

    pub fn sync_installed(
        &self,
        fetched_at: i64,
        cancellation: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<Vec<ProviderGame>, String> {
        let output = self.run(Self::installed_args(), cancellation)?;
        self.installed_json(&output, fetched_at)
    }

    /// Connecte le compte Epic à partir de l'`authorizationCode` copié depuis
    /// la réponse JSON de [`LOGIN_URL`]. La sortie est jetée : elle peut
    /// contenir des jetons de session.
    ///
    /// Legendary sort avec le code 0 même quand Epic refuse le code : le seul
    /// témoin fiable est l'écriture (ou la mise à jour) de `user.json`.
    pub fn login_with_code(&self, code: &str) -> Result<(), String> {
        let path = Self::user_config_path();
        let before = std::fs::read(&path).ok();
        let mut command = self.command(Self::login_args(code));
        command.discard_output = true;
        command.secrets = vec![code.to_string()];
        let output = super::runner::run(command, self.timeout, None)?;
        if output.exit_code != Some(0) {
            return Err("Legendary a échoué pendant la connexion Epic".to_string());
        }
        let after = std::fs::read(&path).ok();
        if super::session_written(before.as_deref(), after.as_deref()) {
            Ok(())
        } else {
            Err("Legendary a refusé l'autorisation Epic (code expiré ou invalide)".to_string())
        }
    }

    /// Fichier d'état écrit par Legendary après une connexion réussie.
    pub fn user_config_path() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| crate::config::home_dir().join(".config"));
        base.join("legendary").join("user.json")
    }

    pub fn is_logged_in() -> bool {
        Self::user_config_path().is_file()
    }

    pub fn login_status_args() -> Vec<String> {
        vec!["status".into(), "--json".into()]
    }

    pub fn login_args(code: &str) -> Vec<String> {
        vec!["auth".into(), "--code".into(), code.into()]
    }
    /// `--json` sérialise les métadonnées, les assets et chaque succès de chaque
    /// jeu : sur une grosse bibliothèque, la sortie dépasse plusieurs mégaoctets
    /// et se fait refuser par le plafond de lecture. Le CSV ne garde que
    /// quatre colonnes, pour la même information utile.
    pub fn library_args() -> Vec<String> {
        vec!["list".into(), "--csv".into()]
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

    pub fn owned_listing(
        &self,
        output: &CommandOutput,
        fetched_at: i64,
    ) -> Result<Vec<ProviderGame>, String> {
        ensure_success(output)?;
        parse_owned_listing(&output.stdout, fetched_at)
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

/// Lit la sortie de `legendary list` : CSV depuis la 1.0.2, JSON accepté pour
/// les versions précédentes et les sorties déjà enregistrées.
pub fn parse_owned_listing(text: &str, fetched_at: i64) -> Result<Vec<ProviderGame>, String> {
    let trimmed = text.trim_start();
    if trimmed.starts_with('[') {
        return parse_owned_json(text, fetched_at);
    }
    parse_owned_csv(text, fetched_at)
}

/// Parseur CSV minimal mais strict (RFC 4180) : guillemets, guillemets doublés,
/// virgules et retours à la ligne à l'intérieur des champs.
fn csv_records(text: &str) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if quoted {
            match character {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => quoted = false,
                _ => field.push(character),
            }
            continue;
        }
        match character {
            '"' if field.is_empty() => quoted = true,
            ',' => record.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            _ => field.push(character),
        }
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records
}

/// `App name,App title,Version,Is DLC` : les DLC sortent sur leurs propres
/// lignes et ne sont pas des jeux lançables.
pub fn parse_owned_csv(text: &str, fetched_at: i64) -> Result<Vec<ProviderGame>, String> {
    let records = csv_records(text);
    let mut rows = records.iter();
    let header = rows
        .next()
        .ok_or_else(|| "sortie CSV Legendary vide".to_string())?;
    let column = |name: &str| {
        header
            .iter()
            .position(|cell| cell.trim().eq_ignore_ascii_case(name))
    };
    let (Some(name_at), Some(title_at)) = (column("App name"), column("App title")) else {
        return Err("sortie CSV Legendary inattendue".to_string());
    };
    let dlc_at = column("Is DLC");
    let mut seen = std::collections::HashSet::new();
    let mut games = Vec::new();
    for row in rows {
        if row.len() <= name_at.max(title_at) {
            continue;
        }
        let is_dlc = dlc_at
            .and_then(|index| row.get(index))
            .is_some_and(|cell| cell.trim().eq_ignore_ascii_case("true"));
        if is_dlc {
            continue;
        }
        let app_name = row[name_at].trim();
        let title = row[title_at].trim();
        if app_name.is_empty() || title.is_empty() {
            continue;
        }
        if !seen.insert(app_name.to_string()) {
            continue;
        }
        games.push(ProviderGame {
            key: ProviderGameKey {
                provider: Provider::Epic,
                product_id: app_name.to_string(),
            },
            title: title.to_string(),
            owned: true,
            install_path: None,
            custom_save_paths: Vec::new(),
            install_state: InstallState::NotInstalled,
            quark_game_id: None,
            capabilities: Default::default(),
            last_synced: Some(fetched_at),
        });
    }
    if games.is_empty() {
        return Err("bibliothèque Epic vide ou compte Legendary indisponible; données précédentes conservées".into());
    }
    Ok(games)
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
            custom_save_paths: Vec::new(),
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
        if !entry.install_path.is_dir() {
            continue;
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
            custom_save_paths: Vec::new(),
            install_state: InstallState::Installed,
            quark_game_id: None,
            capabilities: Default::default(),
            last_synced: Some(fetched_at),
        });
    }
    Ok(games)
}

pub fn merge_epic(existing: &[ProviderGame], refreshed_epic: &[ProviderGame]) -> Vec<ProviderGame> {
    merge_epic_with_verified_installs(existing, refreshed_epic, &[])
}

pub fn merge_epic_with_verified_installs(
    existing: &[ProviderGame],
    refreshed_owned: &[ProviderGame],
    verified_installed: &[ProviderGame],
) -> Vec<ProviderGame> {
    let refreshed = refreshed_owned.iter().map(|incoming| {
        let mut game = incoming.clone();
        if let Some(previous) = existing
            .iter()
            .find(|previous| previous.key == incoming.key)
        {
            if previous.install_state == InstallState::Installed
                && previous
                    .install_path
                    .as_ref()
                    .is_some_and(|path| path.is_dir())
            {
                game.install_state = InstallState::Installed;
                game.install_path = previous.install_path.clone();
            }
            if game.quark_game_id.is_none() {
                game.quark_game_id = previous.quark_game_id.clone();
            }
            if game.capabilities.achievements == CapabilityState::Unknown {
                game.capabilities.achievements = previous.capabilities.achievements.clone();
            }
            if game.capabilities.cloud_saves == CapabilityState::Unknown {
                game.capabilities.cloud_saves = previous.capabilities.cloud_saves.clone();
            }
        }
        game
    });
    let mut merged: Vec<ProviderGame> = existing
        .iter()
        .filter(|game| game.key.provider != Provider::Epic)
        .cloned()
        .collect();
    for game in refreshed {
        if let Some(previous) = merged.iter_mut().find(|previous| previous.key == game.key) {
            *previous = game;
        } else {
            merged.push(game);
        }
    }
    for installed in verified_installed {
        if let Some(previous) = merged
            .iter_mut()
            .find(|previous| previous.key == installed.key)
        {
            let custom_paths = previous.custom_save_paths.clone();
            let quark_game_id = previous.quark_game_id.clone();
            *previous = installed.clone();
            previous.custom_save_paths = custom_paths;
            if previous.quark_game_id.is_none() {
                previous.quark_game_id = quark_game_id;
            }
        } else {
            merged.push(installed.clone());
        }
    }
    merged
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
    fn la_connexion_transmet_le_code_comme_un_seul_argument() {
        assert_eq!(
            LegendaryAdapter::login_args("abc; $(touch x)"),
            vec!["auth", "--code", "abc; $(touch x)"]
        );
        assert!(LOGIN_URL.starts_with("https://legendary.gl/"));
    }

    #[cfg(unix)]
    #[test]
    fn la_connexion_exige_une_session_ecrite_meme_avec_un_code_de_sortie_nul() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "quark-legendary-login-{}-{}",
            std::process::id(),
            crate::config::now_ts()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let config = root.join("config");
        std::fs::create_dir_all(config.join("legendary")).unwrap();
        let previous = std::env::var_os("XDG_CONFIG_HOME");
        // Variable lue par Legendary ; aucun autre test ne la consulte.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &config) };
        let adapter = LegendaryAdapter {
            executable: root.join("legendary"),
            timeout: Duration::from_secs(5),
        };

        // Refus silencieux : exit 0, mais aucune session écrite.
        std::fs::write(&adapter.executable, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&adapter.executable, std::fs::Permissions::from_mode(0o700))
            .unwrap();
        assert!(adapter.login_with_code("code").is_err());

        // Succès : l'outil écrit la session.
        let session = config.join("legendary").join("user.json");
        std::fs::write(
            &adapter.executable,
            format!(
                "#!/bin/sh\nprintf '{{\"account\":\"demo\"}}' > '{}'\n",
                session.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&adapter.executable, std::fs::Permissions::from_mode(0o700))
            .unwrap();
        assert!(adapter.login_with_code("code").is_ok());
        assert!(LegendaryAdapter::is_logged_in());

        unsafe {
            match previous {
                Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
                None => std::env::remove_var("XDG_CONFIG_HOME"),
            }
        }
        let _ = std::fs::remove_dir_all(&root);
    }

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
    fn une_liste_csv_ignore_les_dlc_et_garde_les_titres_echappes() {
        let csv = "App name,App title,Version,Is DLC\r\n\
Anemone,Anemone,1.0,False\r\n\
AnemoneDLC,Anemone: Deluxe,1.0,True\r\n\
\"Odd,Game\",\"Odd, Game \"\"Remastered\"\"\",,False\n\
Empty,,1.0,False\n";
        let games = parse_owned_csv(csv, 7).unwrap();
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].key.product_id, "Anemone");
        assert_eq!(games[1].title, "Odd, Game \"Remastered\"");
        assert_eq!(games[1].key.product_id, "Odd,Game");
        assert_eq!(games[1].last_synced, Some(7));
    }

    #[test]
    fn une_sortie_csv_sans_entete_ou_vide_est_refusee() {
        assert!(parse_owned_csv("", 1).is_err());
        assert!(parse_owned_csv("App name,App title,Version,Is DLC\n", 1).is_err());
        assert!(parse_owned_csv("n'importe quoi\n1,2,3", 1).is_err());
    }

    #[test]
    fn la_liste_accepte_encore_l_ancienne_sortie_json() {
        let json = r#"[{"app_name":"app_1","app_title":"Jeu"}]"#;
        let games = parse_owned_listing(json, 3).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].key.product_id, "app_1");
        let csv = "App name,App title,Version,Is DLC\napp_2,Jeu 2,1.0,False\n";
        assert_eq!(parse_owned_listing(csv, 3).unwrap()[0].title, "Jeu 2");
    }

    fn malformed_or_ambiguous_legendary_output_is_not_an_empty_success() {
        assert!(parse_owned_json("not json", 1).is_err());
        assert!(parse_owned_json("{}", 1).is_err());
        assert!(parse_owned_json("[]", 1).is_err());
    }

    #[test]
    fn legendary_installed_json_keeps_provider_path_and_matches_app_name() {
        let root =
            std::env::temp_dir().join(format!("quark-legendary-fixture-{}", std::process::id()));
        let install_path = root.join("example");
        std::fs::create_dir_all(&install_path).unwrap();
        let json = format!(
            r#"[{{"app_name":"app_123","title":"Example Game","install_path":"{}","version":"1","base_urls":[],"can_run_offline":true,"egl_guid":"x","executable":"game.exe","install_size":42,"install_tags":[],"is_dlc":false,"launch_parameters":[],"manifest_path":"{}/manifest","needs_verification":false,"platform":"Windows","prereq_info":{{}},"uninstaller":null,"requires_ot":false,"save_path":"{}/save","is_preloaded":false}}]"#,
            install_path.display(),
            install_path.display(),
            install_path.display()
        );
        let games = parse_installed_json(&json, 1234).unwrap();
        assert_eq!(games[0].key.product_id, "app_123");
        assert_eq!(
            games[0].install_path.as_deref(),
            Some(install_path.as_path())
        );
        assert_eq!(games[0].install_state, InstallState::Installed);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn failed_legendary_command_never_becomes_an_empty_owned_library() {
        let adapter = LegendaryAdapter::new();
        let output = CommandOutput {
            stdout: "[]".into(),
            stderr: "private".into(),
            exit_code: Some(1),
        };
        assert!(adapter.owned_listing(&output, 1).is_err());
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
            custom_save_paths: Vec::new(),
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
            custom_save_paths: Vec::new(),
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
    fn epic_refresh_preserves_a_previously_verified_install_for_the_same_app_name() {
        let install_path =
            std::env::temp_dir().join(format!("quark-epic-existing-{}", std::process::id()));
        std::fs::create_dir_all(&install_path).unwrap();
        let existing = ProviderGame {
            key: ProviderGameKey {
                provider: Provider::Epic,
                product_id: "app_123".into(),
            },
            title: "Example Game".into(),
            owned: true,
            install_path: Some(install_path.clone()),
            custom_save_paths: Vec::new(),
            install_state: InstallState::Installed,
            quark_game_id: Some("quark-1".into()),
            capabilities: Default::default(),
            last_synced: Some(10),
        };
        let refreshed =
            parse_owned_json(r#"[{"app_name":"app_123","app_title":"Example Game"}]"#, 20).unwrap();
        let merged = merge_epic(&[existing], &refreshed);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].install_state, InstallState::Installed);
        assert_eq!(merged[0].install_path, Some(install_path.clone()));
        assert_eq!(merged[0].quark_game_id.as_deref(), Some("quark-1"));
        let _ = std::fs::remove_dir_all(install_path);
    }

    #[test]
    fn epic_merge_combines_owned_and_installed_records_without_duplicate_provider_ids() {
        let owned = parse_owned_json(
            r#"[{"app_name":"app_1","app_title":"Owned Game"},{"app_name":"app_2","app_title":"Installed Game"}]"#,
            1,
        )
        .unwrap();
        let root =
            std::env::temp_dir().join(format!("quark-legendary-merge-{}", std::process::id()));
        let install_path = root.join("installed");
        std::fs::create_dir_all(&install_path).unwrap();
        let installed_json = format!(
            r#"[{{"app_name":"app_2","title":"Installed Game","install_path":"{}"}}]"#,
            install_path.display()
        );
        let installed = parse_installed_json(&installed_json, 2).unwrap();
        let merged = merge_epic_with_verified_installs(&[], &owned, &installed);
        assert_eq!(merged.len(), 2);
        assert_eq!(
            merged
                .iter()
                .filter(|game| game.key.product_id == "app_2")
                .count(),
            1
        );
        assert_eq!(
            merged
                .iter()
                .find(|game| game.key.product_id == "app_2")
                .unwrap()
                .install_state,
            InstallState::Installed
        );
        assert_eq!(
            merged
                .iter()
                .find(|game| game.key.product_id == "app_1")
                .unwrap()
                .install_state,
            InstallState::NotInstalled
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn newly_verified_epic_install_path_replaces_the_previous_install_location() {
        let root =
            std::env::temp_dir().join(format!("quark-epic-reinstall-{}", std::process::id()));
        let old_path = root.join("old");
        let new_path = root.join("new");
        std::fs::create_dir_all(&old_path).unwrap();
        std::fs::create_dir_all(&new_path).unwrap();
        let previous = ProviderGame {
            key: ProviderGameKey {
                provider: Provider::Epic,
                product_id: "app_1".into(),
            },
            title: "Game".into(),
            owned: true,
            install_path: Some(old_path),
            custom_save_paths: Vec::new(),
            install_state: InstallState::Installed,
            quark_game_id: Some("quark-id".into()),
            capabilities: Default::default(),
            last_synced: Some(1),
        };
        let mut installed = previous.clone();
        installed.install_path = Some(new_path.clone());
        installed.last_synced = Some(2);
        let merged = merge_epic_with_verified_installs(&[previous], &[], &[installed]);
        assert_eq!(merged[0].install_path, Some(new_path));
        assert_eq!(merged[0].quark_game_id.as_deref(), Some("quark-id"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn legendary_install_entries_with_missing_directories_are_not_verified_installs() {
        let missing =
            std::env::temp_dir().join(format!("quark-legendary-missing-{}", std::process::id()));
        let json = format!(
            r#"[{{"app_name":"app_1","title":"Example","install_path":"{}"}}]"#,
            missing.display()
        );
        // Un chemin disparu n'est pas une installation vérifiée, mais il ne
        // doit pas non plus invalider la détection des autres jeux.
        let games = parse_installed_json(&json, 1).unwrap();
        assert!(games.is_empty());

        let root =
            std::env::temp_dir().join(format!("quark-legendary-mixed-{}", std::process::id()));
        let present = root.join("present");
        std::fs::create_dir_all(&present).unwrap();
        let mixed = format!(
            r#"[{{"app_name":"app_1","title":"Gone","install_path":"{}"}},{{"app_name":"app_2","title":"Present","install_path":"{}"}}]"#,
            missing.display(),
            present.display()
        );
        let games = parse_installed_json(&mixed, 2).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].key.product_id, "app_2");
        assert_eq!(games[0].install_state, InstallState::Installed);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn installs_outlive_the_inventory_command_timeout() {
        let adapter = LegendaryAdapter::new();
        assert!(INSTALL_TIMEOUT > adapter.timeout);
        assert!(INSTALL_TIMEOUT >= Duration::from_secs(60 * 60 * 2));
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
