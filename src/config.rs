//! Configuration, bibliothèque de jeux et persistance de Quark.
//!
//! Tout vit dans deux dossiers :
//!   * `~/.config/quark/config.json`      -> réglages + jeux (lisible/éditable à la main)
//!   * `~/.local/share/quark/`            -> artworks mis en cache + préfixes Proton par défaut

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashSet;
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

/// Ramène une lettre accentuée à sa forme simple. Partagé par la recherche
/// de la bibliothèque et la palette : les deux doivent répondre à « parametres »
/// comme à « paramètres ».
pub fn fold_char(ch: char) -> char {
    match ch {
        'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        'œ' => 'o',
        'æ' => 'a',
        _ => ch,
    }
}

/// Forme comparable d'un texte : minuscules, sans accents. Les champs sont
/// comparés séparément par `Game::matches_query`, pour éviter qu'une recherche
/// se disperse entre le titre et les mots-clés.
pub fn normalized_search(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(fold_char)
        .collect()
}

pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

pub fn config_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("QUARK_CONFIG_DIR") {
        return PathBuf::from(path);
    }
    dirs::config_dir()
        .unwrap_or_else(|| home_dir().join(".config"))
        .join("quark")
}

pub fn data_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("QUARK_DATA_DIR") {
        return PathBuf::from(path);
    }
    dirs::data_dir()
        .unwrap_or_else(|| home_dir().join(".local/share"))
        .join("quark")
}

/// Mémoire résidente du processus, en mégaoctets, lue dans `/proc`.
///
/// Affichée dans « À propos » : c'est la mesure qui permet de vérifier que la
/// consommation ne grimpe plus quand on parcourt la bibliothèque.
pub fn resident_mb() -> Option<u64> {
    parse_vmrss(&std::fs::read_to_string("/proc/self/status").ok()?)
}

/// Extrait `VmRSS` (en Ko) d'un contenu de `/proc/self/status`, converti en Mo.
pub fn parse_vmrss(status: &str) -> Option<u64> {
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb / 1024);
        }
    }
    None
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

/// Fichier séparé des identifiants boutiques (droits privés, non chiffré).
pub fn credentials_path() -> PathBuf {
    config_dir().join("credentials.json")
}

/// Dossier racine des artworks téléchargés.
pub fn artwork_root() -> PathBuf {
    data_dir().join("artwork")
}

/// Dossier racine des préfixes Proton créés par Quark.
pub fn prefix_root() -> PathBuf {
    data_dir().join("prefixes")
}

/// Dossier des journaux d'un jeu : un fichier par session, plus les journaux
/// Proton quand l'option est active.
pub fn log_dir(game_id: &str) -> PathBuf {
    data_dir().join("logs").join(game_id)
}

/// Journal de la dernière session (stdout + stderr du jeu et de Proton).
pub fn session_log(game_id: &str) -> PathBuf {
    log_dir(game_id).join("session.log")
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

    pub fn label(self) -> String {
        crate::i18n::tr(match self {
            AssetKind::Cover => "couverture",
            AssetKind::Hero => "bannière",
            AssetKind::Logo => "logo",
            AssetKind::Icon => "icône",
        })
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

/// Variable Wine qui porte les overrides de DLL d'un jeu.
pub const DLL_OVERRIDES_VAR: &str = "WINEDLLOVERRIDES";

/// Une DLL surchargée dans `WINEDLLOVERRIDES`.
///
/// `native` correspond à `n` (DLL fournie avec le jeu), `builtin` à `b`
/// (DLL de Wine). Les deux peuvent être actifs : Wine essaie alors `n` puis `b`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DllOverride {
    pub name: String,
    pub native: bool,
    pub builtin: bool,
}

impl DllOverride {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            native: true,
            builtin: false,
        }
    }

    /// Ordre demandé à Wine, au format `WINEDLLOVERRIDES`.
    pub fn modes(&self) -> &'static str {
        match (self.native, self.builtin) {
            (true, true) => "n,b",
            (true, false) => "n",
            (false, true) => "b",
            (false, false) => "n,b",
        }
    }
}

/// Compare deux noms de DLL en ignorant la casse et le suffixe `.dll`, que
/// Wine accepte mais que le dossier source affiche naturellement.
pub fn dll_name_eq(left: &str, right: &str) -> bool {
    fn stem(name: &str) -> &str {
        let name = name.trim();
        name.strip_suffix(".dll")
            .or_else(|| name.strip_suffix(".DLL"))
            .unwrap_or(name)
    }
    stem(left).eq_ignore_ascii_case(stem(right))
}

/// Lit `WINEDLLOVERRIDES` (dernière valeur gagnante, comme au lancement).
/// Une entrée sans `=`, comme `steam_api64`, reprend l'ordre par défaut `n,b`.
pub fn parse_dll_overrides(value: &str) -> Vec<DllOverride> {
    let mut out: Vec<DllOverride> = Vec::new();
    for raw in value.split(';') {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let (name, modes) = raw.split_once('=').unwrap_or((raw, "n,b"));
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let mut native = false;
        let mut builtin = false;
        for mode in modes.split(',').map(str::trim) {
            match mode.to_ascii_lowercase().as_str() {
                "n" | "native" => native = true,
                "b" | "builtin" => builtin = true,
                _ => {}
            }
        }
        if !native && !builtin {
            native = true;
            builtin = true;
        }
        let entry = DllOverride {
            name: name.to_string(),
            native,
            builtin,
        };
        if let Some(existing) = out.iter_mut().find(|row| dll_name_eq(&row.name, name)) {
            *existing = entry;
        } else {
            out.push(entry);
        }
    }
    out
}

/// Overrides déjà enregistrés dans les variables d'environnement du jeu.
pub fn dll_overrides_from_env(env: &[EnvVar]) -> Vec<DllOverride> {
    env.iter()
        .rev()
        .find(|var| var.name.eq_ignore_ascii_case(DLL_OVERRIDES_VAR))
        .map(|var| parse_dll_overrides(&var.value))
        .unwrap_or_default()
}

/// Sérialise les overrides dans l'ordre affiché, en ignorant les lignes vides.
pub fn dll_overrides_value(overrides: &[DllOverride]) -> String {
    overrides
        .iter()
        .filter_map(|row| {
            let name = row.name.trim();
            (!name.is_empty()).then(|| format!("{name}={}", row.modes()))
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// Remplace `WINEDLLOVERRIDES` sans toucher aux autres variables. Une liste
/// vide retire la variable, ce qui rend à Wine son comportement par défaut.
pub fn set_dll_overrides(env: &mut Vec<EnvVar>, overrides: &[DllOverride]) {
    let value = dll_overrides_value(overrides);
    if value.is_empty() {
        env.retain(|var| !var.name.eq_ignore_ascii_case(DLL_OVERRIDES_VAR));
        return;
    }
    let mut inserted = false;
    env.retain(|var| {
        if !var.name.eq_ignore_ascii_case(DLL_OVERRIDES_VAR) {
            return true;
        }
        if inserted {
            return false;
        }
        inserted = true;
        true
    });
    if let Some(var) = env
        .iter_mut()
        .find(|var| var.name.eq_ignore_ascii_case(DLL_OVERRIDES_VAR))
    {
        var.value = value;
    } else {
        env.push(EnvVar::new(DLL_OVERRIDES_VAR, value));
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
    pub fn label(self) -> String {
        crate::i18n::tr(match self {
            ViewMode::List => "Liste",
            ViewMode::Grid => "Grille",
        })
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
    CatppuccinMocha,
    CatppuccinMacchiato,
    CatppuccinFrappe,
    Dracula,
    Nord,
    GruvboxDark,
    TokyoNight,
    RosePine,
    Everforest,
    OneDark,
    Kanagawa,
}

impl ThemeMode {
    pub const ALL: [ThemeMode; 13] = [
        ThemeMode::Amoled,
        ThemeMode::Graphite,
        ThemeMode::CatppuccinMocha,
        ThemeMode::CatppuccinMacchiato,
        ThemeMode::CatppuccinFrappe,
        ThemeMode::Dracula,
        ThemeMode::Nord,
        ThemeMode::GruvboxDark,
        ThemeMode::TokyoNight,
        ThemeMode::RosePine,
        ThemeMode::Everforest,
        ThemeMode::OneDark,
        ThemeMode::Kanagawa,
    ];

    pub fn label(self) -> String {
        crate::i18n::tr(match self {
            ThemeMode::Amoled => "AMOLED",
            ThemeMode::Graphite => "Graphite",
            ThemeMode::CatppuccinMocha => "Catppuccin Mocha",
            ThemeMode::CatppuccinMacchiato => "Catppuccin Macchiato",
            ThemeMode::CatppuccinFrappe => "Catppuccin Frappé",
            ThemeMode::Dracula => "Dracula",
            ThemeMode::Nord => "Nord",
            ThemeMode::GruvboxDark => "Gruvbox Dark",
            ThemeMode::TokyoNight => "Tokyo Night",
            ThemeMode::RosePine => "Rosé Pine",
            ThemeMode::Everforest => "Everforest",
            ThemeMode::OneDark => "One Dark",
            ThemeMode::Kanagawa => "Kanagawa",
        })
    }

    /// Description courte affichée sous le nom du thème.
    pub fn description(self) -> String {
        crate::i18n::tr(match self {
            ThemeMode::Amoled => "Noir pur, contraste maximal",
            ThemeMode::Graphite => "Noir doux et ardoise",
            ThemeMode::CatppuccinMocha => "Pastel chaleureux sur espresso",
            ThemeMode::CatppuccinMacchiato => "Pastel doux sur bleu nuit",
            ThemeMode::CatppuccinFrappe => "Pastel froid sur ardoise",
            ThemeMode::Dracula => "Violet et cyan sur nuit",
            ThemeMode::Nord => "Bleu arctique et brume",
            ThemeMode::GruvboxDark => "Ambre et terre cuite",
            ThemeMode::TokyoNight => "Bleu électrique et néon",
            ThemeMode::RosePine => "Rose poudré et mauve",
            ThemeMode::Everforest => "Vert forêt et sauge",
            ThemeMode::OneDark => "Bleu acier et graphite",
            ThemeMode::Kanagawa => "Encre japonaise et indigo",
        })
    }
}

fn deserialize_theme<'de, D>(deserializer: D) -> Result<ThemeMode, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    Ok(match value.trim().to_ascii_lowercase().as_str() {
        "graphite" => ThemeMode::Graphite,
        "catppuccin_mocha" | "catppuccin-mocha" => ThemeMode::CatppuccinMocha,
        "catppuccin_macchiato" | "catppuccin-macchiato" => ThemeMode::CatppuccinMacchiato,
        "catppuccin_frappe" | "catppuccin-frappe" | "catppuccin_frappé" => {
            ThemeMode::CatppuccinFrappe
        }
        "dracula" => ThemeMode::Dracula,
        "nord" => ThemeMode::Nord,
        "gruvbox_dark" | "gruvbox-dark" => ThemeMode::GruvboxDark,
        "tokyo_night" | "tokyo-night" => ThemeMode::TokyoNight,
        "rose_pine" | "rose-pine" => ThemeMode::RosePine,
        "everforest" => ThemeMode::Everforest,
        "one_dark" | "one-dark" => ThemeMode::OneDark,
        "kanagawa" => ThemeMode::Kanagawa,
        _ => ThemeMode::Amoled,
    })
}

/// Langue de l'interface. Elle est distincte de `Settings.language`, qui est
/// la langue demandée aux API Steam pour les fiches de boutique.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UiLanguage {
    #[default]
    French,
    English,
}

impl UiLanguage {
    pub const ALL: [UiLanguage; 2] = [UiLanguage::French, UiLanguage::English];

    pub fn label(self) -> String {
        crate::i18n::tr(match self {
            UiLanguage::French => "Français",
            UiLanguage::English => "English",
        })
    }

    pub fn short(self) -> &'static str {
        match self {
            UiLanguage::French => "FR",
            UiLanguage::English => "EN",
        }
    }
}

/// Tri des jeux sur la page d'accueil.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HomeSort {
    /// Dernière session la plus récente d'abord.
    #[default]
    Recent,
    /// Date d'ajout à la bibliothèque, la plus récente d'abord.
    Added,
    /// Nombre d'heures jouées, le plus élevé d'abord.
    Playtime,
}

impl HomeSort {
    pub const ALL: [HomeSort; 3] = [HomeSort::Recent, HomeSort::Added, HomeSort::Playtime];

    /// Nombre de jeux montrés dans le carrousel de l'accueil.
    pub const CAROUSEL: usize = 5;

    pub fn label(self) -> String {
        crate::i18n::tr(match self {
            HomeSort::Recent => "Joués récemment",
            HomeSort::Added => "Ajoutés récemment",
            HomeSort::Playtime => "Plus joués",
        })
    }
}

/// Page affichée au lancement de Quark.
///
/// L'accueil reste la valeur par défaut, y compris pour les configurations
/// existantes qui ne connaissent pas encore ce champ. Une valeur inconnue
/// écrite à la main retombe sur l'accueil au lieu d'empêcher le chargement de
/// toute la bibliothèque.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StartPage {
    #[default]
    Home,
    Library,
    Store,
    MyGames,
}

impl StartPage {
    pub const ALL: [StartPage; 4] = [
        StartPage::Home,
        StartPage::Library,
        StartPage::Store,
        StartPage::MyGames,
    ];

    pub fn label(self) -> String {
        crate::i18n::tr(match self {
            StartPage::Home => "Accueil",
            StartPage::Library => "Bibliothèque",
            StartPage::Store => "Boutique",
            StartPage::MyGames => "Mes jeux",
        })
    }

    fn from_key(key: &str) -> Self {
        match key.trim().to_ascii_lowercase().as_str() {
            "library" | "bibliotheque" | "bibliothèque" => StartPage::Library,
            "store" | "boutique" => StartPage::Store,
            "my_games" | "my-games" | "mes_jeux" | "mes-jeux" => StartPage::MyGames,
            _ => StartPage::Home,
        }
    }
}

fn deserialize_start_page<'de, D>(deserializer: D) -> Result<StartPage, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    Ok(StartPage::from_key(&value))
}

/// Regroupement de la vue liste. `Status` reproduit les colonnes Steam
/// historiques de Quark ; `None` montre une liste continue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GroupBy {
    #[default]
    Status,
    Store,
    Genre,
    Year,
    None,
}

impl GroupBy {
    pub const ALL: [GroupBy; 5] = [
        GroupBy::Status,
        GroupBy::Store,
        GroupBy::Genre,
        GroupBy::Year,
        GroupBy::None,
    ];

    pub fn label(self) -> String {
        crate::i18n::tr(match self {
            GroupBy::Status => "Statut",
            GroupBy::Store => "Boutique",
            GroupBy::Genre => "Genre",
            GroupBy::Year => "Année",
            GroupBy::None => "Aucun",
        })
    }

    fn from_key(key: &str) -> Self {
        match key.trim().to_ascii_lowercase().as_str() {
            "store" | "boutique" => GroupBy::Store,
            "genre" => GroupBy::Genre,
            "year" | "annee" | "année" => GroupBy::Year,
            "none" | "aucun" => GroupBy::None,
            _ => GroupBy::Status,
        }
    }
}

fn deserialize_group_by<'de, D>(deserializer: D) -> Result<GroupBy, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    Ok(GroupBy::from_key(&value))
}

/// Ordre de la liste et de la grille de la bibliothèque. Le nom reste l'ordre
/// historique ; les autres choix sont enregistrés dans les vues.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LibrarySort {
    #[default]
    Name,
    Recent,
    Added,
    Playtime,
}

impl LibrarySort {
    pub const ALL: [LibrarySort; 4] = [
        LibrarySort::Name,
        LibrarySort::Recent,
        LibrarySort::Added,
        LibrarySort::Playtime,
    ];

    pub fn label(self) -> String {
        crate::i18n::tr(match self {
            LibrarySort::Name => "Nom",
            LibrarySort::Recent => "Dernière session",
            LibrarySort::Added => "Ajout récent",
            LibrarySort::Playtime => "Temps joué",
        })
    }

    fn from_key(key: &str) -> Self {
        match key.trim().to_ascii_lowercase().as_str() {
            "recent" => LibrarySort::Recent,
            "added" => LibrarySort::Added,
            "playtime" => LibrarySort::Playtime,
            _ => LibrarySort::Name,
        }
    }
}

fn deserialize_library_sort<'de, D>(deserializer: D) -> Result<LibrarySort, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    Ok(LibrarySort::from_key(&value))
}

/// Une vue nommée de la bibliothèque : filtres, recherche, ordre et
/// regroupement. Elle ne contient aucun chemin ni état d'interface volatile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SavedView {
    pub name: String,
    pub status: GameStatus,
    pub tag: String,
    pub genre: String,
    pub query: String,
    #[serde(deserialize_with = "deserialize_group_by")]
    pub group_by: GroupBy,
    pub view: ViewMode,
    #[serde(deserialize_with = "deserialize_library_sort")]
    pub sort: LibrarySort,
}

impl SavedView {
    pub const MAX_NAME_CHARS: usize = 48;

    pub fn capture(name: &str, settings: &Settings, query: &str) -> Self {
        Self {
            name: name.trim().chars().take(Self::MAX_NAME_CHARS).collect(),
            status: settings.home_status,
            tag: settings.home_tag.trim().to_string(),
            genre: settings.home_genre.trim().to_string(),
            query: query.trim().chars().take(120).collect(),
            group_by: settings.group_by,
            view: settings.view,
            sort: settings.library_sort,
        }
    }

    /// Nettoie une vue lue sur le disque. Une vue sans nom est inutilisable :
    /// elle est ignorée plutôt que de créer une entrée vide dans le menu.
    pub fn normalized(mut self) -> Option<Self> {
        self.name = self
            .name
            .trim()
            .chars()
            .take(Self::MAX_NAME_CHARS)
            .collect();
        if self.name.is_empty() {
            return None;
        }
        self.tag = self.tag.trim().to_string();
        self.genre = self.genre.trim().to_string();
        self.query = self.query.trim().chars().take(120).collect();
        Some(self)
    }

    pub fn apply_to(&self, settings: &mut Settings) {
        settings.home_status = self.status;
        settings.home_tag = self.tag.clone();
        settings.home_genre = self.genre.clone();
        settings.group_by = self.group_by;
        settings.view = self.view;
        settings.library_sort = self.sort;
    }

    pub fn matches(&self, settings: &Settings, query: &str) -> bool {
        self.status == settings.home_status
            && self.tag.eq_ignore_ascii_case(settings.home_tag.trim())
            && self.genre.eq_ignore_ascii_case(settings.home_genre.trim())
            && self.query.eq_ignore_ascii_case(query.trim())
            && self.group_by == settings.group_by
            && self.view == settings.view
            && self.sort == settings.library_sort
    }
}

/// Un groupe de la vue liste : un titre, une couleur éventuelle et les jeux
/// dans l'ordre déjà trié. Les vues sans regroupement ont un seul groupe au
/// titre vide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameGroup {
    pub key: String,
    pub label: String,
    pub color: Option<(u8, u8, u8)>,
    pub ids: Vec<String>,
    rank: i32,
}

/// Statut personnel d'un jeu, comme les colonnes de la bibliothèque Steam.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GameStatus {
    /// Aucun statut : le jeu n'est range dans aucune colonne.
    #[default]
    None,
    Backlog,
    Playing,
    Finished,
    Dropped,
}

impl GameStatus {
    pub const ALL: [GameStatus; 5] = [
        GameStatus::None,
        GameStatus::Backlog,
        GameStatus::Playing,
        GameStatus::Finished,
        GameStatus::Dropped,
    ];

    /// Ordre des sections de la vue liste, comme les colonnes Steam : ce qui
    /// est commencé d'abord, les jeux sans statut à la fin.
    pub const COLUMNS: [GameStatus; 5] = [
        GameStatus::Playing,
        GameStatus::Backlog,
        GameStatus::Finished,
        GameStatus::Dropped,
        GameStatus::None,
    ];

    pub fn label(self) -> String {
        crate::i18n::tr(match self {
            GameStatus::None => "Aucun",
            GameStatus::Backlog => "\u{c0} jouer",
            GameStatus::Playing => "En cours",
            GameStatus::Finished => "Termin\u{e9}",
            GameStatus::Dropped => "Abandonn\u{e9}",
        })
    }

    /// Titre de section : « Sans statut » plutôt que « Aucun ».
    pub fn column_label(self) -> String {
        match self {
            GameStatus::None => crate::i18n::tr("Sans statut"),
            other => other.label(),
        }
    }

    /// Couleur de la pastille de statut.
    pub fn color(self) -> (u8, u8, u8) {
        match self {
            GameStatus::None => (0x6A, 0x70, 0x76),
            GameStatus::Backlog => (0x5A, 0x8F, 0xD6),
            GameStatus::Playing => (0x4C, 0xB0, 0x6A),
            GameStatus::Finished => (0xB4, 0xC7, 0xDC),
            GameStatus::Dropped => (0xC9, 0x6A, 0x5A),
        }
    }
}

/// Une session de jeu termin\u{e9}e : son d\u{e9}but et sa dur\u{e9}e. C'est ce qui
/// permet les statistiques (heures par semaine, calendrier, s\u{e9}rie de jours).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaySession {
    /// D\u{e9}but de la session (epoch Unix).
    pub start: i64,
    /// Dur\u{e9}e en secondes.
    pub secs: u64,
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
    /// Les deux runtimes : Heroic permet aussi de cocher les deux cases.
    Both,
}

impl AnticheatMode {
    pub fn battleye(self) -> bool {
        matches!(self, AnticheatMode::BattlEye | AnticheatMode::Both)
    }

    pub fn eac(self) -> bool {
        matches!(self, AnticheatMode::EasyAntiCheat | AnticheatMode::Both)
    }

    /// Même mode avec BattlEye coché ou non (les deux options sont
    /// indépendantes, comme dans Heroic).
    pub fn with_battleye(self, on: bool) -> Self {
        match (on, self.eac()) {
            (true, true) => AnticheatMode::Both,
            (true, false) => AnticheatMode::BattlEye,
            (false, true) => AnticheatMode::EasyAntiCheat,
            (false, false) => AnticheatMode::None,
        }
    }

    /// Même mode avec EasyAntiCheat coché ou non.
    pub fn with_eac(self, on: bool) -> Self {
        match (self.battleye(), on) {
            (true, true) => AnticheatMode::Both,
            (true, false) => AnticheatMode::BattlEye,
            (false, true) => AnticheatMode::EasyAntiCheat,
            (false, false) => AnticheatMode::None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AnticheatMode::None => "aucun",
            AnticheatMode::BattlEye => "BattlEye",
            AnticheatMode::EasyAntiCheat => "EasyAntiCheat",
            AnticheatMode::Both => "BattlEye + EasyAntiCheat",
        }
    }

    /// Motifs cherchés dans les noms de dossiers installés par Steam.
    pub fn keywords(self) -> &'static [&'static str] {
        match self {
            AnticheatMode::None => &[],
            AnticheatMode::BattlEye => &["battleye"],
            AnticheatMode::EasyAntiCheat => &["easyanticheat", "eac_runt"],
            AnticheatMode::Both => &["battleye", "easyanticheat"],
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
    /// Tri des jeux sur la page d'accueil.
    pub home_sort: HomeSort,
    #[serde(deserialize_with = "deserialize_theme")]
    pub theme: ThemeMode,
    /// Langue de l'interface, indépendante de la langue des fiches Steam.
    pub ui_language: UiLanguage,
    /// Le parcours de bienvenue a été terminé. Les configurations existantes
    /// sont marquées comme déjà accueillies lors de la migration.
    pub onboarding_done: bool,
    /// Dernière version dont le résumé a été lu. Vide sur une installation
    /// neuve ou une configuration antérieure à 1.2.
    pub last_seen_version: String,
    /// « Mes jeux » en liste (une ligne par jeu, bouton d'action visible) au lieu
    /// de la grille de jaquettes. Faux par défaut : l'affichage historique.
    pub my_games_list: bool,
    /// Affiche un hero aléatoire flouté derrière l'accueil et la bibliothèque.
    pub hero_background: bool,
    /// Rayon du flou du fond, en pixels logiques.
    pub hero_background_blur: f32,
    /// Clé API SteamGridDB (https://www.steamgriddb.com/profile/preferences/api).
    pub sgdb_api_key: String,
    /// Chemin explicite de PCSX2. Vide = détection dans le PATH et Flatpak.
    pub pcsx2_path: String,
    /// Pseudo RetroAchievements, utilisé avec la clé Web API ci-dessous.
    pub ra_username: String,
    /// Clé Web API RetroAchievements (profil → Keys).
    pub ra_api_key: String,
    /// Langue demandée à l'API Steam (storesearch).
    pub language: String,
    /// Pays demandé à l'API Steam (storesearch).
    pub country: String,
    /// Runtime Proton prérempli dans chaque jeu ajouté (None = meilleur
    /// détecté au lancement).
    pub default_proton: Option<String>,
    /// Préfixe Proton prérempli dans chaque jeu ajouté. Le dossier est utilisé
    /// tel quel : Quark n'y crée aucun sous-dossier.
    pub default_prefix: Option<String>,
    /// Dossier parent des installations Epic/GOG. Vide = `~/Games`.
    /// C'est le dossier proposé dans la confirmation d'installation ; il peut
    /// être changé pour une installation précise sans modifier ce réglage.
    pub default_install_dir: Option<String>,
    /// Arguments préremplis dans chaque jeu ajouté.
    pub default_args: String,
    /// Anti-triche prérempli dans chaque jeu ajouté.
    pub default_anticheat: AnticheatMode,
    /// Options de lancement (GameMode, MangoHud, gamescope, PRIME, wrapper)
    /// recopiées dans chaque jeu ajouté.
    pub default_launch: LaunchOptions,
    /// `PROTON_LOG=1` recopié dans chaque jeu ajouté.
    pub default_proton_log: bool,
    /// Lancement par umu-launcher recopié dans chaque jeu ajouté.
    pub default_umu: bool,
    /// Variables d'environnement recopiées dans chaque jeu ajouté.
    ///
    /// Ce ne sont **pas** des variables globales : chaque jeu en reçoit une
    /// copie à l'ajout, puis gère la sienne. Une variable ajoutée après coup
    /// ici ne touche donc aucun jeu existant.
    #[serde(alias = "env")]
    pub default_env: Vec<EnvVar>,
    /// Migration des anciennes variables globales : elles ont été recopiées
    /// une fois dans les jeux qui n'en avaient pas.
    pub defaults_migrated: bool,
    /// Migration du préfixe par défaut : il a été recopié une fois dans les
    /// jeux qui n'avaient encore ni préfixe propre ni préfixe d'installation.
    pub prefix_defaults_migrated: bool,
    /// Vérifie et installe automatiquement les mises à jour au démarrage.
    /// Désactivable dans « À propos » ; les binaires de développement et les
    /// installations en lecture seule sont ignorés.
    pub auto_update: bool,
    /// Garde Quark dans la zone de notification : la croix ne quitte plus
    /// l'application, elle la range dans l'icône.
    pub tray_icon: bool,
    /// Overlay en jeu : panneau au-dessus du jeu, captures et succès.
    pub overlay: bool,
    /// Touche de capture d'écran globale (F12 par défaut).
    pub screenshot_key: String,
    /// Raccourci global qui ouvre ou ferme l'overlay (Maj+Retour arrière).
    pub overlay_hotkey: String,
    /// Affiche aussi les succès verrouillés dans l'overlay.
    pub overlay_show_locked: bool,
    /// Version compacte des lignes de succès dans l'overlay.
    pub overlay_compact: bool,
    /// Affiche la progression et la rareté dans l'overlay.
    pub overlay_show_progress: bool,
    /// Synchronise les sauvegardes cloud Epic/GOG après installation et à la
    /// fermeture du jeu.
    pub cloud_saves: bool,
    /// Fait défiler le carrousel de l'accueil tout seul.
    pub carousel_auto: bool,
    /// Intervalle du défilement automatique, en secondes.
    pub carousel_secs: f32,
    /// Filtre de genre de la page d'accueil (vide = tous).
    pub home_genre: String,
    /// Filtre de tag personnel (vide = tous).
    pub home_tag: String,
    /// Filtre de statut (`None` = tous).
    pub home_status: GameStatus,
    /// Page ouverte au prochain démarrage.
    #[serde(deserialize_with = "deserialize_start_page")]
    pub start_page: StartPage,
    /// Regroupement de la vue liste.
    #[serde(deserialize_with = "deserialize_group_by")]
    pub group_by: GroupBy,
    /// Ordre de la liste et de la grille de la bibliothèque.
    #[serde(deserialize_with = "deserialize_library_sort")]
    pub library_sort: LibrarySort,
    /// Vues nommées, enregistrées avec la bibliothèque.
    pub saved_views: Vec<SavedView>,
    /// Les vues de départ ont déjà été proposées une fois. Le drapeau évite
    /// de les recréer après que l'utilisateur les a supprimées.
    pub saved_views_seeded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            view: ViewMode::List,
            home_sort: HomeSort::Recent,
            theme: ThemeMode::Amoled,
            ui_language: UiLanguage::French,
            onboarding_done: false,
            last_seen_version: String::new(),
            my_games_list: false,
            hero_background: false,
            hero_background_blur: 14.0,
            sgdb_api_key: String::new(),
            pcsx2_path: String::new(),
            ra_username: String::new(),
            ra_api_key: String::new(),
            language: "french".to_string(),
            country: "FR".to_string(),
            default_proton: None,
            default_prefix: None,
            default_install_dir: None,
            default_args: String::new(),
            default_anticheat: AnticheatMode::None,
            default_launch: LaunchOptions::default(),
            default_proton_log: false,
            default_umu: false,
            default_env: Vec::new(),
            defaults_migrated: false,
            prefix_defaults_migrated: false,
            auto_update: true,
            tray_icon: true,
            overlay: true,
            screenshot_key: "F12".to_string(),
            overlay_hotkey: "Shift+BackSpace".to_string(),
            overlay_show_locked: true,
            overlay_compact: false,
            overlay_show_progress: true,
            cloud_saves: true,
            carousel_auto: true,
            carousel_secs: 8.0,
            home_genre: String::new(),
            home_tag: String::new(),
            home_status: GameStatus::None,
            start_page: StartPage::Home,
            group_by: GroupBy::Status,
            library_sort: LibrarySort::Name,
            saved_views: Vec::new(),
            saved_views_seeded: false,
        }
    }
}

impl Settings {
    /// Dossier parent proposé pour les installations Epic/GOG.
    ///
    /// Un chemin vide n'est jamais inventé : sans réglage, Quark propose
    /// `~/Games`, créé au moment de l'installation.
    pub fn install_root(&self) -> PathBuf {
        self.default_install_dir
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(expand_tilde)
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join("Games"))
    }
}

// ---------------------------------------------------------------------------
// Jeux
// ---------------------------------------------------------------------------

/// Options de lancement graphiques, réglées jeu par jeu : les cases à cocher
/// remplacent des variables (`MANGOHUD=1`) ou des commandes (`gamescope`,
/// `gamemoderun`) que tout le monde ne connaît pas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchOptions {
    /// `gamemoderun` : profil CPU de GameMode.
    pub gamemode: bool,
    /// `MANGOHUD=1` : surcouche de mesures MangoHud.
    pub mangohud: bool,
    /// gamescope autour du jeu (compositeur, mise à l'échelle).
    pub gamescope: bool,
    pub gamescope_width: u32,
    pub gamescope_height: u32,
    /// `-F fsr` : agrandissement FSR de gamescope.
    pub gamescope_fsr: bool,
    /// Limite d'images gamescope (0 = aucune).
    pub gamescope_fps: u32,
    /// Rendu sur la carte NVIDIA dédiée (PRIME offload).
    pub prime: bool,
    /// Commande wrapper : `%command%` y est remplacé par la commande du jeu.
    pub wrapper: String,
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            gamemode: false,
            mangohud: false,
            gamescope: false,
            gamescope_width: 1920,
            gamescope_height: 1080,
            gamescope_fsr: false,
            gamescope_fps: 0,
            prime: false,
            wrapper: String::new(),
        }
    }
}

impl LaunchOptions {
    /// Vrai si une option change la ligne de commande (les variables seules
    /// comptent aussi : MangoHud et PRIME passent par l'environnement).
    pub fn any(&self) -> bool {
        self.gamemode
            || self.mangohud
            || self.gamescope
            || self.prime
            || !self.wrapper.trim().is_empty()
    }

    /// Résolution gamescope, bornée pour ne jamais produire `0`.
    pub fn gamescope_size(&self) -> (u32, u32) {
        (
            self.gamescope_width.clamp(320, 7680),
            self.gamescope_height.clamp(200, 4320),
        )
    }
}

/// Une édition installée d'un jeu. Une fiche Quark peut réunir plusieurs
/// installations (Steam, Epic, GOG, locale) sans dupliquer les métadonnées, les
/// artworks, les notes ou le temps de jeu.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Installation {
    /// Stable pour la vie de la configuration : `local:<jeu>` ou
    /// `<boutique>:<identifiant produit>`.
    pub id: String,
    /// Boutique d'origine. `None` désigne une installation ajoutée à la main.
    pub provider: Option<crate::providers::Provider>,
    /// Identifiant du produit chez la boutique (App name Epic, ID GOG…).
    pub product_id: Option<String>,
    /// Nom lisible de l'édition (« Steam », « GOG »…).
    pub label: String,
    /// Dossier d'installation, quand la boutique le publie.
    pub path: Option<String>,
    /// Exécutable de cette édition.
    pub exe: String,
    /// Préfixe Proton propre à cette édition (vide = préfixe du jeu).
    pub prefix: Option<String>,
    /// Dossier de sauvegarde utilisé par la synchronisation cloud.
    pub save_path: Option<String>,
    /// Dernier horodatage de synchronisation GOG (`gogdl save-sync --ts`).
    pub save_sync_ts: Option<i64>,
    /// Dernière synchronisation cloud réussie.
    pub last_synced: Option<i64>,
    /// L'édition est réellement présente sur le disque.
    pub installed: bool,
}

impl Default for Installation {
    fn default() -> Self {
        Self {
            id: new_id(),
            provider: None,
            product_id: None,
            label: "Installation locale".to_string(),
            path: None,
            exe: String::new(),
            prefix: None,
            save_path: None,
            save_sync_ts: None,
            last_synced: None,
            installed: true,
        }
    }
}

impl Installation {
    pub fn source_label(&self) -> &'static str {
        match self.provider {
            None => "Locale",
            Some(crate::providers::Provider::Steam) => "Steam",
            Some(crate::providers::Provider::Epic) => "Epic Games",
            Some(crate::providers::Provider::Gog) => "GOG",
        }
    }

    pub fn slug(&self) -> &'static str {
        match self.provider {
            None => "local",
            Some(crate::providers::Provider::Steam) => "steam",
            Some(crate::providers::Provider::Epic) => "epic",
            Some(crate::providers::Provider::Gog) => "gog",
        }
    }
}

/// Informations propres à un jeu PlayStation 2 émulé par PCSX2.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Ps2Config {
    /// Numéro de série du disque (`SLUS-20062`), lu dans `SYSTEM.CNF`.
    pub serial: String,
    /// Identifiant de la fiche RetroAchievements, choisi par titre puis
    /// conservé : l'API ne permet pas de le déduire du numéro de série.
    pub ra_game_id: Option<u64>,
}

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
    /// Dossier analysé pour proposer des DLL dans l'onglet « DLL overrides ».
    /// Vide = dossier de l'exécutable du jeu.
    pub dll_source: Option<String>,
    /// Présent pour un jeu PlayStation 2 lancé par PCSX2. `exe` pointe alors
    /// sur l'image disque, pas sur un binaire Windows.
    pub ps2: Option<Ps2Config>,
    /// Runtime anti-triche à utiliser pour ce jeu (aucun par défaut).
    pub anticheat: AnticheatMode,
    /// Options de lancement graphiques (GameMode, MangoHud, gamescope…).
    pub launch: LaunchOptions,
    /// Lance le jeu par umu-launcher (`umu-run`) plutôt que par le script
    /// Proton de Steam.
    pub umu: bool,
    /// GAMEID transmis à umu (« 0 » quand il n'est pas renseigné).
    pub umu_game_id: String,
    /// `PROTON_LOG=1` : Proton écrit un journal détaillé dans le dossier de
    /// logs du jeu, à côté de la sortie standard capturée.
    pub proton_log: bool,
    /// Statut personnel (à jouer, en cours, terminé, abandonné).
    pub status: GameStatus,
    /// Notes personnelles, libres.
    pub notes: String,
    /// Tags personnels (« coop », « détente »…) qui servent de catégories.
    pub tags: Vec<String>,
    /// Fiche Steam : développeur, genres, date de sortie, description courte.
    pub developer: String,
    pub genres: Vec<String>,
    pub released: String,
    pub description: String,
    /// Historique des sessions, du plus ancien au plus récent.
    pub sessions: Vec<PlaySession>,
    /// Éditions installées de ce jeu. La première est créée automatiquement
    /// depuis l'ancien champ `exe` à la première ouverture.
    pub installations: Vec<Installation>,
    /// Édition utilisée par « Jouer ». Vide = la première installation.
    pub active_installation: Option<String>,
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
            dll_source: None,
            ps2: None,
            anticheat: AnticheatMode::None,
            launch: LaunchOptions::default(),
            umu: false,
            umu_game_id: String::new(),
            proton_log: false,
            status: GameStatus::None,
            notes: String::new(),
            tags: Vec::new(),
            developer: String::new(),
            genres: Vec::new(),
            released: String::new(),
            description: String::new(),
            sessions: Vec::new(),
            installations: Vec::new(),
            active_installation: None,
        }
    }
}

/// Préfixe automatique d'une installation supplémentaire.
///
/// `legacy_ready` indique que `prefix_root()/<jeu>` contient déjà un `drive_c`
/// (donc probablement les sauvegardes historiques). Dans ce cas, on ne crée
/// pas de dossier séparé : l'édition activée doit continuer à voir ces saves.
fn auto_installation_prefix(
    game_id: &str,
    slug: &str,
    installation_count: usize,
    legacy_ready: bool,
) -> Option<PathBuf> {
    (installation_count > 1 && !legacy_ready)
        .then(|| prefix_root().join(format!("{game_id}-{slug}")))
}

impl Game {
    pub fn new(name: impl Into<String>, exe: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            exe: exe.into(),
            ..Default::default()
        }
    }

    /// Recherche libre de la bibliothèque : accents ignorés, chaque champ
    /// comparé séparément (titre, développeur, tags, genres). Une recherche
    /// vide correspond à tout.
    pub fn matches_query(&self, query: &str) -> bool {
        let needle = normalized_search(query.trim());
        if needle.is_empty() {
            return true;
        }
        [self.name.as_str(), self.developer.as_str()]
            .iter()
            .any(|field| normalized_search(field).contains(&needle))
            || self
                .tags
                .iter()
                .chain(self.genres.iter())
                .any(|field| normalized_search(field).contains(&needle))
    }

    pub fn exe_path(&self) -> PathBuf {
        PathBuf::from(expand_tilde(&self.exe))
    }

    /// Installation actuellement sélectionnée pour le lancement.
    pub fn active_installation(&self) -> Option<&Installation> {
        self.active_installation
            .as_deref()
            .and_then(|id| {
                self.installations
                    .iter()
                    .find(|installation| installation.id == id)
            })
            .or_else(|| self.installations.first())
    }

    /// Bascule vers une édition installée. L'exécutable et le préfixe de
    /// l'édition deviennent ceux du jeu, ce qui laisse tout le code de
    /// lancement (Proton, umu, journal) inchangé.
    pub fn activate_installation(&mut self, id: &str) -> bool {
        let Some(installation) = self
            .installations
            .iter()
            .find(|installation| installation.id == id)
            .cloned()
        else {
            return false;
        };
        self.active_installation = Some(installation.id.clone());
        self.exe = installation.exe.clone();
        self.prefix = installation.prefix.clone();
        // Une deuxième édition ne reçoit un préfixe séparé que si le dossier
        // historique n'existe pas encore. S'il existe, il contient les saves
        // de l'utilisateur : le réutiliser évite de les rendre invisibles en
        // basculant d'une installation locale vers Epic/GOG.
        let legacy = prefix_root().join(&self.id);
        if self.prefix.is_none()
            && let Some(prefix) = auto_installation_prefix(
                &self.id,
                installation.slug(),
                self.installations.len(),
                legacy.join("drive_c").is_dir(),
            )
        {
            self.prefix = Some(prefix.display().to_string());
        }
        true
    }

    /// Ajoute ou met à jour une édition sans créer de doublon.
    pub fn upsert_installation(&mut self, installation: Installation) -> bool {
        let existing = self.installations.iter_mut().find(|current| {
            current.id == installation.id
                || (current.provider == installation.provider
                    && current.provider.is_some()
                    && current.product_id == installation.product_id)
        });
        match existing {
            Some(current) => {
                let id = current.id.clone();
                // `migrate_installations` reconstruit l'édition depuis la fiche
                // de boutique à chaque sauvegarde. Elle ne connaît ni le
                // préfixe choisi, ni l'horodatage GOG, ni le dossier de
                // sauvegarde : ces valeurs locales doivent survivre au merge.
                *current = Installation {
                    exe: if installation.exe.trim().is_empty() {
                        current.exe.clone()
                    } else {
                        installation.exe.clone()
                    },
                    prefix: current.prefix.clone().or(installation.prefix),
                    save_path: current.save_path.clone().or(installation.save_path),
                    save_sync_ts: current.save_sync_ts.or(installation.save_sync_ts),
                    last_synced: current.last_synced.or(installation.last_synced),
                    ..installation
                };
                current.id = id;
                false
            }
            None => {
                self.installations.push(installation);
                true
            }
        }
    }

    /// Crée l'installation locale des anciennes configurations et recale les
    /// champs historiques sur l'édition active.
    pub fn normalize_installations(&mut self) {
        if self.installations.is_empty() && !self.exe.trim().is_empty() {
            let path = self
                .exe_path()
                .parent()
                .map(|parent| parent.display().to_string());
            self.installations.push(Installation {
                id: format!("local:{}", self.id),
                provider: None,
                product_id: None,
                label: "Installation locale".to_string(),
                path,
                exe: self.exe.clone(),
                prefix: self.prefix.clone(),
                save_path: None,
                save_sync_ts: None,
                last_synced: None,
                installed: true,
            });
        }
        if self.active_installation.is_none() && !self.installations.is_empty() {
            self.active_installation = self.installations.first().map(|i| i.id.clone());
        }
        let active = self
            .active_installation
            .clone()
            .or_else(|| self.installations.first().map(|i| i.id.clone()));
        let exe_path = self.exe_path();
        if let Some(installation) = active.and_then(|id| {
            self.installations
                .iter_mut()
                .find(|installation| installation.id == id)
        }) {
            let belongs_to_installation = installation
                .path
                .as_deref()
                .map(|path| exe_path.starts_with(path))
                .unwrap_or(false);
            if !self.exe.trim().is_empty()
                && (installation.provider.is_none() || belongs_to_installation)
            {
                installation.exe = self.exe.clone();
            } else if !installation.exe.trim().is_empty() {
                self.exe = installation.exe.clone();
            }
            if self.prefix.is_some() {
                installation.prefix = self.prefix.clone();
            }
            self.active_installation = Some(installation.id.clone());
        }
    }

    /// Vrai si l'exécutable est un binaire Linux : il démarre sans Proton.
    pub fn is_native(&self) -> bool {
        !crate::proton::looks_windows(&self.exe_path())
    }

    /// Préfixe Proton effectif : jeu > installation active > dossier géré par
    /// Quark.
    ///
    /// Le dossier choisi est utilisé **tel quel** : Quark n'y crée aucun
    /// sous-dossier. Le préfixe « par défaut » des réglages n'est pas relu ici :
    /// il est recopié une seule fois dans chaque jeu lors de la migration, puis
    /// chaque jeu garde le sien (changer le défaut ne déplace pas les saves
    /// d'un jeu déjà configuré). Seul le repli automatique range les préfixes
    /// sous `~/.local/share/quark/prefixes/`.
    pub fn prefix_path(&self, _s: &Settings) -> PathBuf {
        if let Some(p) = self.prefix.as_ref().filter(|p| !p.trim().is_empty()) {
            return PathBuf::from(expand_tilde(p));
        }
        if let Some(p) = self
            .active_installation()
            .and_then(|installation| installation.prefix.as_ref())
            .filter(|p| !p.trim().is_empty())
        {
            return PathBuf::from(expand_tilde(p));
        }
        prefix_root().join(&self.id)
    }

    /// Préfixe Wine réellement écrit par le runtime : Proton range son
    /// `drive_c` dans `<préfixe>/pfx`, tandis qu'umu utilise directement le
    /// dossier `WINEPREFIX`. Confondre les deux fait chercher `user.reg` et les
    /// sauvegardes au mauvais endroit pour les jeux lancés par umu.
    pub fn wine_prefix_path(&self, settings: &Settings) -> PathBuf {
        let root = self.prefix_path(settings);
        if self.umu { root } else { root.join("pfx") }
    }

    pub fn playtime_label(&self) -> String {
        if self.playtime_secs == 0 {
            crate::i18n::tr("Aucune session")
        } else {
            human_playtime(self.playtime_secs)
        }
    }

    /// Vrai si l'exécutable est absent : chemin vide, ou fichier introuvable.
    /// Un nom de commande sans `/` (« steam ») n'est pas un chemin, il reste
    /// accepté (c'est le PATH qui décidera).
    pub fn exe_missing(&self) -> bool {
        let exe = self.exe.trim();
        if exe.is_empty() {
            return true;
        }
        if !exe.contains('/') {
            return false;
        }
        !self.exe_path().is_file()
    }

    /// Inscrit une session à l'historique et au temps de jeu.
    ///
    /// L'appel est idempotent pour un même début de session : Quark écrit un
    /// point de contrôle toutes les minutes pendant la partie, puis l'inscription
    /// finale met simplement la durée à jour au lieu de la compter deux fois. Si
    /// Quark est tué en cours de partie, le temps déjà joué est donc conservé.
    pub fn record_session(&mut self, start: i64, secs: u64) {
        let secs = secs.max(1);
        match self
            .sessions
            .iter_mut()
            .find(|session| session.start == start)
        {
            Some(existing) => {
                self.playtime_secs = self.playtime_secs.saturating_sub(existing.secs) + secs;
                existing.secs = secs;
            }
            None => {
                self.sessions.push(PlaySession { start, secs });
                self.playtime_secs += secs;
            }
        }
        self.last_played = Some(start + secs as i64);
        // Un jeu lancé n'est plus « à jouer » : il passe « en cours » tant que
        // le joueur ne lui a pas donné un autre statut.
        if self.status == GameStatus::Backlog {
            self.status = GameStatus::Playing;
        }
    }

    pub fn last_played_label(&self) -> String {
        match self.last_played {
            Some(t) => human_ago(t),
            None => crate::i18n::tr("Jamais lancé"),
        }
    }

    /// Lettre de secours quand le jeu n'a pas d'icône (voir [`monogram_letter`]).
    pub fn monogram_letter(&self) -> String {
        monogram_letter(&self.name)
    }
}

/// Articles ignorés pour choisir la lettre d'un jeu sans icône.
const ARTICLES: [&str; 16] = [
    "the", "a", "an", "le", "la", "les", "l", "un", "une", "des", "el", "los", "las", "il", "lo",
    "der",
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
    } else if crate::i18n::is_english() {
        format!("{:.1} h", secs as f64 / 3600.0)
    } else {
        format!("{:.1} h", secs as f64 / 3600.0).replace('.', ",")
    }
}

pub fn human_ago(ts: i64) -> String {
    let delta = (now_ts() - ts).max(0) as u64;
    let english = crate::i18n::is_english();
    match delta {
        0..=59 => crate::i18n::tr("à l'instant"),
        60..=3599 => {
            if english {
                format!("{} min ago", delta / 60)
            } else {
                format!("il y a {} min", delta / 60)
            }
        }
        3600..=86_399 => {
            if english {
                format!("{} h ago", delta / 3600)
            } else {
                format!("il y a {} h", delta / 3600)
            }
        }
        86_400..=2_591_999 => {
            let d = delta / 86_400;
            if d == 1 {
                crate::i18n::tr("hier")
            } else if english {
                format!("{d} days ago")
            } else {
                format!("il y a {d} jours")
            }
        }
        _ => {
            let m = delta / 2_592_000;
            if m == 1 {
                if english {
                    "1 month ago".to_string()
                } else {
                    "il y a 1 mois".to_string()
                }
            } else if english {
                format!("{m} months ago")
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
    /// Installations des boutiques externes, séparées des fiches Quark.
    pub provider_library: crate::providers::ProviderLibrary,
    /// Erreur de chargement : la bibliothèque reste alors en lecture seule
    /// pour ne jamais écraser un fichier illisible ou inaccessible.
    #[serde(skip)]
    pub load_error: Option<String>,
}

/// Nom de secours unique : une nouvelle corruption ne doit jamais écraser la
/// copie de la précédente.
fn unique_corrupt_path(base: &str) -> PathBuf {
    let dir = config_dir();
    let stamp = now_ts();
    for attempt in 0..100 {
        let name = if attempt == 0 {
            format!("{base}.corrompu-{stamp}")
        } else {
            format!("{base}.corrompu-{stamp}-{attempt}")
        };
        let path = dir.join(name);
        if !path.exists() {
            return path;
        }
    }
    dir.join(format!("{base}.corrompu-{stamp}-{}", std::process::id()))
}

impl Library {
    /// Jeux visibles selon les filtres de l'accueil (genre, tag, statut).
    pub fn filtered_ids(&self, sort: HomeSort) -> Vec<String> {
        let settings = &self.settings;
        self.home_ids(sort)
            .into_iter()
            .filter(|id| {
                self.get(id).is_some_and(|game| {
                    self.matches(
                        game,
                        &settings.home_genre,
                        &settings.home_tag,
                        settings.home_status,
                    )
                })
            })
            .collect()
    }

    pub fn load() -> Self {
        let path = config_path();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut library = Self::default();
                library.seed_saved_views();
                return library;
            }
            Err(error) => {
                let message = format!(
                    "Bibliothèque illisible ({}) : {error}. Aucune sauvegarde ne sera faite pour ne pas écraser le fichier.",
                    path.display()
                );
                eprintln!("[quark] {message}");
                return Self {
                    load_error: Some(message),
                    ..Self::default()
                };
            }
        };
        match serde_json::from_str::<Library>(&text) {
            Ok(mut lib) => {
                lib.migrate_defaults();
                lib
            }
            Err(error) => {
                let backup = unique_corrupt_path("config.json");
                let _ = std::fs::copy(&path, &backup);
                let message = format!(
                    "Bibliothèque illisible ({}) : {error}. Copie conservée dans {}. Aucune sauvegarde ne sera faite pour ne pas écraser le fichier.",
                    path.display(),
                    backup.display()
                );
                eprintln!("[quark] {message}");
                Self {
                    load_error: Some(message),
                    ..Self::default()
                }
            }
        }
    }

    /// Écrit la configuration de façon durable : fichier temporaire, `fsync`,
    /// puis renommage atomique. Un chargement en erreur rend la sauvegarde
    /// impossible tant que l'utilisateur n'a pas réparé le fichier.
    pub fn save(&mut self) -> Result<(), String> {
        if let Some(error) = &self.load_error {
            return Err(error.clone());
        }
        self.migrate_installations();
        self.prune_saved_views();
        std::fs::create_dir_all(config_dir()).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = config_dir().join(format!(
            "config.json.tmp-{}-{}-{}",
            std::process::id(),
            now_ts(),
            new_id()
        ));
        let result = (|| -> Result<(), String> {
            use std::io::Write as _;
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&tmp)
                .map_err(|e| e.to_string())?;
            file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, config_path()).map_err(|e| e.to_string())?;
            if let Ok(directory) = std::fs::File::open(config_dir()) {
                let _ = directory.sync_all();
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }

    /// Ancienne version : `settings.env` était appliqué à tous les lancements.
    /// Les variables sont désormais recopiées dans chaque jeu, donc on les
    /// verse une seule fois dans ceux qui n'en ont pas encore : le lancement
    /// des jeux existants ne change pas.
    pub fn migrate_defaults(&mut self) {
        self.migrate_installations();
        // Les configurations créées avant la 1.2 n'ont pas de parcours de
        // bienvenue : on ne le leur impose pas, mais elles verront le résumé
        // de la 1.2 une fois. Une configuration neuve garde le drapeau à faux
        // et affiche donc le parcours au premier lancement.
        if !self.settings.onboarding_done
            && (!self.games.is_empty() || self.settings.saved_views_seeded)
        {
            self.settings.onboarding_done = true;
        }
        self.prune_saved_views();
        self.seed_saved_views();
        // Le préfixe par défaut était appliqué à la volée à tous les jeux qui
        // n'en avaient pas : changer le réglage déplaçait donc leurs saves.
        // On le recopie une seule fois dans ces jeux ; ensuite le réglage ne
        // concerne plus que les nouveaux jeux.
        if !self.settings.prefix_defaults_migrated {
            if let Some(prefix) = self
                .settings
                .default_prefix
                .clone()
                .filter(|prefix| !prefix.trim().is_empty())
            {
                for game in &mut self.games {
                    let has_installation_prefix = game
                        .active_installation()
                        .and_then(|installation| installation.prefix.as_deref())
                        .is_some_and(|prefix| !prefix.trim().is_empty());
                    let has_game_prefix = game
                        .prefix
                        .as_deref()
                        .is_some_and(|prefix| !prefix.trim().is_empty());
                    if !has_game_prefix && !has_installation_prefix {
                        game.prefix = Some(prefix.clone());
                    }
                }
            }
            self.settings.prefix_defaults_migrated = true;
        }
        // Anciennes installations : rattache une fiche fournisseur à la racine
        // par défaut quand son chemin y est clairement contenu, pour que la
        // désinstallation puisse vérifier la racine. Indépendant de
        // `defaults_migrated`, déjà vrai dans les configurations récentes.
        let root = self.settings.install_root();
        for game in &mut self.provider_library.games {
            if game.install_root.is_none()
                && game
                    .install_path
                    .as_ref()
                    .is_some_and(|path| path.starts_with(&root))
            {
                game.install_root = Some(root.clone());
            }
        }
        if self.settings.defaults_migrated {
            return;
        }
        if !self.settings.default_env.is_empty() {
            for game in &mut self.games {
                if game.env.is_empty() {
                    game.env = self.settings.default_env.clone();
                }
            }
        }
        self.settings.defaults_migrated = true;
    }

    /// Relie les fiches de boutique aux installations de la bibliothèque
    /// unifiée. Idempotent : appelé au chargement et avant chaque sauvegarde.
    pub fn migrate_installations(&mut self) {
        for game in &mut self.games {
            game.normalize_installations();
            // Les versions précédentes ont pu enregistrer un préfixe
            // `<jeu>-epic` encore vide alors que le dossier historique existe.
            // On l'oublie pour retrouver les sauvegardes ; un préfixe déjà
            // initialisé ou choisi à la main n'est jamais touché.
            let legacy = prefix_root().join(&game.id);
            if !legacy.join("drive_c").is_dir() {
                continue;
            }
            for installation in &mut game.installations {
                let auto = prefix_root().join(format!("{}-{}", game.id, installation.slug()));
                if installation
                    .prefix
                    .as_deref()
                    .is_some_and(|prefix| PathBuf::from(prefix) == auto)
                    && !auto.join("drive_c").is_dir()
                {
                    installation.prefix = None;
                }
            }
            if let Some(active) = game.active_installation().cloned() {
                let auto = prefix_root().join(format!("{}-{}", game.id, active.slug()));
                if game
                    .prefix
                    .as_deref()
                    .is_some_and(|prefix| PathBuf::from(prefix) == auto)
                    && !auto.join("drive_c").is_dir()
                {
                    game.prefix = None;
                }
            }
        }
        let records: Vec<crate::providers::ProviderGame> = self.provider_library.games.clone();
        for record in records {
            let Some(game_id) = record.quark_game_id.clone() else {
                continue;
            };
            let Some(game) = self.get_mut(&game_id) else {
                continue;
            };
            let installed = record.install_state == crate::providers::InstallState::Installed;
            let installation = Installation {
                id: format!(
                    "{}:{}",
                    match record.key.provider {
                        crate::providers::Provider::Steam => "steam",
                        crate::providers::Provider::Epic => "epic",
                        crate::providers::Provider::Gog => "gog",
                    },
                    record.key.product_id
                ),
                provider: Some(record.key.provider),
                product_id: Some(record.key.product_id.clone()),
                label: record.title.clone(),
                path: record
                    .install_path
                    .as_ref()
                    .map(|path| path.display().to_string()),
                exe: record
                    .install_exe
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
                prefix: None,
                save_path: record
                    .custom_save_paths
                    .first()
                    .map(|path| path.display().to_string()),
                save_sync_ts: None,
                last_synced: record.last_synced,
                installed,
            };
            game.upsert_installation(installation);
            if game.exe.trim().is_empty() && !game.installations.is_empty() {
                let active = game
                    .active_installation()
                    .map(|installation| (installation.exe.clone(), installation.prefix.clone()));
                if let Some((exe, prefix)) = active {
                    game.exe = exe;
                    game.prefix = prefix;
                }
            }
        }
    }

    /// Installation correspondant à une fiche de boutique, si elle existe.
    pub fn installation_for_provider(
        &self,
        game_id: &str,
        key: &crate::providers::ProviderGameKey,
    ) -> Option<&Installation> {
        self.get(game_id)?
            .installations
            .iter()
            .find(|installation| {
                installation.provider == Some(key.provider)
                    && installation.product_id.as_deref() == Some(key.product_id.as_str())
            })
    }

    /// Sélectionne l'édition d'une boutique après adoption ou installation.
    pub fn activate_provider_installation(
        &mut self,
        game_id: &str,
        key: &crate::providers::ProviderGameKey,
    ) -> bool {
        let Some(game) = self.get_mut(game_id) else {
            return false;
        };
        let Some(id) = game
            .installations
            .iter()
            .find(|installation| {
                installation.provider == Some(key.provider)
                    && installation.product_id.as_deref() == Some(key.product_id.as_str())
            })
            .map(|installation| installation.id.clone())
        else {
            return false;
        };
        game.activate_installation(&id)
    }

    pub fn installation_for_provider_mut(
        &mut self,
        game_id: &str,
        key: &crate::providers::ProviderGameKey,
    ) -> Option<&mut Installation> {
        self.get_mut(game_id)?
            .installations
            .iter_mut()
            .find(|installation| {
                installation.provider == Some(key.provider)
                    && installation.product_id.as_deref() == Some(key.product_id.as_str())
            })
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
        self.sorted_ids_by(LibrarySort::Name)
    }

    /// Jeux triés selon la vue choisie. Contrairement à `home_ids`, cet ordre
    /// sert aussi à la liste et à la grille de la bibliothèque.
    pub fn sorted_ids_by(&self, sort: LibrarySort) -> Vec<String> {
        let mut games: Vec<&Game> = self.games.iter().collect();
        match sort {
            LibrarySort::Name => games.sort_by_key(|game| game.name.to_lowercase()),
            LibrarySort::Recent => games.sort_by(|a, b| {
                b.last_played
                    .unwrap_or(0)
                    .cmp(&a.last_played.unwrap_or(0))
                    .then_with(|| b.added_at.cmp(&a.added_at))
            }),
            LibrarySort::Added => games.sort_by(|a, b| b.added_at.cmp(&a.added_at)),
            LibrarySort::Playtime => games.sort_by(|a, b| b.playtime_secs.cmp(&a.playtime_secs)),
        }
        games.into_iter().map(|game| game.id.clone()).collect()
    }

    /// Nettoie les vues enregistrées après un chargement ou avant une
    /// sauvegarde. Les doublons de nom, les entrées vides et les filtres qui
    /// ne correspondent plus à aucun tag ou genre sont retirés : une vue ne
    /// doit jamais ouvrir une bibliothèque vide à cause d'une ancienne donnée.
    pub fn prune_saved_views(&mut self) {
        let tags: HashSet<String> = self
            .all_tags()
            .into_iter()
            .map(|(name, _)| name.to_lowercase())
            .collect();
        let genres: HashSet<String> = self
            .all_genres()
            .into_iter()
            .map(|(name, _)| name.to_lowercase())
            .collect();
        let mut seen = HashSet::new();
        let mut cleaned = Vec::new();
        for view in self.settings.saved_views.drain(..) {
            let Some(mut view) = view.normalized() else {
                continue;
            };
            if !view.tag.is_empty() && !tags.contains(&view.tag.to_lowercase()) {
                view.tag.clear();
            }
            if !view.genre.is_empty() && !genres.contains(&view.genre.to_lowercase()) {
                view.genre.clear();
            }
            if seen.insert(view.name.to_lowercase()) {
                cleaned.push(view);
            }
        }
        cleaned.truncate(32);
        self.settings.saved_views = cleaned;
    }

    /// Ajoute une fois les vues de départ (« À jouer », « En cours »,
    /// « Terminés »). Heronite fait de même avec ses presets : la fonction est
    /// visible dès l'installation, puis reste supprimable comme les autres.
    pub fn seed_saved_views(&mut self) {
        if self.settings.saved_views_seeded {
            return;
        }
        for (name, status) in [
            ("À jouer", GameStatus::Backlog),
            ("En cours", GameStatus::Playing),
            ("Terminés", GameStatus::Finished),
        ] {
            if self
                .settings
                .saved_views
                .iter()
                .any(|view| view.name.eq_ignore_ascii_case(name))
            {
                continue;
            }
            self.settings.saved_views.push(SavedView {
                name: name.to_string(),
                status,
                group_by: GroupBy::Status,
                view: ViewMode::List,
                sort: LibrarySort::Name,
                ..Default::default()
            });
        }
        self.settings.saved_views_seeded = true;
    }

    /// Répartit des identifiants déjà triés en groupes affichables.
    pub fn grouped_ids(&self, ids: &[String], group_by: GroupBy) -> Vec<GameGroup> {
        if group_by == GroupBy::None {
            return vec![GameGroup {
                key: "all".to_string(),
                label: String::new(),
                color: None,
                ids: ids.to_vec(),
                rank: 0,
            }];
        }
        let mut groups: Vec<GameGroup> = Vec::new();
        for id in ids {
            let Some(game) = self.get(id) else {
                continue;
            };
            let (key, label, color, rank) = match group_by {
                GroupBy::None => unreachable!(),
                GroupBy::Status => {
                    let rank = GameStatus::COLUMNS
                        .iter()
                        .position(|status| *status == game.status)
                        .unwrap_or(GameStatus::COLUMNS.len()) as i32;
                    (
                        format!("status-{}", game.status.label()),
                        game.status.column_label().to_string(),
                        (game.status != GameStatus::None).then(|| game.status.color()),
                        rank,
                    )
                }
                GroupBy::Store => {
                    let (label, color, rank) = store_group(game);
                    (format!("store-{label}"), label.to_string(), color, rank)
                }
                GroupBy::Genre => {
                    let label = game
                        .genres
                        .iter()
                        .map(|genre| genre.trim())
                        .find(|genre| !genre.is_empty())
                        .unwrap_or("Sans genre");
                    let rank = if label == "Sans genre" { 1 } else { 0 };
                    (format!("genre-{label}"), label.to_string(), None, rank)
                }
                GroupBy::Year => {
                    let year = game
                        .released
                        .trim()
                        .get(0..4)
                        .and_then(|text| text.parse::<i32>().ok())
                        .filter(|year| (1900..=2100).contains(year));
                    match year {
                        Some(year) => (format!("year-{year}"), year.to_string(), None, -year),
                        None => (
                            "year-unknown".to_string(),
                            "Année inconnue".to_string(),
                            None,
                            i32::MAX,
                        ),
                    }
                }
            };
            if let Some(group) = groups.iter_mut().find(|group| group.key == key) {
                group.ids.push(id.clone());
            } else {
                groups.push(GameGroup {
                    key,
                    label,
                    color,
                    ids: vec![id.clone()],
                    rank,
                });
            }
        }
        groups.sort_by(|a, b| {
            a.rank
                .cmp(&b.rank)
                .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
        });
        groups
    }

    /// Jeux du carrousel : les dernières sessions, de la plus récente à la
    /// plus ancienne. Un jeu jamais lancé n'y a pas sa place.
    pub fn recent_ids(&self, limit: usize) -> Vec<String> {
        let mut games: Vec<&Game> = self
            .games
            .iter()
            .filter(|game| game.last_played.is_some())
            .collect();
        games.sort_by(|a, b| {
            let (Some(a_time), Some(b_time)) = (a.last_played, b.last_played) else {
                return std::cmp::Ordering::Equal;
            };
            b_time
                .cmp(&a_time)
                .then_with(|| b.playtime_secs.cmp(&a.playtime_secs))
        });
        games
            .into_iter()
            .take(limit)
            .map(|g| g.id.clone())
            .collect()
    }

    /// Tags personnels de la bibliothèque, avec leur nombre de jeux.
    pub fn all_tags(&self) -> Vec<(String, usize)> {
        let mut tags: Vec<(String, usize)> = Vec::new();
        for game in &self.games {
            for tag in &game.tags {
                let tag = tag.trim();
                if tag.is_empty() {
                    continue;
                }
                match tags
                    .iter_mut()
                    .find(|(name, _)| name.eq_ignore_ascii_case(tag))
                {
                    Some((_, count)) => *count += 1,
                    None => tags.push((tag.to_string(), 1)),
                }
            }
        }
        tags.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        tags
    }

    /// Genres Steam pr\u{e9}sents dans la biblioth\u{e8}que, avec leur nombre de jeux.
    pub fn all_genres(&self) -> Vec<(String, usize)> {
        let mut genres: Vec<(String, usize)> = Vec::new();
        for game in &self.games {
            for genre in &game.genres {
                let genre = genre.trim();
                if genre.is_empty() {
                    continue;
                }
                match genres
                    .iter_mut()
                    .find(|(name, _)| name.eq_ignore_ascii_case(genre))
                {
                    Some((_, count)) => *count += 1,
                    None => genres.push((genre.to_string(), 1)),
                }
            }
        }
        genres.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        genres
    }

    /// Vrai si le jeu passe les filtres de genre, de tag et de statut.
    pub fn matches(&self, game: &Game, genre: &str, tag: &str, status: GameStatus) -> bool {
        let genre = genre.trim();
        let tag = tag.trim();
        if status != GameStatus::None && game.status != status {
            return false;
        }
        if !genre.is_empty() && !game.genres.iter().any(|g| g.eq_ignore_ascii_case(genre)) {
            return false;
        }
        if !tag.is_empty() && !game.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
            return false;
        }
        true
    }

    /// Toute la bibliothèque, triée selon le choix de la page d'accueil.
    pub fn home_ids(&self, sort: HomeSort) -> Vec<String> {
        let mut games: Vec<&Game> = self.games.iter().collect();
        match sort {
            HomeSort::Recent => games.sort_by(|a, b| {
                b.last_played
                    .unwrap_or(0)
                    .cmp(&a.last_played.unwrap_or(0))
                    .then_with(|| b.added_at.cmp(&a.added_at))
            }),
            HomeSort::Added => games.sort_by(|a, b| b.added_at.cmp(&a.added_at)),
            HomeSort::Playtime => games.sort_by(|a, b| b.playtime_secs.cmp(&a.playtime_secs)),
        }
        games.into_iter().map(|g| g.id.clone()).collect()
    }
}

/// Groupe « boutique » d'un jeu : une édition locale, une seule boutique, ou
/// « Plusieurs » quand le même jeu possède des éditions d'origines différentes.
fn store_group(game: &Game) -> (&'static str, Option<(u8, u8, u8)>, i32) {
    let mut sources = HashSet::new();
    for installation in &game.installations {
        sources.insert(installation.source_label());
    }
    let label = match sources.len() {
        0 => "Locale",
        1 => sources.into_iter().next().unwrap_or("Locale"),
        _ => "Plusieurs",
    };
    let color = match label {
        "Steam" => Some((0x1B, 0x6B, 0x93)),
        "Epic Games" => Some((0x8A, 0x8F, 0x98)),
        "GOG" => Some((0x8B, 0x5C, 0xF6)),
        "Plusieurs" => Some((0xB8, 0x8A, 0x3E)),
        _ => Some((0x6A, 0x70, 0x76)),
    };
    let rank = match label {
        "Locale" => 0,
        "Steam" => 1,
        "Epic Games" => 2,
        "GOG" => 3,
        _ => 4,
    };
    (label, color, rank)
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
    fn un_prefixe_historique_est_reutilise_pour_une_seconde_installation() {
        // Le dossier `<jeu>` existe déjà : on ne crée pas `<jeu>-epic`, sinon
        // les sauvegardes de l'ancienne installation deviennent invisibles.
        assert!(auto_installation_prefix("jeu", "epic", 2, true).is_none());
        // Une seule installation garde aussi le dossier historique.
        assert!(auto_installation_prefix("jeu", "epic", 1, false).is_none());
        let prefix = auto_installation_prefix("jeu", "epic", 2, false).unwrap();
        assert_eq!(prefix.file_name().unwrap(), "jeu-epic");
    }

    #[test]
    fn le_prefixe_de_l_installation_active_est_utilise_sans_prefixe_de_jeu() {
        let mut g = game("abc");
        g.installations.push(Installation {
            id: "epic:1".to_string(),
            prefix: Some("/tmp/quark-tests/epic".to_string()),
            ..Installation::default()
        });
        g.active_installation = Some("epic:1".to_string());
        assert_eq!(
            g.prefix_path(&Settings::default()),
            PathBuf::from("/tmp/quark-tests/epic")
        );
        // Un préfixe de jeu reste prioritaire sur celui de l'installation.
        g.prefix = Some("/tmp/quark-tests/jeu".to_string());
        assert_eq!(
            g.prefix_path(&Settings::default()),
            PathBuf::from("/tmp/quark-tests/jeu")
        );
    }

    #[test]
    fn le_prefixe_wine_suit_proton_ou_umu() {
        let mut g = game("abc");
        assert_eq!(
            g.wine_prefix_path(&Settings::default()),
            prefix_root().join("abc").join("pfx")
        );
        g.umu = true;
        assert_eq!(
            g.wine_prefix_path(&Settings::default()),
            prefix_root().join("abc")
        );
    }

    #[test]
    fn le_prefixe_par_defaut_est_recopie_une_seule_fois() {
        let mut lib = Library::default();
        lib.settings.default_prefix = Some("/tmp/quark-tests/defaut".to_string());
        lib.games.push(game("a"));
        lib.migrate_defaults();
        assert_eq!(
            lib.get("a").and_then(|game| game.prefix.as_deref()),
            Some("/tmp/quark-tests/defaut")
        );

        // Changer le défaut ne déplace plus le préfixe d'un jeu existant.
        lib.settings.default_prefix = Some("/tmp/quark-tests/nouveau".to_string());
        lib.migrate_defaults();
        assert_eq!(
            lib.get("a").and_then(|game| game.prefix.as_deref()),
            Some("/tmp/quark-tests/defaut")
        );
    }

    #[test]
    fn une_configuration_1_1_4_s_ouvre_sans_perdre_ses_jeux() {
        // Configuration minimale telle qu'écrite par la 1.1.4 : aucun champ
        // de la 1.2 (langue d'interface, onboarding, fond, version lue).
        let raw = r#"{
            "games": [{"id": "ancien", "name": "Ancien jeu"}],
            "settings": {"theme": "amoled", "language": "french", "saved_views_seeded": false}
        }"#;
        let mut lib: Library = serde_json::from_str(raw).expect("config 1.1.4 lisible");
        assert_eq!(lib.games.len(), 1);
        assert_eq!(lib.settings.ui_language, UiLanguage::French);
        assert!(!lib.settings.onboarding_done);
        assert!(lib.settings.last_seen_version.is_empty());
        assert!(!lib.settings.hero_background);
        assert!(lib.settings.hero_background_blur > 0.0);

        // Une bibliothèque existante est considérée comme déjà accueillie :
        // la mise à jour ne lui impose pas le parcours de première installation.
        lib.migrate_defaults();
        assert!(lib.settings.onboarding_done);
        assert!(lib.settings.saved_views_seeded);
    }

    #[test]
    fn une_configuration_neuve_garde_le_parcours_de_bienvenue() {
        let raw = r#"{"games": [], "settings": {"saved_views_seeded": false}}"#;
        let mut lib: Library = serde_json::from_str(raw).expect("configuration neuve lisible");
        lib.migrate_defaults();
        assert!(!lib.settings.onboarding_done);
        assert_eq!(lib.settings.ui_language, UiLanguage::French);
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
    fn le_dossier_d_installation_suit_le_reglage_ou_propose_games() {
        let mut settings = Settings::default();
        assert!(settings.install_root().ends_with("Games"));
        settings.default_install_dir = Some("/tmp/quark-tests/jeux".to_string());
        assert_eq!(
            settings.install_root(),
            PathBuf::from("/tmp/quark-tests/jeux")
        );
        settings.default_install_dir = Some("   ".to_string());
        assert!(settings.install_root().ends_with("Games"));
    }

    #[test]
    fn un_jeu_ps2_garde_son_serial_et_sa_fiche_ra() {
        let mut game = Game::new("Shadow of the Colossus", "/jeux/shadow.iso");
        assert!(game.ps2.is_none());
        game.ps2 = Some(Ps2Config {
            serial: "SCUS-97472".to_string(),
            ra_game_id: Some(238),
        });
        let text = serde_json::to_string(&game).unwrap();
        let decoded: Game = serde_json::from_str(&text).unwrap();
        assert_eq!(decoded.ps2, game.ps2);
        // Une configuration 1.2.1 ne contient pas `ps2` : le champ reste
        // optionnel et ne casse pas le chargement.
        let old: Game = serde_json::from_str(r#"{"name":"Old","exe":"/old"}"#).unwrap();
        assert!(old.ps2.is_none());
    }

    #[test]
    fn prefix_sans_reglage_est_propre_au_jeu() {
        let mut settings = Settings::default();
        settings.default_prefix = None;
        assert_eq!(
            game("abc").prefix_path(&settings),
            prefix_root().join("abc")
        );
    }

    #[test]
    fn variables_d_un_jeu_absent_se_chargent_vide() {
        // Config d'une version précédente : « env » n'existe pas encore.
        let json = r#"{ "name": "Vieux jeu", "exe": "/tmp/vieux.exe" }"#;
        let game: Game = serde_json::from_str(json).unwrap();
        assert!(game.env.is_empty());
        assert!(game.dll_source.is_none());
        let settings: Settings = serde_json::from_str(r#"{ "language": "english" }"#).unwrap();
        assert!(settings.default_env.is_empty());
    }

    #[test]
    fn un_prefixe_par_defaut_n_est_pas_relu_a_la_volee() {
        let mut settings = Settings::default();
        settings.default_prefix = Some("/tmp/quark-tests/prefixes".to_string());
        // Le défaut est recopié à l'ajout (ou par la migration) ; un jeu sans
        // préfixe garde son dossier automatique et ne suit pas les changements.
        assert_eq!(
            game("abc").prefix_path(&settings),
            prefix_root().join("abc")
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
    fn les_overrides_de_dll_existants_sont_relus() {
        let rows = parse_dll_overrides("winmm=n,b;steam_api64;dxvk=b;winmm=b");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].name, "winmm");
        // Dernière occurrence gagnante, comme dans launch_env.
        assert!(!rows[0].native);
        assert!(rows[0].builtin);
        // Entrée sans « = » : ordre par défaut native puis builtin.
        assert_eq!(rows[1].name, "steam_api64");
        assert!(rows[1].native && rows[1].builtin);
        assert_eq!(rows[2].modes(), "b");
        // Les noms avec extension restent comparables au dossier source.
        assert!(dll_name_eq("steam_api64.dll", "steam_api64"));
        assert!(!dll_name_eq("winmm", "winmmx"));
    }

    #[test]
    fn les_overrides_de_dll_remplacent_la_variable_sans_toucher_aux_autres() {
        let mut env = vec![
            EnvVar::new("DXVK_HUD", "fps"),
            EnvVar::new("WINEDLLOVERRIDES", "winmm=n"),
            EnvVar::new("winedlloverrides", "dxgi=b"),
        ];
        let rows = vec![
            DllOverride {
                name: "steam_api64".to_string(),
                native: true,
                builtin: true,
            },
            DllOverride {
                name: "dxgi".to_string(),
                native: false,
                builtin: true,
            },
        ];
        set_dll_overrides(&mut env, &rows);
        assert_eq!(env.len(), 2);
        assert_eq!(env[0].name, "DXVK_HUD");
        assert_eq!(env[1].name, DLL_OVERRIDES_VAR);
        assert_eq!(env[1].value, "steam_api64=n,b;dxgi=b");
        assert_eq!(dll_overrides_from_env(&env), rows);

        // Liste vide : la variable disparaît, pas les autres.
        set_dll_overrides(&mut env, &[]);
        assert_eq!(env.len(), 1);
        assert_eq!(env[0].name, "DXVK_HUD");
    }

    #[test]
    fn les_anciennes_variables_globales_deviennent_un_defaut_par_jeu() {
        // Config de la version précédente : "env" au niveau des réglages.
        let json = r#"{
            "settings": { "env": [{ "name": "MANGOHUD", "value": "1" }] },
            "games": [
                { "id": "a", "name": "Sans variables" },
                { "id": "b", "name": "Avec", "env": [{ "name": "DXVK_HUD", "value": "fps" }] }
            ]
        }"#;
        let mut lib: Library = serde_json::from_str(json).unwrap();
        assert_eq!(lib.settings.default_env.len(), 1);
        lib.migrate_defaults();
        // Le jeu sans variables reçoit la copie, celui qui en a garde les siennes.
        assert_eq!(lib.get("a").unwrap().env[0].name, "MANGOHUD");
        assert_eq!(lib.get("b").unwrap().env[0].name, "DXVK_HUD");
        // La migration ne s'applique qu'une fois.
        lib.settings
            .default_env
            .push(EnvVar::new("PROTON_LOG", "1"));
        lib.migrate_defaults();
        assert_eq!(lib.get("a").unwrap().env.len(), 1);
    }

    #[test]
    fn les_options_de_lancement_ont_des_valeurs_par_defaut() {
        // Config d'une version précédente : ni `launch` ni `umu`.
        let game: Game =
            serde_json::from_str(r#"{ "name": "Vieux", "exe": "/tmp/v.exe" }"#).unwrap();
        assert!(!game.launch.any());
        assert_eq!(game.launch.gamescope_size(), (1920, 1080));
        assert!(!game.umu);
        assert!(!game.proton_log);
        // Ancienne clé d'overlay : ignorée sans erreur de lecture.
        let old: Game = serde_json::from_str(r#"{ "name": "V", "steam_launch_id": 42 }"#).unwrap();
        assert_eq!(old.name, "V");
    }

    #[test]
    fn gamescope_borne_la_resolution() {
        let mut opts = LaunchOptions::default();
        opts.gamescope_width = 0;
        opts.gamescope_height = 99_999;
        assert_eq!(opts.gamescope_size(), (320, 4320));
    }

    #[test]
    fn le_journal_est_propre_a_chaque_jeu() {
        let a = session_log("jeu-a");
        let b = session_log("jeu-b");
        assert_ne!(a, b);
        assert_eq!(a.file_name().unwrap(), "session.log");
        assert!(a.starts_with(log_dir("jeu-a")));
    }

    #[test]
    fn l_accueil_trie_par_recence_d_ajout_et_d_heures() {
        let mut lib = Library::default();
        let mut a = game("a");
        a.name = "A".to_string();
        a.added_at = 300;
        a.last_played = Some(10);
        a.playtime_secs = 60;
        let mut b = game("b");
        b.name = "B".to_string();
        b.added_at = 100;
        b.last_played = Some(90);
        b.playtime_secs = 9_000;
        let mut c = game("c");
        c.name = "C".to_string();
        c.added_at = 200;
        lib.games = vec![a, b, c];

        // Carrousel : la session la plus récente d'abord, jamais joué exclu.
        assert_eq!(lib.recent_ids(5), vec!["b".to_string(), "a".to_string()]);

        assert_eq!(lib.home_ids(HomeSort::Added), vec!["a", "c", "b"]);
        assert_eq!(lib.home_ids(HomeSort::Playtime), vec!["b", "a", "c"]);
        // Jamais joué en dernier quand on trie par session.
        assert_eq!(lib.home_ids(HomeSort::Recent), vec!["b", "a", "c"]);
    }

    #[test]
    fn une_session_est_archivee_dans_l_historique() {
        let mut game = game("a");
        game.status = GameStatus::Backlog;
        game.record_session(1_000, 600);
        assert_eq!(game.playtime_secs, 600);
        assert_eq!(game.last_played, Some(1_600));
        assert_eq!(game.sessions.len(), 1);
        assert_eq!(game.sessions[0].secs, 600);
        // Un jeu « à jouer » passe « en cours » au premier lancement.
        assert_eq!(game.status, GameStatus::Playing);
        // Une durée nulle ne crée jamais une session vide.
        game.record_session(2_000, 0);
        assert_eq!(game.sessions[1].secs, 1);
    }

    #[test]
    fn un_point_de_controle_n_est_pas_compte_deux_fois() {
        let mut game = game("a");
        // Points de contrôle pendant la partie, puis inscription finale.
        game.record_session(5_000, 60);
        game.record_session(5_000, 120);
        assert_eq!(game.sessions.len(), 1);
        assert_eq!(game.playtime_secs, 120);
        game.record_session(5_000, 150);
        assert_eq!(game.sessions.len(), 1);
        assert_eq!(game.sessions[0].secs, 150);
        assert_eq!(game.playtime_secs, 150);
        assert_eq!(game.last_played, Some(5_150));
        // Une autre session s'ajoute normalement.
        game.record_session(9_000, 30);
        assert_eq!(game.sessions.len(), 2);
        assert_eq!(game.playtime_secs, 180);
    }

    #[test]
    fn mes_jeux_s_ouvre_en_grille_par_defaut() {
        // Les configurations existantes n'ont pas ce réglage : elles gardent la grille.
        assert!(!Settings::default().my_games_list);
    }

    #[test]
    fn les_filtres_de_genre_de_tag_et_de_statut_se_combinent() {
        let mut lib = Library::default();
        let mut game = game("a");
        game.genres = vec!["RPG".to_string()];
        game.tags = vec!["coop".to_string()];
        game.status = GameStatus::Playing;
        lib.games = vec![game];
        let rpg = lib.games[0].clone();
        assert!(lib.matches(&rpg, "", "", GameStatus::None));
        assert!(lib.matches(&rpg, "rpg", "coop", GameStatus::Playing));
        assert!(!lib.matches(&rpg, "Survie", "", GameStatus::None));
        assert!(!lib.matches(&rpg, "", "sol", GameStatus::None));
        assert!(!lib.matches(&rpg, "", "", GameStatus::Finished));
    }

    #[test]
    fn les_tags_et_genres_sont_repertories_avec_leur_nombre() {
        let mut lib = Library::default();
        let mut a = game("a");
        a.tags = vec!["coop".to_string(), "Détente".to_string()];
        a.genres = vec!["RPG".to_string()];
        let mut b = game("b");
        b.tags = vec!["COOP".to_string()];
        b.genres = vec!["RPG".to_string()];
        lib.games = vec![a, b];
        let tags = lib.all_tags();
        // La casse ne crée pas deux catégories.
        assert_eq!(
            tags.iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("coop"))
                .unwrap()
                .1,
            2
        );
        assert_eq!(lib.all_genres()[0], ("RPG".to_string(), 2));
    }

    #[test]
    fn un_executable_absent_est_reconnu() {
        let mut game = game("a");
        game.exe = "/tmp/quark-fichier-qui-nexiste-pas/game.exe".to_string();
        assert!(game.exe_missing());
        // Un nom de commande n'est pas un chemin : il part au PATH.
        game.exe = "steam".to_string();
        assert!(!game.exe_missing());
        game.exe = String::new();
        assert!(game.exe_missing());
    }

    #[test]
    fn prefix_du_jeu_ecrase_le_prefixe_par_defaut() {
        let mut settings = Settings::default();
        settings.default_prefix = Some("/tmp/quark-tests/defaut".to_string());
        let mut g = game("abc");
        g.prefix = Some("/tmp/quark-tests/mon-jeu".to_string());
        assert_eq!(
            g.prefix_path(&settings),
            PathBuf::from("/tmp/quark-tests/mon-jeu")
        );
    }

    #[test]
    fn bibliotheque_existante_preserve_les_jeux_et_ajoute_les_comptes_boutiques() {
        let old = r#"{
            "games": [{ "id": "quark-game", "name": "Celeste", "playtime_secs": 3600 }],
            "settings": { "language": "english", "country": "US" }
        }"#;
        let mut library: Library = serde_json::from_str(old).unwrap();
        assert_eq!(library.games[0].id, "quark-game");
        assert_eq!(library.games[0].playtime_secs, 3600);
        assert_eq!(library.settings.language, "english");
        assert!(library.provider_library.games.is_empty());

        library
            .provider_library
            .games
            .push(crate::providers::ProviderGame {
                key: crate::providers::ProviderGameKey {
                    provider: crate::providers::Provider::Epic,
                    product_id: "celeste-epic-id".to_string(),
                },
                title: "Celeste".to_string(),
                owned: true,
                install_path: Some(PathBuf::from("/games/Celeste")),
                install_root: None,
                custom_save_paths: Vec::new(),
                cover_url: None,
                install_exe: None,
                install_state: crate::providers::InstallState::Installed,
                quark_game_id: Some("quark-game".to_string()),
                capabilities: crate::providers::GameCapabilities {
                    achievements: crate::providers::CapabilityState::Supported,
                    cloud_saves: crate::providers::CapabilityState::RequiresSetup(
                        "choose a save directory".to_string(),
                    ),
                },
                last_synced: Some(1_793_110_500),
            });

        let encoded = serde_json::to_string(&library).unwrap();
        let restored: Library = serde_json::from_str(&encoded).unwrap();
        assert_eq!(restored.games[0].id, "quark-game");
        assert_eq!(restored.games[0].playtime_secs, 3600);
        let provider_game = &restored.provider_library.games[0];
        assert_eq!(provider_game.key.provider, crate::providers::Provider::Epic);
        assert_eq!(provider_game.key.product_id, "celeste-epic-id");
        assert_eq!(
            provider_game.install_state,
            crate::providers::InstallState::Installed
        );
        assert_eq!(
            provider_game.capabilities.achievements,
            crate::providers::CapabilityState::Supported
        );
        assert_eq!(
            provider_game.capabilities.cloud_saves,
            crate::providers::CapabilityState::RequiresSetup("choose a save directory".to_string())
        );
    }
    #[test]
    fn migration_cree_une_installation_locale_puis_une_edition_de_boutique() {
        let mut lib = Library::default();
        let mut game = Game::new("Celeste", "/tmp/Celeste/Celeste.exe");
        game.id = "quark-game".to_string();
        lib.games.push(game);
        lib.provider_library
            .games
            .push(crate::providers::ProviderGame {
                key: crate::providers::ProviderGameKey {
                    provider: crate::providers::Provider::Epic,
                    product_id: "celeste-epic".to_string(),
                },
                title: "Celeste".to_string(),
                owned: true,
                install_path: Some(PathBuf::from("/tmp/Celeste")),
                install_root: Some(PathBuf::from("/tmp")),
                custom_save_paths: Vec::new(),
                cover_url: None,
                install_exe: Some(PathBuf::from("/tmp/Celeste/Celeste.exe")),
                install_state: crate::providers::InstallState::Installed,
                quark_game_id: Some("quark-game".to_string()),
                capabilities: Default::default(),
                last_synced: None,
            });
        lib.migrate_installations();
        let game = lib.get("quark-game").unwrap();
        assert_eq!(game.installations.len(), 2);
        assert!(game.installations.iter().any(|installation| {
            installation.provider == Some(crate::providers::Provider::Epic)
                && installation.product_id.as_deref() == Some("celeste-epic")
        }));
        // Les données locales de l'édition (préfixe, horodatage GOG, dossier de
        // sauvegarde) ne doivent pas être écrasées par la fiche de boutique.
        if let Some(installation) = lib
            .get_mut("quark-game")
            .unwrap()
            .installations
            .iter_mut()
            .find(|installation| installation.provider.is_some())
        {
            installation.prefix = Some("/tmp/prefix-epic".to_string());
            installation.save_path = Some("/tmp/saves-epic".to_string());
            installation.save_sync_ts = Some(42);
        }
        // La migration est idempotente : sauvegarder deux fois ne duplique pas
        // les éditions et conserve les valeurs locales.
        lib.migrate_installations();
        let game = lib.get("quark-game").unwrap();
        assert_eq!(game.installations.len(), 2);
        let provider = game
            .installations
            .iter()
            .find(|installation| installation.provider.is_some())
            .unwrap();
        assert_eq!(provider.prefix.as_deref(), Some("/tmp/prefix-epic"));
        assert_eq!(provider.save_path.as_deref(), Some("/tmp/saves-epic"));
        assert_eq!(provider.save_sync_ts, Some(42));
    }

    #[test]
    fn une_bibliotheque_illisible_ne_peut_pas_etre_sauvegardee() {
        let mut library = Library::default();
        library.load_error = Some("config.json illisible".to_string());
        assert!(library.save().is_err());
    }

    #[test]
    fn les_nouveaux_reglages_sont_compatibles_avec_les_anciennes_configurations() {
        let missing: Library = serde_json::from_str(r#"{"settings":{}}"#).unwrap();
        assert_eq!(missing.settings.start_page, StartPage::Home);
        assert_eq!(missing.settings.group_by, GroupBy::Status);
        assert_eq!(missing.settings.library_sort, LibrarySort::Name);
        assert!(missing.settings.saved_views.is_empty());

        // Une valeur inconnue écrite à la main ne doit pas rendre toute la
        // bibliothèque illisible : chaque nouveau réglage retombe sur son défaut.
        let unknown: Library = serde_json::from_str(
            r#"{"settings":{"start_page":"demain","group_by":"univers","library_sort":"prix"}}"#,
        )
        .unwrap();
        assert_eq!(unknown.settings.start_page, StartPage::Home);
        assert_eq!(unknown.settings.group_by, GroupBy::Status);
        assert_eq!(unknown.settings.library_sort, LibrarySort::Name);
    }

    #[test]
    fn les_vues_de_depart_ne_sont_creees_qu_une_fois() {
        let mut lib = Library::default();
        lib.seed_saved_views();
        let names: Vec<&str> = lib
            .settings
            .saved_views
            .iter()
            .map(|view| view.name.as_str())
            .collect();
        assert_eq!(names, ["À jouer", "En cours", "Terminés"]);
        assert_eq!(lib.settings.saved_views[1].status, GameStatus::Playing);
        lib.seed_saved_views();
        assert_eq!(lib.settings.saved_views.len(), 3);

        lib.settings.saved_views.remove(0);
        lib.seed_saved_views();
        assert_eq!(lib.settings.saved_views.len(), 2);
    }

    #[test]
    fn une_vue_garde_les_filtres_le_tri_et_le_regroupement() {
        let mut settings = Settings::default();
        settings.home_status = GameStatus::Playing;
        settings.home_tag = "Coop".to_string();
        settings.home_genre = "RPG".to_string();
        settings.group_by = GroupBy::Genre;
        settings.view = ViewMode::Grid;
        settings.library_sort = LibrarySort::Recent;

        let view = SavedView::capture("  Soir  ", &settings, "  zelda ");
        assert_eq!(view.name, "Soir");
        assert_eq!(view.query, "zelda");
        assert!(view.matches(&settings, "zelda"));

        let encoded = serde_json::to_string(&view).unwrap();
        let restored: SavedView = serde_json::from_str(&encoded).unwrap();
        assert_eq!(restored, view);

        let mut target = Settings::default();
        restored.apply_to(&mut target);
        assert_eq!(target.home_status, GameStatus::Playing);
        assert_eq!(target.home_tag, "Coop");
        assert_eq!(target.home_genre, "RPG");
        assert_eq!(target.group_by, GroupBy::Genre);
        assert_eq!(target.view, ViewMode::Grid);
        assert_eq!(target.library_sort, LibrarySort::Recent);
    }

    #[test]
    fn les_vues_sont_nettoyees_sans_perdre_les_filtres_valides() {
        let mut lib = Library::default();
        let mut game = game("a");
        game.tags = vec!["Coop".to_string()];
        game.genres = vec!["RPG".to_string()];
        lib.games.push(game);
        lib.settings.saved_views = vec![
            SavedView {
                name: "   ".to_string(),
                ..Default::default()
            },
            SavedView {
                name: "Coop".to_string(),
                status: GameStatus::Playing,
                tag: "coop".to_string(),
                genre: "RPG".to_string(),
                ..Default::default()
            },
            SavedView {
                name: "coop".to_string(),
                tag: "coop".to_string(),
                ..Default::default()
            },
            SavedView {
                name: "Ancienne".to_string(),
                tag: "disparu".to_string(),
                genre: "RPG".to_string(),
                ..Default::default()
            },
        ];

        lib.prune_saved_views();
        assert_eq!(lib.settings.saved_views.len(), 2);
        assert_eq!(lib.settings.saved_views[0].name, "Coop");
        assert_eq!(lib.settings.saved_views[1].name, "Ancienne");
        // Le tag qui n'existe plus est retiré, le genre encore présent reste.
        assert!(lib.settings.saved_views[1].tag.is_empty());
        assert_eq!(lib.settings.saved_views[1].genre, "RPG");
    }

    #[test]
    fn le_regroupement_suit_l_ordre_des_statuts_et_les_annees_recentes() {
        let mut lib = Library::default();
        let mut a = game("a");
        a.name = "A".to_string();
        a.status = GameStatus::Playing;
        a.genres = vec!["RPG".to_string()];
        a.released = "2024-05-01".to_string();
        a.playtime_secs = 300;
        let mut b = game("b");
        b.name = "B".to_string();
        b.status = GameStatus::Finished;
        b.released = String::new();
        let mut c = game("c");
        c.name = "C".to_string();
        c.status = GameStatus::Playing;
        c.genres = vec!["Action".to_string()];
        c.released = "2020-01-01".to_string();
        c.playtime_secs = 900;
        let mut d = game("d");
        d.name = "D".to_string();
        d.genres = vec!["RPG".to_string()];
        d.released = "2024-02-02".to_string();
        lib.games = vec![a, b, c, d];
        let ids = lib.sorted_ids();

        let by_status = lib.grouped_ids(&ids, GroupBy::Status);
        let labels: Vec<&str> = by_status.iter().map(|group| group.label.as_str()).collect();
        assert_eq!(labels, ["En cours", "Terminé", "Sans statut"]);
        assert_eq!(by_status[0].ids, ["a", "c"]);
        assert_eq!(by_status[2].ids, ["d"]);

        let by_genre = lib.grouped_ids(&ids, GroupBy::Genre);
        let labels: Vec<&str> = by_genre.iter().map(|group| group.label.as_str()).collect();
        assert_eq!(labels, ["Action", "RPG", "Sans genre"]);
        assert_eq!(by_genre[1].ids, ["a", "d"]);

        let by_year = lib.grouped_ids(&ids, GroupBy::Year);
        let labels: Vec<&str> = by_year.iter().map(|group| group.label.as_str()).collect();
        assert_eq!(labels, ["2024", "2020", "Année inconnue"]);

        let none = lib.grouped_ids(&ids, GroupBy::None);
        assert_eq!(none.len(), 1);
        assert!(none[0].label.is_empty());
        assert_eq!(none[0].ids, ids);

        assert_eq!(
            lib.sorted_ids_by(LibrarySort::Playtime),
            ["c", "a", "b", "d"]
        );
    }

    #[test]
    fn la_recherche_de_la_bibliotheque_ignore_les_accents_sans_melanger_les_champs() {
        let mut game = game("a");
        game.name = "Paramètres avancés".to_string();
        game.developer = "Éditeur indépendant".to_string();
        game.tags = vec!["Détente".to_string()];
        assert!(game.matches_query("parametres"));
        assert!(game.matches_query("PARAMÈTRES"));
        assert!(game.matches_query("editeur"));
        assert!(game.matches_query("detente"));

        // « witcher » ne doit pas se disperser entre le titre et les autres
        // champs : c'est le même piège que dans la palette.
        game.name = "Hollow Knight".to_string();
        game.developer = "Team Cherry".to_string();
        game.tags.clear();
        assert!(!game.matches_query("witcher"));
        assert!(game.matches_query(""));
    }

    #[test]
    fn le_regroupement_par_boutique_distingue_locale_steam_et_plusieurs() {
        let mut lib = Library::default();
        let local = game("local");
        let mut steam = game("steam");
        steam.installations.push(Installation {
            provider: Some(crate::providers::Provider::Steam),
            ..Installation::default()
        });
        let mut mixed = game("mixed");
        mixed.installations.push(Installation {
            provider: Some(crate::providers::Provider::Steam),
            ..Installation::default()
        });
        mixed.installations.push(Installation {
            provider: Some(crate::providers::Provider::Gog),
            ..Installation::default()
        });
        lib.games = vec![local, steam, mixed];
        let ids = lib.sorted_ids();
        let groups = lib.grouped_ids(&ids, GroupBy::Store);
        let labels: Vec<&str> = groups.iter().map(|group| group.label.as_str()).collect();
        assert_eq!(labels, ["Locale", "Steam", "Plusieurs"]);
        assert_eq!(groups[1].ids, ["steam"]);
        assert_eq!(groups[2].ids, ["mixed"]);
    }
}
