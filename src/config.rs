//! Configuration, bibliothèque de jeux et persistance de Quark.
//!
//! Tout vit dans deux dossiers :
//!   * `~/.config/quark/config.json`      -> réglages + jeux (lisible/éditable à la main)
//!   * `~/.local/share/quark/`            -> artworks mis en cache + préfixes Proton par défaut

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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

    /// Retire l'artwork de ce type : le rendu de secours reprend sa place.
    pub fn clear(&mut self, kind: AssetKind) {
        match kind {
            AssetKind::Cover => self.cover = None,
            AssetKind::Hero => self.hero = None,
            AssetKind::Logo => self.logo = None,
            AssetKind::Icon => self.icon = None,
        }
    }
}

// ---------------------------------------------------------------------------
// Variables d'environnement
// ---------------------------------------------------------------------------

/// Variable d'environnement passée au jeu (ou à tous les jeux, depuis les
/// réglages généraux).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

impl EnvVar {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }

    /// « WINEDLLOVERRIDES=winmm=n,b;steam_api64 » -> nom + valeur (premier `=`).
    /// Renvoie `None` si la ligne ne contient pas de nom exploitable.
    pub fn parse(line: &str) -> Option<Self> {
        let (name, value) = line.split_once('=')?;
        let name = name.trim();
        if name.is_empty() || name.contains(char::is_whitespace) {
            return None;
        }
        Some(Self {
            name: name.to_string(),
            value: value.trim().to_string(),
        })
    }

    /// Nom utilisable par `Command::env` (sinon la variable est ignorée).
    pub fn is_valid(&self) -> bool {
        let name = self.name.trim();
        !name.is_empty() && !name.contains(char::is_whitespace)
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

/// Runtime anti-triche fourni par Steam (BattlEye / EasyAntiCheat).
///
/// C'est une option cochable par jeu, comme dans Heroic : elle ne change rien
/// tant qu'aucun runtime correspondant n'est installé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AnticheatMode {
    #[default]
    None,
    BattlEye,
    EasyAntiCheat,
}

impl AnticheatMode {
    pub const ALL: [AnticheatMode; 3] = [
        AnticheatMode::None,
        AnticheatMode::BattlEye,
        AnticheatMode::EasyAntiCheat,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AnticheatMode::None => "Aucun",
            AnticheatMode::BattlEye => "BattlEye",
            AnticheatMode::EasyAntiCheat => "EasyAntiCheat",
        }
    }

    /// Motif recherché dans le nom du runtime installé par Steam.
    pub fn keyword(self) -> Option<&'static str> {
        match self {
            AnticheatMode::None => None,
            AnticheatMode::BattlEye => Some("battleye"),
            AnticheatMode::EasyAntiCheat => Some("easyanticheat"),
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
    /// Le dossier est utilisé tel quel : Quark n'y crée aucun sous-dossier.
    pub default_prefix: Option<String>,
    /// Variables d'environnement appliquées à tous les lancements (le jeu peut
    /// écraser une variable du même nom).
    pub env: Vec<EnvVar>,
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
            env: Vec::new(),
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
    /// Variables d'environnement propres au jeu (prioritaires sur celles des
    /// réglages généraux).
    pub env: Vec<EnvVar>,
    /// Runtime anti-triche à utiliser pour ce jeu (aucun par défaut).
    pub anticheat: AnticheatMode,
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
            env: Vec::new(),
            anticheat: AnticheatMode::None,
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

    /// Préfixe Proton effectif : jeu > réglages > dossier géré par Quark.
    ///
    /// Le dossier choisi (par le jeu ou par les réglages) est utilisé **tel
    /// quel** : Quark n'y crée aucun sous-dossier, donc le préfixe est
    /// exactement celui que tu as sélectionné. Seul le repli automatique
    /// (aucun préfixe indiqué) range les préfixes sous
    /// `~/.local/share/quark/prefixes/`.
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

    /// Lettre de secours quand le jeu n'a pas d'icône (voir [`monogram_letter`]).
    pub fn monogram_letter(&self) -> String {
        monogram_letter(&self.name)
    }
}

/// Articles ignorés pour choisir la lettre d'un jeu sans icône.
const ARTICLES: [&str; 16] = [
    "the", "a", "an", "le", "la", "les", "l", "un", "une", "des", "el", "los", "las", "il",
    "lo", "der",
];

/// Première lettre d'un titre, articles initiaux ignorés : « The Witcher 3 »
/// donne « W », « Les Chevaliers de Baphomet » donne « C », « 7 Days to Die »
/// donne « 7 ». Un titre vide donne « ? ».
pub fn monogram_letter(name: &str) -> String {
    let mut rest = name.trim();
    loop {
        let mut stripped: Option<&str> = None;
        for article in ARTICLES {
            let length = article.len();
            // L'article doit être un mot à part entière : suivi d'un espace, d'une
            // apostrophe ou d'un tiret (« l'Abbaye », « Les-Chevaliers »).
            if rest.len() <= length || !rest.is_char_boundary(length) {
                continue;
            }
            if !rest[..length].eq_ignore_ascii_case(article) {
                continue;
            }
            let tail = &rest[length..];
            let separateur = matches!(tail.chars().next(), Some(' ' | '\'' | '\u{2019}' | '-'));
            if separateur {
                stripped = Some(tail.trim_start_matches(|c: char| {
                    c == ' ' || c == '\'' || c == '\u{2019}' || c == '-'
                }));
                break;
            }
        }
        match stripped {
            Some(next) if !next.is_empty() => rest = next,
            _ => break,
        }
    }
    rest.chars()
        .find(|c| c.is_alphanumeric())
        .or_else(|| name.trim().chars().next())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn game(id: &str) -> Game {
        Game {
            id: id.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn lettre_ignore_les_articles_initiaux() {
        assert_eq!(monogram_letter("The Witcher 3: Wild Hunt"), "W");
        assert_eq!(monogram_letter("A Short Hike"), "S");
        assert_eq!(monogram_letter("An Anno 1800"), "A");
        assert_eq!(monogram_letter("Les Chevaliers de Baphomet"), "C");
        assert_eq!(monogram_letter("l'Abbaye des Morts"), "A");
    }

    #[test]
    fn lettre_sans_article_ou_vide() {
        assert_eq!(monogram_letter("Celeste"), "C");
        assert_eq!(monogram_letter("7 Days to Die"), "7");
        assert_eq!(monogram_letter("Another World"), "A");
        assert_eq!(monogram_letter("   "), "?");
        assert_eq!(monogram_letter("A"), "A");
    }

    #[test]
    fn prefix_sans_reglage_est_propre_au_jeu() {
        let mut settings = Settings::default();
        settings.default_prefix = None;
        assert_eq!(game("abc").prefix_path(&settings), prefix_root().join("abc"));
    }

    #[test]
    fn variables_d_un_jeu_absent_se_chargent_vide() {
        // Config d'une version précédente : « env » n'existe pas encore.
        let json = r#"{ "name": "Vieux jeu", "exe": "/tmp/vieux.exe" }"#;
        let game: Game = serde_json::from_str(json).unwrap();
        assert!(game.env.is_empty());
        let settings: Settings = serde_json::from_str(r#"{ "language": "english" }"#).unwrap();
        assert!(settings.env.is_empty());
    }

    #[test]
    fn prefix_par_defaut_utilise_le_dossier_choisi() {
        let mut settings = Settings::default();
        settings.default_prefix = Some("/tmp/quark-tests/prefixes".to_string());
        // Aucun sous-dossier : le dossier indiqué est le préfixe, tel quel.
        assert_eq!(
            game("abc").prefix_path(&settings),
            PathBuf::from("/tmp/quark-tests/prefixes")
        );
    }

    #[test]
    fn variables_d_environnement_analysees() {
        let var = EnvVar::parse("WINEDLLOVERRIDES=winmm=n,b;steam_api64").unwrap();
        assert_eq!(var.name, "WINEDLLOVERRIDES");
        assert_eq!(var.value, "winmm=n,b;steam_api64");
        // Valeur vide : acceptée, c'est « NOM= » qui compte.
        assert_eq!(EnvVar::parse("DXVK_HUD=").unwrap().value, "");
        // Rien d'exploitable : pas de nom.
        assert!(EnvVar::parse("WINEDLLOVERRIDES").is_none());
        assert!(EnvVar::parse("=valeur").is_none());
        assert!(!EnvVar::new("  ", "x").is_valid());
    }

    #[test]
    fn prefix_du_jeu_ecrase_le_prefixe_par_defaut() {
        let mut settings = Settings::default();
        settings.default_prefix = Some("/tmp/quark-tests/defaut".to_string());
        let mut g = game("abc");
        g.prefix = Some("/tmp/quark-tests/mon-jeu".to_string());
        assert_eq!(g.prefix_path(&settings), PathBuf::from("/tmp/quark-tests/mon-jeu"));
    }
}
