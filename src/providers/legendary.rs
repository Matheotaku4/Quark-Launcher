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

    /// Installation avec avancement publié (téléchargement, vérification…).
    pub fn run_install_streaming(
        &self,
        args: impl IntoIterator<Item = String>,
        cancellation: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
        progress: std::sync::Arc<std::sync::Mutex<super::runner::InstallProgress>>,
    ) -> Result<CommandOutput, String> {
        super::runner::run_streaming(self.command(args), INSTALL_TIMEOUT, cancellation, progress)
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
        config_dir().join("user.json")
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

    /// Installation non interactive. Legendary pose ses questions par
    /// `input()` sans détecter l'absence de terminal : sans `-y` (drapeau
    /// global, donc avant la sous-commande) et sans `--skip-sdl`/`--skip-dlcs`,
    /// la lecture échoue et l'installation ressort en erreur.
    pub fn install_args(app_name: &str, root: &std::path::Path) -> Vec<String> {
        vec![
            "-y".into(),
            "install".into(),
            app_name.into(),
            "--base-path".into(),
            root.display().to_string(),
            "--platform".into(),
            "Windows".into(),
            "--skip-sdl".into(),
            "--skip-dlcs".into(),
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
    pub fn info_args(app_name: &str) -> Vec<String> {
        vec!["info".into(), app_name.into(), "--json".into()]
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
        // `legendary achievements <app> --json` existe depuis la 0.21.0 et la
        // version épinglée est la 0.21.1.
        CapabilityState::Supported
    }

    pub fn supports_cloud_saves(&self) -> CapabilityState {
        // `legendary sync-saves` compare les dates locales et distantes et
        // télécharge ou envoie la sauvegarde la plus récente.
        CapabilityState::Supported
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
            install_root: None,
            custom_save_paths: Vec::new(),
            cover_url: None,
            install_exe: None,
            install_state: InstallState::NotInstalled,
            quark_game_id: None,
            capabilities: super::model::GameCapabilities {
                achievements: CapabilityState::Supported,
                cloud_saves: CapabilityState::Supported,
            },
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
            install_root: None,
            custom_save_paths: Vec::new(),
            cover_url: None,
            install_exe: None,
            install_state: InstallState::NotInstalled,
            quark_game_id: None,
            capabilities: super::model::GameCapabilities {
                achievements: CapabilityState::Supported,
                cloud_saves: CapabilityState::Supported,
            },
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
        /// Chemin relatif au dossier installé, publié par Legendary.
        #[serde(default)]
        executable: Option<String>,
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
        let executable = entry
            .executable
            .as_deref()
            .map(str::trim)
            .filter(|executable| !executable.is_empty())
            .map(|executable| entry.install_path.join(executable))
            .filter(|path| path.is_file());
        games.push(ProviderGame {
            key: ProviderGameKey {
                provider: Provider::Epic,
                product_id: entry.app_name,
            },
            title: entry.title,
            owned: true,
            install_root: entry
                .install_path
                .parent()
                .map(std::path::Path::to_path_buf),
            install_path: Some(entry.install_path),
            custom_save_paths: Vec::new(),
            cover_url: None,
            install_exe: executable,
            install_state: InstallState::Installed,
            quark_game_id: None,
            capabilities: super::model::GameCapabilities {
                achievements: CapabilityState::Supported,
                cloud_saves: CapabilityState::Supported,
            },
            last_synced: Some(fetched_at),
        });
    }
    Ok(games)
}

/// Dossier de configuration de Legendary, commun à `user.json` et au cache de
/// métadonnées.
pub fn config_dir() -> PathBuf {
    if let Some(custom) = std::env::var_os("LEGENDARY_CONFIG_PATH").map(PathBuf::from) {
        return custom;
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| crate::config::home_dir().join(".config"))
        .join("legendary")
}

/// Fichier de métadonnées écrit par Legendary pour un jeu : il contient la
/// fiche catalogue Epic complète, donc `keyImages`.
pub fn metadata_path(app_name: &str) -> PathBuf {
    let safe: String = app_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect();
    config_dir().join("metadata").join(format!("{safe}.json"))
}

/// Choisit la meilleure image parmi `keyImages` : le format vertical de la
/// boutique d'abord, l'illustration large en dernier recours.
pub fn cover_from_key_images(images: &[serde_json::Value]) -> Option<String> {
    const PREFERENCE: [&str; 8] = [
        "DieselGameBoxTall",
        "OfferImageTall",
        "DieselStoreFrontTall",
        "DieselGameBox",
        "Thumbnail",
        "OfferImageWide",
        "DieselStoreFrontWide",
        "featuredMedia",
    ];
    for wanted in PREFERENCE {
        let found = images
            .iter()
            .find(|image| image.get("type").and_then(serde_json::Value::as_str) == Some(wanted))
            .and_then(|image| image.get("url"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|url| {
                (url.starts_with("https://") || url.starts_with("http://")) && !url.is_empty()
            });
        if let Some(url) = found {
            return Some(url.to_string());
        }
    }
    images
        .iter()
        .filter_map(|image| image.get("url").and_then(serde_json::Value::as_str))
        .map(str::trim)
        .find(|url| url.starts_with("https://") || url.starts_with("http://"))
        .map(str::to_string)
}

fn key_images(root: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    root.get("metadata")
        .and_then(|metadata| metadata.get("keyImages"))
        .or_else(|| root.get("keyImages"))
        .or_else(|| {
            root.get("game")
                .and_then(|game| game.get("metadata"))?
                .get("keyImages")
        })
        .and_then(serde_json::Value::as_array)
}

/// Couverture publiée par Epic dans les métadonnées du jeu
/// (`metadata.keyImages`), en préférant le format vertical de la boutique.
pub fn cover_from_info(text: &str) -> Result<String, String> {
    let start = text
        .find('{')
        .ok_or_else(|| "fiche Epic illisible".to_string())?;
    let root: serde_json::Value =
        serde_json::from_str(&text[start..]).map_err(|_| "fiche Epic illisible".to_string())?;
    key_images(&root)
        .and_then(|images| cover_from_key_images(images))
        .ok_or_else(|| "fiche Epic sans couverture".to_string())
}

/// Lit le cache de métadonnées écrit par Legendary après une synchronisation ou
/// un `legendary info`.
pub fn cover_from_metadata(text: &str) -> Result<String, String> {
    let root: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "métadonnées Epic illisibles".to_string())?;
    key_images(&root)
        .and_then(|images| cover_from_key_images(images))
        .ok_or_else(|| "métadonnées Epic sans couverture".to_string())
}

/// Illustration large publiée par Epic : elle sert de bannière dans la fiche.
pub fn hero_from_key_images(images: &[serde_json::Value]) -> Option<String> {
    const PREFERENCE: [&str; 4] = [
        "DieselStoreFrontWide",
        "OfferImageWide",
        "DieselGameBox",
        "Thumbnail",
    ];
    for wanted in PREFERENCE {
        let found = images
            .iter()
            .find(|image| image.get("type").and_then(serde_json::Value::as_str) == Some(wanted))
            .and_then(|image| image.get("url"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|url| url.starts_with("https://") || url.starts_with("http://"));
        if let Some(url) = found {
            return Some(url.to_string());
        }
    }
    None
}

fn hero_root(root: &serde_json::Value) -> Option<String> {
    key_images(root).and_then(|images| hero_from_key_images(images))
}

pub fn hero_from_info(text: &str) -> Result<String, String> {
    let start = text
        .find('{')
        .ok_or_else(|| "fiche Epic illisible".to_string())?;
    let root: serde_json::Value =
        serde_json::from_str(&text[start..]).map_err(|_| "fiche Epic illisible".to_string())?;
    hero_root(&root).ok_or_else(|| "fiche Epic sans illustration large".to_string())
}

pub fn hero_from_metadata(text: &str) -> Result<String, String> {
    let root: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "métadonnées Epic illisibles".to_string())?;
    hero_root(&root).ok_or_else(|| "métadonnées Epic sans illustration large".to_string())
}

/// Couverture Epic. `legendary info --json` ne publie **pas** `keyImages` —
/// seulement quelques champs choisis : la sortie ne suffisait donc jamais.
/// En revanche, il rafraîchit le cache de métadonnées du jeu, qui contient la
/// fiche catalogue complète. On lit ce cache d'abord, puis on le provoque.
pub fn epic_cover_url(app_name: &str) -> Result<String, String> {
    epic_asset_url(app_name, cover_from_metadata, cover_from_info, "couverture")
}

/// Bannière large Epic, avec le même cache de métadonnées que la couverture.
pub fn epic_hero_url(app_name: &str) -> Result<String, String> {
    epic_asset_url(app_name, hero_from_metadata, hero_from_info, "bannière")
}

fn epic_asset_url(
    app_name: &str,
    from_metadata: fn(&str) -> Result<String, String>,
    from_info: fn(&str) -> Result<String, String>,
    label: &str,
) -> Result<String, String> {
    let cache = metadata_path(app_name);
    if let Ok(text) = std::fs::read_to_string(&cache) {
        if let Ok(url) = from_metadata(&text) {
            return Ok(url);
        }
    }
    let adapter = LegendaryAdapter::new();
    if !adapter.executable.is_file() {
        return Err(format!(
            "Legendary n'est pas installé : la {label} Epic est indisponible"
        ));
    }
    let output = super::runner::run(
        adapter.command(LegendaryAdapter::info_args(app_name)),
        Duration::from_secs(120),
        None,
    )?;
    ensure_success(&output)?;
    if let Ok(text) = std::fs::read_to_string(&cache) {
        if let Ok(url) = from_metadata(&text) {
            return Ok(url);
        }
    }
    // Anciennes versions de Legendary : la fiche peut arriver sur stdout.
    from_info(&output.stdout)
}

pub use super::achievements::{AchievementInfo, AchievementRow};

/// Lit `legendary achievements <app> --json`. Legendary range les succès en
/// quatre listes (terminés, en cours, non commencés, masqués) et publie les
/// totaux séparément : les deux formes sont utilisées, avec repli sur la liste.
pub fn parse_achievements(text: &str) -> Result<AchievementInfo, String> {
    #[derive(serde::Deserialize)]
    struct RawRow {
        #[serde(default)]
        name: String,
        #[serde(default)]
        display_name: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        unlocked: bool,
        #[serde(default)]
        icon: Option<String>,
        #[serde(default)]
        icon_gray: Option<String>,
        #[serde(default)]
        icon_url: Option<String>,
        #[serde(default)]
        icon_link: Option<String>,
        #[serde(default)]
        image: Option<String>,
        /// Progression d'un succès à paliers (Legendary la publie en 0..1).
        #[serde(default)]
        progress: Option<f64>,
        /// Certaines versions de Legendary exposent le taux d'obtention ;
        /// les formes publiées varient, d'où la lecture tolérante.
        #[serde(default)]
        rarity: Option<serde_json::Value>,
        #[serde(default)]
        percent: Option<serde_json::Value>,
    }
    #[derive(serde::Deserialize)]
    struct Raw {
        #[serde(default)]
        total_achievements: Option<u32>,
        #[serde(default)]
        user_unlocked: Option<u32>,
        #[serde(default)]
        completed: Option<Vec<RawRow>>,
        #[serde(default)]
        in_progress: Option<Vec<RawRow>>,
        #[serde(default)]
        uninitiated: Option<Vec<RawRow>>,
        #[serde(default)]
        hidden: Option<Vec<RawRow>>,
    }
    // On repère une clé propre à la réponse avant de remonter à l'accolade :
    // une ligne de journal peut contenir un autre objet JSON avant le résultat.
    let marker = text
        .find("\"total_achievements\"")
        .or_else(|| text.find("\"completed\""))
        .or_else(|| text.find("\"in_progress\""));
    let Some(start) = marker
        .and_then(|marker| text[..marker].rfind('{'))
        .or_else(|| text.find('{'))
    else {
        // Legendary peut être lancé sans `--json` par une ancienne version,
        // ou ne rien écrire sur stdout alors que la commande a réussi. Le
        // tableau texte reste alors exploitable.
        return parse_achievements_text(text);
    };
    // Legendary peut ajouter une ligne de journal avant ou après le JSON ;
    // `from_str` exigeait alors que l'objet occupe toute la fin de la sortie.
    // On lit le premier objet complet et on ignore ce qui suit.
    let mut stream = serde_json::Deserializer::from_str(&text[start..]).into_iter::<Raw>();
    let raw = match stream.next() {
        Some(Ok(raw)) => raw,
        Some(Err(_)) | None => {
            // Un objet JSON étranger (journal, réponse d'erreur) ne doit pas
            // masquer un tableau texte valide.
            return parse_achievements_text(text);
        }
    };
    let mut rows = Vec::new();
    for list in [raw.completed, raw.in_progress, raw.uninitiated, raw.hidden]
        .into_iter()
        .flatten()
    {
        for row in list {
            let key = row.name.trim().to_string();
            let name = row
                .display_name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or(row.name.trim())
                .to_string();
            if name.is_empty() {
                continue;
            }
            let icon_url = if row.unlocked {
                row.icon
                    .or(row.icon_url)
                    .or(row.icon_link)
                    .or(row.image)
                    .or(row.icon_gray)
            } else {
                row.icon_gray
                    .or(row.icon)
                    .or(row.icon_url)
                    .or(row.icon_link)
                    .or(row.image)
            }
            .map(|url| url.trim().to_string())
            .filter(|url| url.starts_with("https://") || url.starts_with("http://"));
            rows.push(AchievementRow {
                id: key.clone(),
                key,
                name,
                description: row.description.unwrap_or_default().trim().to_string(),
                unlocked: row.unlocked,
                progress: if row.unlocked {
                    1.0
                } else {
                    row.progress.unwrap_or(0.0).clamp(0.0, 1.0) as f32
                },
                session: false,
                rarity: super::achievements::rarity_percent(
                    row.rarity.as_ref().or(row.percent.as_ref()),
                ),
                icon_url,
                icon_path: None,
            });
        }
    }
    let total = raw.total_achievements.unwrap_or(0);
    if rows.is_empty() && total == 0 {
        return Err("ce jeu Epic ne publie pas de succès".to_string());
    }
    let mut info = AchievementInfo {
        total: total.max(rows.len() as u32),
        unlocked: raw.user_unlocked.unwrap_or(0),
        rows,
    };
    if info.unlocked == 0 {
        info.unlocked = info.rows.iter().filter(|row| row.unlocked).count() as u32;
    }
    info.unlocked = info.unlocked.min(info.total);
    Ok(info)
}

/// Repli pour la sortie texte de `legendary achievements` (versions qui ne
/// connaissent pas `--json`, ou réponse vide). Les lignes publiées sont :
/// ` - Nom | 100XP | description | Progress: 42.0% | Completed on: …`.
pub fn parse_achievements_text(text: &str) -> Result<AchievementInfo, String> {
    let mut total = 0u32;
    let mut unlocked = 0u32;
    let mut rows = Vec::new();
    let mut section = "";
    for line in text.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("Total achievements:") {
            total = value.trim().parse().unwrap_or(0);
            continue;
        }
        if let Some(value) = line.strip_prefix("Completed achievements:") {
            unlocked = value.trim().parse().unwrap_or(0);
            continue;
        }
        if let Some(value) = line.strip_prefix("* ") {
            section = value.trim();
            continue;
        }
        if !line.starts_with("- ") || section.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line[2..].split('|').map(str::trim).collect();
        let name = fields.first().copied().unwrap_or("").trim();
        if name.is_empty() {
            continue;
        }
        let description = fields.get(2).copied().unwrap_or("").trim().to_string();
        let progress = fields
            .get(3)
            .and_then(|value| value.strip_prefix("Progress:"))
            .and_then(|value| value.trim().trim_end_matches('%').parse::<f32>().ok())
            .map(|percent| (percent / 100.0).clamp(0.0, 1.0))
            .unwrap_or(0.0);
        let is_unlocked = section.eq_ignore_ascii_case("Completed");
        rows.push(AchievementRow {
            key: name.to_string(),
            id: name.to_string(),
            name: name.to_string(),
            description,
            unlocked: is_unlocked,
            progress: if is_unlocked { 1.0 } else { progress },
            session: false,
            rarity: None,
            icon_url: None,
            icon_path: None,
        });
    }
    if rows.is_empty() {
        return Err(
            "Legendary n'a renvoyé aucun succès lisible (jeu sans succès ou cache Epic incomplet)"
                .to_string(),
        );
    }
    let mut info = AchievementInfo {
        total: total.max(rows.len() as u32),
        unlocked,
        rows,
    };
    info.normalize();
    Ok(info)
}

/// Taille annoncée par `legendary info --json`, quand la version de Legendary
/// la publie. Une absence n'est pas une erreur : la fenêtre d'installation
/// affiche simplement « taille inconnue » et laisse démarrer le téléchargement.
pub fn epic_install_size(app_name: &str) -> Result<Option<u64>, String> {
    let adapter = LegendaryAdapter::new();
    if !adapter.executable.is_file() {
        return Err("Legendary n'est pas installé".to_string());
    }
    let output = super::runner::run(
        adapter.command(LegendaryAdapter::info_args(app_name)),
        Duration::from_secs(120),
        None,
    )?;
    ensure_success(&output)?;
    Ok(super::install_size_from_json(&output.stdout))
}

pub fn epic_achievements(app_name: &str) -> Result<AchievementInfo, String> {
    let adapter = LegendaryAdapter::new();
    if !adapter.executable.is_file() {
        return Err(
            "Legendary n'est pas installé : les succès Epic sont indisponibles".to_string(),
        );
    }
    let run = || {
        super::runner::run(
            adapter.command(LegendaryAdapter::achievements_args(app_name)),
            Duration::from_secs(120),
            None,
        )
    };
    let mut output = run()?;
    if output.exit_code != Some(0) {
        return Err(format!(
            "Legendary n'a pas pu lire les succès ({})",
            super::runner::failure_hint(&output)
        ));
    }
    let first = parse_achievements(&output.stdout);
    if first.is_ok() {
        return first;
    }
    // Le cache de métadonnées Epic ne contient parfois pas encore la fiche des
    // succès. `info` la rafraîchit ; on retente une seule fois pour éviter une
    // boucle de commandes réseau.
    let info = super::runner::run(
        adapter.command(LegendaryAdapter::info_args(app_name)),
        Duration::from_secs(120),
        None,
    );
    if info
        .as_ref()
        .is_ok_and(|output| output.exit_code == Some(0))
    {
        output = run()?;
        if let Ok(info) = parse_achievements(&output.stdout) {
            return Ok(info);
        }
    }
    Err(match first {
        Err(error) => error,
        Ok(_) => "Les succès Epic n'ont pas pu être relus".to_string(),
    })
}

/// Temps de jeu Epic. Legendary ne le publie pas dans ses listes, mais son
/// `user.json` contient la session que le client Epic accepte : la lecture est
/// faite ici, sans afficher ni journaliser le jeton.
pub fn epic_playtime(artifact_id: &str) -> Result<u64, String> {
    let text = std::fs::read_to_string(LegendaryAdapter::user_config_path()).map_err(|_| {
        "session Epic absente : connecte la boutique dans Paramètres → Boutiques & outils"
            .to_string()
    })?;
    let (account, token) = parse_user_session(&text)?;
    let response = crate::artwork::http()
        .get(format!(
            "https://library-service.live.use1a.on.epicgames.com/library/api/public/playtime/account/{account}/artifact/{artifact_id}"
        ))
        .bearer_auth(token)
        .timeout(Duration::from_secs(20))
        .send()
        .map_err(|_| "temps de jeu Epic injoignable".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Epic a refusé le temps de jeu (HTTP {})",
            response.status().as_u16()
        ));
    }
    let body = crate::net::text(response, crate::net::MAX_JSON, "temps de jeu Epic")
        .map_err(|_| "temps de jeu Epic illisible".to_string())?;
    parse_playtime(&body)
}

/// Champs utiles de `user.json` : identifiant de compte et jeton d'accès.
pub fn parse_user_session(text: &str) -> Result<(String, String), String> {
    let root: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "session Epic illisible".to_string())?;
    let account = root
        .get("account_id")
        .or_else(|| root.get("accountId"))
        .or_else(|| root.get("account"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "session Epic sans identifiant de compte".to_string())?;
    let token = root
        .get("access_token")
        .or_else(|| root.get("accessToken"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "session Epic sans jeton d'accès".to_string())?;
    Ok((account.to_string(), token.to_string()))
}

/// `totalTime` est publié en secondes par le service de bibliothèque Epic.
pub fn parse_playtime(body: &str) -> Result<u64, String> {
    let root: serde_json::Value =
        serde_json::from_str(body).map_err(|_| "temps de jeu Epic illisible".to_string())?;
    let artifact = root.get("artifact").unwrap_or(&root);
    let seconds = artifact
        .get("totalTime")
        .or_else(|| artifact.get("total_time"))
        .or_else(|| root.get("totalTime"))
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "Epic ne publie pas de temps de jeu pour ce jeu".to_string())?;
    Ok(seconds)
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
            if game.cover_url.is_none() {
                game.cover_url = previous.cover_url.clone();
            }
            if game.install_exe.is_none() {
                game.install_exe = previous.install_exe.clone();
            }
            if game.install_root.is_none() {
                game.install_root = previous.install_root.clone();
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
    fn le_temps_de_jeu_epic_est_lu_dans_la_session_et_la_reponse_de_bibliotheque() {
        let session = r#"{"account_id":"abc123","displayName":"joueur","access_token":"jeton"}"#;
        assert_eq!(
            parse_user_session(session).unwrap(),
            ("abc123".to_string(), "jeton".to_string())
        );
        assert!(parse_user_session(r#"{"displayName":"joueur"}"#).is_err());
        assert!(parse_user_session("pas du json").is_err());
        assert_eq!(
            parse_playtime(r#"{"artifactId":"Anemone","totalTime":2440}"#).unwrap(),
            2440
        );
        assert_eq!(
            parse_playtime(r#"{"artifact":{"artifactId":"Anemone","totalTime":90}}"#).unwrap(),
            90
        );
        assert!(parse_playtime(r#"{"artifactId":"Anemone"}"#).is_err());
    }

    #[test]
    fn la_couverture_epic_vient_du_cache_de_metadonnees_et_prefere_le_format_vertical() {
        let cached = r#"{
          "app_name": "Anemone",
          "app_title": "Anemone",
          "metadata": {
            "title": "Anemone",
            "keyImages": [
              {"type": "OfferImageWide", "url": "https://cdn1.epicgames.com/wide.jpg"},
              {"type": "DieselGameBoxTall", "url": "https://cdn1.epicgames.com/tall.jpg"},
              {"type": "Thumbnail", "url": "https://cdn1.epicgames.com/thumb.jpg"}
            ]
          },
          "asset_infos": {},
          "base_urls": []
        }"#;
        assert_eq!(
            cover_from_metadata(cached).unwrap(),
            "https://cdn1.epicgames.com/tall.jpg"
        );
        // Le wrapper `game` de certaines versions est accepté aussi.
        let wrapped = r#"{"game":{"metadata":{"keyImages":[{"type":"Thumbnail","url":"https://cdn1.epicgames.com/t.jpg"}]}}}"#;
        assert_eq!(
            cover_from_metadata(wrapped).unwrap(),
            "https://cdn1.epicgames.com/t.jpg"
        );
        assert!(cover_from_metadata(r#"{"metadata":{}}"#).is_err());
        assert!(cover_from_metadata("pas du json").is_err());
        // La sortie de `legendary info` sans keyImages ne suffit pas.
        assert!(cover_from_info(r#"{"game":{"app_name":"x"}}"#).is_err());
        // La bannière prend l'image large, pas la boîte verticale.
        assert_eq!(
            hero_from_metadata(cached).unwrap(),
            "https://cdn1.epicgames.com/wide.jpg"
        );
        assert!(hero_from_metadata(r#"{"metadata":{}}"#).is_err());
        assert!(hero_from_info(r#"{"game":{"app_name":"x"}}"#).is_err());
    }

    #[test]
    fn le_chemin_du_cache_legendary_est_sur_et_respecte_la_variable_de_configuration() {
        let previous = std::env::var_os("LEGENDARY_CONFIG_PATH");
        let root = std::env::temp_dir().join(format!("quark-lgd-meta-{}", std::process::id()));
        unsafe { std::env::set_var("LEGENDARY_CONFIG_PATH", &root) };
        let path = metadata_path("../../evil app");
        assert_eq!(path, root.join("metadata").join(".._.._evil_app.json"));
        unsafe {
            match previous {
                Some(value) => std::env::set_var("LEGENDARY_CONFIG_PATH", value),
                None => std::env::remove_var("LEGENDARY_CONFIG_PATH"),
            }
        }
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

    #[test]
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
            install_root: None,
            custom_save_paths: Vec::new(),
            cover_url: None,
            install_exe: None,
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
            install_root: None,
            custom_save_paths: Vec::new(),
            cover_url: None,
            install_exe: None,
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
            install_root: None,
            custom_save_paths: Vec::new(),
            cover_url: None,
            install_exe: None,
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
            install_root: None,
            custom_save_paths: Vec::new(),
            cover_url: None,
            install_exe: None,
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
                // `-y` est global : sans lui, Legendary attend des réponses au
                // clavier, qu'il n'a pas dans un processus sans terminal.
                "-y",
                "install",
                "Game; $(touch x)",
                "--base-path",
                "/Games With Space",
                "--platform",
                "Windows",
                "--skip-sdl",
                "--skip-dlcs"
            ]
        );
    }

    #[test]
    fn les_succes_epic_tolerent_un_journal_apres_le_json_et_les_listes_nulles() {
        let json = r#"[Core] INFO: using config directory
        {"total_achievements": 1, "user_unlocked": 0, "completed": null,
         "in_progress": null, "uninitiated": [
            {"name": "ACH_ONE", "display_name": "Premier", "description": "Fini",
             "icon_link": "https://cdn/ach.png", "progress": 0.5}
         ], "hidden": null}
        [Core] INFO: done"#;
        let info = parse_achievements(json).unwrap();
        assert_eq!(info.total, 1);
        assert_eq!(info.rows[0].name, "Premier");
        assert_eq!(
            info.rows[0].icon_url.as_deref(),
            Some("https://cdn/ach.png")
        );
        assert_eq!(info.rows[0].progress, 0.5);
        assert!(parse_achievements("pas de json").is_err());
    }

    #[test]
    fn un_tableau_texte_legendary_reste_lisible_sans_json() {
        let text = r#"* Achievements for "Celeste"
  Total achievements: 2
  Completed achievements: 1
* Completed
 - First Step | 100XP | Termine le tutoriel | Progress: 100.0% | Completed on: 2026-01-01
* In progress
 - Summit | 200XP | Atteins le sommet | Progress: 40.0% | Completed on: None
"#;
        let info = parse_achievements(text).unwrap();
        assert_eq!(info.total, 2);
        assert_eq!(info.unlocked, 1);
        assert_eq!(info.rows.len(), 2);
        assert!(info.rows[0].unlocked);
        assert_eq!(info.rows[1].progress, 0.4);
    }

    #[test]
    fn les_succes_epic_lisent_le_taux_d_obtention_quand_il_est_publie() {
        let json = r#"{
            "total_achievements": 2,
            "user_unlocked": 1,
            "completed": [
                {"name": "ACH_ONE", "display_name": "Premier", "unlocked": true, "rarity": 12.5}
            ],
            "in_progress": [],
            "uninitiated": [],
            "hidden": []
        }"#;
        let info = parse_achievements(json).unwrap();
        assert_eq!(info.rows[0].rarity, Some(12.5));
        let object = r#"{
            "total_achievements": 1,
            "user_unlocked": 0,
            "completed": [],
            "in_progress": [],
            "uninitiated": [
                {"name": "ACH_TWO", "display_name": "Deuxième", "rarity": {"percent": 3.75}}
            ],
            "hidden": []
        }"#;
        assert_eq!(
            parse_achievements(object).unwrap().rows[0].rarity,
            Some(3.75)
        );
    }
}
