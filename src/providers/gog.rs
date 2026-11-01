//! Adaptateur GOG limité aux commandes réellement exposées par gogdl 1.4.0.
//!
//! gogdl n'expose pas de commande CLI de découverte/liste de bibliothèque :
//! l'adaptateur ne déduit donc jamais une bibliothèque possédée de `info`.

use super::model::{CapabilityState, InstallState, Provider, ProviderGame};
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

    /// Téléchargement avec avancement publié (gogdl écrit le même format que
    /// Legendary : `= Progress: 12.50 4472…/1053…`).
    pub fn run_install_streaming(
        &self,
        args: impl IntoIterator<Item = String>,
        cancellation: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
        progress: std::sync::Arc<std::sync::Mutex<super::runner::InstallProgress>>,
    ) -> Result<CommandOutput, String> {
        super::runner::run_streaming(
            self.command(args),
            Duration::from_secs(6 * 60 * 60),
            cancellation,
            progress,
        )
    }

    /// Bibliothèque possédée, lue via l'API publique de GOG avec les jetons
    /// écrits par gogdl (qui ne sait pas lister une bibliothèque).
    pub fn sync_library(&self, fetched_at: i64) -> Result<Vec<super::model::ProviderGame>, String> {
        let tokens = super::gog_library::usable_tokens(&self.auth_config, crate::config::now_ts())?;
        let owned = super::gog_library::library(&tokens.access_token)?;
        Ok(super::gog_library::to_games(&owned, fetched_at))
    }

    pub fn login_args(code: &str) -> Vec<String> {
        vec!["auth".into(), "--code".into(), code.into()]
    }

    /// gogdl ne liste rien, mais l'API GOG le fait avec la session gogdl.
    pub fn library_capability() -> CapabilityState {
        CapabilityState::Supported
    }

    /// `--path` est le dossier **parent** : gogdl y ajoute le nom de dossier
    /// publié par `gogdl info` (`folder_name`). DLC et langue sont explicites
    /// pour que rien ne dépende d'un choix interactif.
    pub fn install_args(game_id: &str, destination: &Path, platform: &str) -> Vec<String> {
        vec![
            "download".into(),
            game_id.into(),
            "--path".into(),
            destination.display().to_string(),
            "--platform".into(),
            platform.into(),
            "--lang".into(),
            "en-US".into(),
            "--skip-dlcs".into(),
        ]
    }

    /// Taille et nom de dossier du téléchargement (`gogdl info` sort du JSON).
    pub fn info_args(game_id: &str, platform: &str) -> Vec<String> {
        vec![
            "info".into(),
            game_id.into(),
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

/// Taille annoncée par `gogdl info`, quand gogdl la publie.
pub fn install_size_from_info(text: &str) -> Option<u64> {
    super::install_size_from_json(text)
}

/// Taille d'installation demandée à gogdl pour le jeu choisi.
pub fn gog_install_size(game_id: &str) -> Result<Option<u64>, String> {
    let adapter = GogAdapter::new(auth_config_path());
    if !adapter.executable.is_file() {
        return Err("gogdl n'est pas installé".to_string());
    }
    let output = adapter.run(GogAdapter::info_args(game_id, "windows"), None)?;
    if output.exit_code != Some(0) {
        return Err("gogdl n'a pas pu lire la taille du jeu".to_string());
    }
    Ok(install_size_from_info(&output.stdout))
}

/// Nom du dossier d'installation publié par `gogdl info`.
pub fn folder_from_info(text: &str) -> Result<String, String> {
    let start = text
        .find('{')
        .ok_or_else(|| "réponse gogdl illisible".to_string())?;
    let root: serde_json::Value =
        serde_json::from_str(&text[start..]).map_err(|_| "réponse gogdl illisible".to_string())?;
    root.get("folder_name")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty() && !name.contains('/') && !name.contains('\0'))
        .map(str::to_string)
        .ok_or_else(|| "gogdl ne publie pas de dossier pour ce jeu".to_string())
}

/// Retrouve le dossier réellement créé par gogdl. `gogdl info` publie un
/// `folder_name`, mais certaines versions ou certains jeux créent un dossier
/// légèrement différent (ponctuation, suffixe de langue, ancien dossier déjà
/// présent). On vérifie donc le dossier annoncé, puis les dossiers voisins qui
/// portent le manifeste GOG du jeu avant de déclarer l'installation échouée.
pub fn resolve_install_path(
    destination: &Path,
    folder_hint: &str,
    product_id: &str,
) -> Option<PathBuf> {
    let exact = destination.join(folder_hint);
    if is_game_directory(&exact, product_id) {
        return Some(exact);
    }
    // Certaines versions installent directement dans le dossier choisi.
    if is_game_directory(destination, product_id) {
        return Some(destination.to_path_buf());
    }

    let marker = format!("goggame-{product_id}.info").to_lowercase();
    let mut candidates: Vec<(u8, std::time::SystemTime, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(destination).ok()?.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&path) else {
            continue;
        };
        let mut score = 0;
        let mut modified = std::time::SystemTime::UNIX_EPOCH;
        for file in files.flatten() {
            let name = file.file_name().to_string_lossy().to_lowercase();
            if name == marker {
                score = 3;
            } else if score < 2 && name.starts_with("goggame-") && name.ends_with(".info") {
                score = 2;
            } else if score < 1
                && (name.ends_with(".exe") || name.ends_with(".info") || name == "gameinfo")
            {
                score = 1;
            }
            if let Ok(time) = file.metadata().and_then(|meta| meta.modified()) {
                modified = modified.max(time);
            }
        }
        if score > 0 {
            candidates.push((score, modified, path));
        }
    }
    candidates.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    // Un manifeste portant l'identifiant exact est la preuve la plus forte.
    if let Some((3, _, path)) = candidates.first() {
        return Some(path.clone());
    }
    // Sinon, seulement un dossier dont le nom ressemble à celui annoncé, ou
    // l'unique dossier de jeu trouvé : jamais un autre jeu installé à côté.
    let hint_key = folder_key(folder_hint);
    if let Some((score, _, path)) = candidates.iter().find(|(score, _, path)| {
        *score <= 1
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| folder_key(name) == hint_key)
    }) {
        let _ = score;
        return Some(path.clone());
    }
    (candidates.len() == 1 && candidates[0].0 <= 1).then(|| candidates[0].2.clone())
}

fn folder_key(name: &str) -> String {
    name.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_game_directory(dir: &Path, product_id: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    let marker = format!("goggame-{product_id}.info").to_lowercase();
    let mut names = Vec::new();
    for entry in entries.flatten() {
        names.push(entry.file_name().to_string_lossy().to_lowercase());
    }
    let has_other_manifest = names
        .iter()
        .any(|name| name.starts_with("goggame-") && name.ends_with(".info") && name != &marker);
    if has_other_manifest {
        return false;
    }
    names.iter().any(|name| {
        name == &marker
            || name.ends_with(".exe")
            || name == "gameinfo"
            || (name.starts_with("goggame-") && name.ends_with(".info"))
    })
}

/// Exécutable le plus plausible dans un dossier GOG installé : le plus court
/// chemin `.exe` sous la racine, sinon un binaire Linux portant le nom du jeu.
pub fn find_executable(root: &Path, title: &str) -> Option<PathBuf> {
    fn walk(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
        if depth > 3 || found.len() > 40 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, depth + 1, found);
            } else if path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("exe") || extension.is_empty()
                })
            {
                found.push(path);
            }
        }
    }
    let mut candidates = Vec::new();
    walk(root, 0, &mut candidates);
    let normalized = title.to_lowercase();
    candidates.sort_by_key(|path| path.components().count());
    let by_name = candidates.iter().find(|path| {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| {
                let stem = stem.to_lowercase();
                stem.len() > 3 && (normalized.contains(&stem) || stem.contains(&normalized))
            })
    });
    by_name.cloned().or_else(|| {
        candidates
            .into_iter()
            .find(|path| path.extension().is_some())
    })
}

/// Fusionne la bibliothèque GOG rafraîchie sans perdre l'état local.
fn gog_title_key(title: &str) -> String {
    super::gog_library::clean_gog_title(title)
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn merge_gog(existing: &[ProviderGame], refreshed: &[ProviderGame]) -> Vec<ProviderGame> {
    let mut merged: Vec<ProviderGame> = existing
        .iter()
        .filter(|game| game.key.provider != Provider::Gog)
        .cloned()
        .collect();
    for incoming in refreshed {
        let mut game = incoming.clone();
        // La déduplication GOG peut garder l'identifiant de la fiche normale
        // alors que l'installation suivie appartient à l'ancienne fiche
        // « Amazon Prime » : on récupère l'état par titre en priorité.
        let previous = existing
            .iter()
            .find(|previous| previous.key == incoming.key)
            .or_else(|| {
                existing
                    .iter()
                    .filter(|previous| {
                        previous.key.provider == Provider::Gog
                            && gog_title_key(&previous.title) == gog_title_key(&incoming.title)
                    })
                    .max_by_key(|previous| {
                        (previous.install_state == InstallState::Installed) as u8
                    })
            });
        if let Some(previous) = previous {
            game.custom_save_paths = previous.custom_save_paths.clone();
            if game.quark_game_id.is_none() {
                game.quark_game_id = previous.quark_game_id.clone();
            }
            if game.cover_url.is_none() {
                game.cover_url = previous.cover_url.clone();
            }
            if game.install_exe.is_none() {
                game.install_exe = previous.install_exe.clone();
            }
            if game.install_root.is_none() {
                game.install_root = previous.install_root.clone();
            }
            if previous.install_state == InstallState::Installed
                && previous
                    .install_path
                    .as_ref()
                    .is_some_and(|path| path.is_dir())
            {
                game.install_state = InstallState::Installed;
                game.install_path = previous.install_path.clone();
            }
            if game.capabilities.achievements == CapabilityState::Unknown {
                game.capabilities.achievements = previous.capabilities.achievements.clone();
            }
            if game.capabilities.cloud_saves == CapabilityState::Unknown {
                game.capabilities.cloud_saves = previous.capabilities.cloud_saves.clone();
            }
        }
        merged.push(game);
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_bibliotheque_gog_est_annoncee_comme_disponible() {
        assert_eq!(GogAdapter::library_capability(), CapabilityState::Supported);
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
                "windows",
                "--lang",
                "en-US",
                "--skip-dlcs"
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
    fn le_dossier_reel_est_retrouve_meme_si_gogdl_a_change_son_nom() {
        let root = std::env::temp_dir().join(format!(
            "quark-gog-install-{}-{}",
            std::process::id(),
            crate::config::now_ts()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Call of Juarez")).unwrap();
        std::fs::write(root.join("Call of Juarez/goggame-123.info"), "{}").unwrap();
        // Un autre jeu installé à côté ne doit jamais être choisi.
        std::fs::create_dir_all(root.join("Autre jeu")).unwrap();
        std::fs::write(root.join("Autre jeu/goggame-999.info"), "{}").unwrap();
        let found = resolve_install_path(&root, "Call of Juarez Prime", "123").unwrap();
        assert_eq!(found, root.join("Call of Juarez"));
        assert_eq!(
            resolve_install_path(&root, "Call of Juarez", "123").unwrap(),
            root.join("Call of Juarez")
        );
        assert!(resolve_install_path(&root, "Call of Juarez", "404").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn la_page_de_connexion_gog_est_l_url_officielle_du_client_public() {
        assert!(LOGIN_URL.starts_with("https://auth.gog.com/auth?client_id=46899977096215655"));
        assert!(LOGIN_URL.contains("response_type=code"));
    }
}
