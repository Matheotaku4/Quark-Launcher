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

    pub fn label(self) -> &'static str {
        match self {
            HomeSort::Recent => "Joués récemment",
            HomeSort::Added => "Ajoutés récemment",
            HomeSort::Playtime => "Plus joués",
        }
    }
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

    pub fn label(self) -> &'static str {
        match self {
            GameStatus::None => "Aucun",
            GameStatus::Backlog => "\u{c0} jouer",
            GameStatus::Playing => "En cours",
            GameStatus::Finished => "Termin\u{e9}",
            GameStatus::Dropped => "Abandonn\u{e9}",
        }
    }

    /// Titre de section : « Sans statut » plutôt que « Aucun ».
    pub fn column_label(self) -> &'static str {
        match self {
            GameStatus::None => "Sans statut",
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
    pub theme: ThemeMode,
    /// Clé API SteamGridDB (https://www.steamgriddb.com/profile/preferences/api).
    pub sgdb_api_key: String,
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
    /// Garde Quark dans la zone de notification : la croix ne quitte plus
    /// l'application, elle la range dans l'icône.
    pub tray_icon: bool,
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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            view: ViewMode::List,
            home_sort: HomeSort::Recent,
            theme: ThemeMode::Amoled,
            sgdb_api_key: String::new(),
            language: "french".to_string(),
            country: "FR".to_string(),
            default_proton: None,
            default_prefix: None,
            default_args: String::new(),
            default_anticheat: AnticheatMode::None,
            default_launch: LaunchOptions::default(),
            default_proton_log: false,
            default_umu: false,
            default_env: Vec::new(),
            defaults_migrated: false,
            tray_icon: true,
            carousel_auto: true,
            carousel_secs: 8.0,
            home_genre: String::new(),
            home_tag: String::new(),
            home_status: GameStatus::None,
        }
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

    /// Vrai si l'exécutable est un binaire Linux : il démarre sans Proton.
    pub fn is_native(&self) -> bool {
        !crate::proton::looks_windows(&self.exe_path())
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

    /// Ajoute une session terminée à l'historique et au temps de jeu.
    pub fn record_session(&mut self, start: i64, secs: u64) {
        let secs = secs.max(1);
        self.sessions.push(PlaySession { start, secs });
        self.playtime_secs += secs;
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
    /// Installations des boutiques externes, séparées des fiches Quark.
    pub provider_library: crate::providers::ProviderLibrary,
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
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match serde_json::from_str::<Library>(&text) {
            Ok(mut lib) => {
                lib.migrate_defaults();
                lib
            }
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

    /// Ancienne version : `settings.env` était appliqué à tous les lancements.
    /// Les variables sont désormais recopiées dans chaque jeu, donc on les
    /// verse une seule fois dans ceux qui n'en ont pas encore : le lancement
    /// des jeux existants ne change pas.
    pub fn migrate_defaults(&mut self) {
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
        let settings: Settings = serde_json::from_str(r#"{ "language": "english" }"#).unwrap();
        assert!(settings.default_env.is_empty());
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
                custom_save_paths: Vec::new(),
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
}
