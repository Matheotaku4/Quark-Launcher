//! Configuration, bibliothèque de jeux et persistance de Quark.
//!
//! Tout vit dans deux dossiers :
//!   * `~/.config/quark/config.json`      -> réglages + jeux (lisible/éditable à la main)
//!   * `~/.local/share/quark/`            -> artworks mis en cache + préfixes Proton par défaut

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const APP_NAME: &str = "Quark";

pub fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| home_dir().join(".config"))
        .join("quark")
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| home_dir().join(".local/share"))
        .join("quark")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

/// Dossier racine des artworks téléchargés.
pub fn artwork_root() -> PathBuf {
    data_dir().join("artwork")
}

/// Dossier racine des préfixes Proton créés par Quark.
pub fn prefix_root() -> PathBuf {
    data_dir().join("prefixes")
}

/// Identifiant unique (suffisant pour une bibliothèque locale).
pub fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:x}{:x}{:x}", t.as_secs(), t.subsec_nanos(), n)
}

/// Développe un `~` en chemin réel.
pub fn expand_tilde(p: &str) -> String {
    let trimmed = p.trim();
    if let Some(rest) = trimmed.strip_prefix("~/") {
        home_dir().join(rest).display().to_string()
    } else if trimmed == "~" {
        home_dir().display().to_string()
    } else {
        trimmed.to_string()
    }
}

/// URI exploitable par le chargeur d'images d'egui.
pub fn file_uri(path: &Path) -> String {
    let s = path.display().to_string();
    if s.starts_with("file://") {
        s
    } else {
        format!("file://{s}")
    }
}

// ---------------------------------------------------------------------------
// Artworks
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Cover,
    Hero,
    Logo,
    Icon,
}

impl AssetKind {
    pub const ALL: [AssetKind; 4] = [
        AssetKind::Cover,
        AssetKind::Hero,
        AssetKind::Logo,
        AssetKind::Icon,
    ];

    /// Nom de fichier (sans extension) dans le dossier d'artworks du jeu.
    pub fn stem(self) -> &'static str {
        match self {
            AssetKind::Cover => "cover",
            AssetKind::Hero => "hero",
            AssetKind::Logo => "logo",
            AssetKind::Icon => "icon",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AssetKind::Cover => "couverture",
            AssetKind::Hero => "bannière",
            AssetKind::Logo => "logo",
            AssetKind::Icon => "icône",
        }
    }
}

/// chemins locaux des artworks d'un jeu (None = absent -> rendu de secours).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Artwork {
    pub cover: Option<PathBuf>,
    pub hero: Option<PathBuf>,
    pub logo: Option<PathBuf>,
    pub icon: Option<PathBuf>,
}

impl Artwork {
    pub fn get(&self, kind: AssetKind) -> Option<&PathBuf> {
        match kind {
            AssetKind::Cover => self.cover.as_ref(),
            AssetKind::Hero => self.hero.as_ref(),
            AssetKind::Logo => self.logo.as_ref(),
            AssetKind::Icon => self.icon.as_ref(),
        }
    }

    pub fn set(&mut self, kind: AssetKind, path: PathBuf) {
        match kind {
            AssetKind::Cover => self.cover = Some(path),
            AssetKind::Hero => self.hero = Some(path),
            AssetKind::Logo => self.logo = Some(path),
            AssetKind::Icon => self.icon = Some(path),
        }
    }
}

// ---------------------------------------------------------------------------
// Réglages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    List,
    Grid,
}

impl Default for ViewMode {
    fn default() -> Self {
        ViewMode::List
    }
}

impl ViewMode {
    pub fn label(self) -> &'static str {
        match self {
            ViewMode::List => "Liste",
            ViewMode::Grid => "Grille",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    /// Noir profond (AMOLED) : #000000 partout.
    #[default]
    Amoled,
    /// Noir/gris ardoise, un peu moins agressif sur les écrans non-OLED.
    Graphite,
}

impl ThemeMode {
    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::Amoled => "AMOLED (noir profond)",
            ThemeMode::Graphite => "Graphite (noir doux)",
        }
    }
}

/// Comment lancer un jeu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", tag = "mode", content = "runtime")]
pub enum ProtonMode {
    /// Automatique : Proton si l'exécutable est un .exe, sinon natif.
    #[default]
    Default,
    /// Toujours lancer directement (binaire Linux).
    Native,
    /// Toujours via ce runtime Proton (clé = dossier du runtime).
    Runtime(String),
}

impl ProtonMode {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub view: ViewMode,
    pub theme: ThemeMode,
    /// Clé API SteamGridDB (https://www.steamgriddb.com/profile/preferences/api).
    pub sgdb_api_key: String,
    /// Langue demandée à l'API Steam (storesearch).
    pub language: String,
    /// Pays demandé à l'API Steam (storesearch).
    pub country: String,
    /// Clé du runtime Proton utilisé par défaut (None = meilleur détecté).
    pub default_proton: Option<String>,
    /// Préfixe Proton par défaut pour les jeux qui n'en ont pas d'autre.
    pub default_prefix: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            view: ViewMode::List,
            theme: ThemeMode::Amoled,
            sgdb_api_key: String::new(),
            language: "french".to_string(),
            country: "FR".to_string(),
            default_proton: None,
            default_prefix: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Jeux
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Game {
    pub id: String,
    pub name: String,
    pub exe: String,
    /// Arguments supplémentaires passés après l'exécutable.
    pub args: String,
    /// AppID Steam si le jeu a été rapproché de la fiche Steam.
    pub steam_app_id: Option<u32>,
    /// Identifiant SteamGridDB si le jeu vient de la recherche SteamGridDB.
    pub sgdb_id: Option<u64>,
    pub artwork: Artwork,
    pub playtime_secs: u64,
    pub last_played: Option<i64>,
    pub added_at: i64,
    pub proton: ProtonMode,
    /// Préfixe Proton propre à ce jeu (écrase le préfixe par défaut).
    pub prefix: Option<String>,
}

impl Default for Game {
    fn default() -> Self {
        Self {
            id: new_id(),
            name: String::new(),
            exe: String::new(),
            args: String::new(),
            steam_app_id: None,
            sgdb_id: None,
            artwork: Artwork::default(),
            playtime_secs: 0,
            last_played: None,
            added_at: now_ts(),
            proton: ProtonMode::Default,
            prefix: None,
        }
    }
}

impl Game {
    pub fn new(name: impl Into<String>, exe: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            exe: exe.into(),
            ..Default::default()
        }
    }

    pub fn exe_path(&self) -> PathBuf {
        PathBuf::from(expand_tilde(&self.exe))
    }

    /// Préfixe Proton effectif : jeu > réglages > dossier généré par Quark.
    pub fn prefix_path(&self, s: &Settings) -> PathBuf {
        if let Some(p) = self.prefix.as_ref().filter(|p| !p.trim().is_empty()) {
            return PathBuf::from(expand_tilde(p));
        }
        if let Some(p) = s.default_prefix.as_ref().filter(|p| !p.trim().is_empty()) {
            return PathBuf::from(expand_tilde(p));
        }
        prefix_root().join(&self.id)
    }

    pub fn playtime_label(&self) -> String {
        if self.playtime_secs == 0 {
            "Aucune session".to_string()
        } else {
            human_playtime(self.playtime_secs)
        }
    }

    pub fn last_played_label(&self) -> String {
        match self.last_played {
            Some(t) => human_ago(t),
            None => "Jamais lancé".to_string(),
        }
    }

    pub fn initials(&self) -> String {
        let mut out = String::new();
        for word in self.name.split_whitespace().take(2) {
            if let Some(c) = word.chars().next() {
                out.extend(c.to_uppercase());
            }
        }
        if out.is_empty() { "?".to_string() } else { out }
    }
}

pub fn human_playtime(secs: u64) -> String {
    if secs < 60 {
        "< 1 min".to_string()
    } else if secs < 3600 {
        format!("{} min", secs / 60)
    } else {
        format!("{:.1} h", secs as f64 / 3600.0).replace('.', ",")
    }
}

pub fn human_ago(ts: i64) -> String {
    let delta = (now_ts() - ts).max(0) as u64;
    match delta {
        0..=59 => "à l'instant".to_string(),
        60..=3599 => format!("il y a {} min", delta / 60),
        3600..=86_399 => format!("il y a {} h", delta / 3600),
        86_400..=2_591_999 => {
            let d = delta / 86_400;
            if d == 1 {
                "hier".to_string()
            } else {
                format!("il y a {d} jours")
            }
        }
        _ => {
            let m = delta / 2_592_000;
            if m == 1 {
                "il y a 1 mois".to_string()
            } else {
                format!("il y a {m} mois")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Bibliothèque
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub games: Vec<Game>,
    pub settings: Settings,
}

impl Library {
    pub fn load() -> Self {
        let path = config_path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match serde_json::from_str::<Library>(&text) {
            Ok(lib) => lib,
            Err(err) => {
                eprintln!("[quark] config illisible ({}): {err}", path.display());
                let backup = config_dir().join("config.json.corrompu");
                let _ = std::fs::copy(&path, &backup);
                Self::default()
            }
        }
    }

    pub fn save(&self) -> Result<(), String> {
        std::fs::create_dir_all(config_dir()).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = config_dir().join("config.json.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, config_path()).map_err(|e| e.to_string())
    }

    pub fn get(&self, id: &str) -> Option<&Game> {
        self.games.iter().find(|g| g.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Game> {
        self.games.iter_mut().find(|g| g.id == id)
    }

    pub fn remove(&mut self, id: &str) {
        self.games.retain(|g| g.id != id);
    }

    /// Jeux triés comme dans Steam : ordre alphabétique insensible à la casse.
    pub fn sorted_ids(&self) -> Vec<String> {
        let mut games: Vec<&Game> = self.games.iter().collect();
        games.sort_by_key(|g| g.name.to_lowercase());
        games.into_iter().map(|g| g.id.clone()).collect()
    }
}
