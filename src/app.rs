//! État de l'application et assemblage de l'interface.

use crate::artwork::{self, AssetPlan, GameMetadata, SearchHit, SearchOutcome};
use crate::config::{
    Artwork, AssetKind, Game, Library, ProtonMode, ThemeMode, ViewMode, human_playtime,
};
use crate::disk::{self, DiskReport};
use crate::news::{self, NewsItem};
use crate::proton::{self, ProtonRuntime};
use crate::protondb::{self, Report};
use crate::session::Session;
use crate::steamlib::{self, SteamGame};
use crate::theme::{self, Palette};
use crate::tray;
use crate::update::{self, Release};
use crate::widgets::{self, Icon};
use eframe::egui::{self, Align, Align2, FontId, Layout, RichText, Sense, Stroke, Vec2};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Store,
    Library,
    MyGames,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StoreLoadState {
    Idle,
    Loading,
    Ready,
    Failed,
}

pub struct SteamSyncOutcome {
    local: Vec<SteamGame>,
    local_error: Option<String>,
    owned: Option<Result<Vec<crate::providers::ProviderGame>, String>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Error,
}

pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    /// Apparition du message (animation d'entrée).
    pub created: Instant,
    pub until: Instant,
}

pub enum SearchState {
    Idle,
    Loading,
    Done(Vec<SearchHit>, Vec<String>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AddStep {
    Name,
    /// Choix des artworks proposés pour le jeu retenu.
    Artwork,
    Scan,
    Exe,
}

/// État de la fenêtre « Ajouter un jeu » (nom -> artworks -> exécutable).
pub struct AddGame {
    pub open: bool,
    pub step: AddStep,
    pub name: String,
    pub state: SearchState,
    /// Recherche en cours : (requête envoyée, résultat).
    pub rx: Option<Receiver<(String, SearchOutcome)>>,
    pub last_typed: Option<Instant>,
    /// Dernière requête dont les résultats sont affichés.
    pub searched_for: String,
    pub hit: Option<SearchHit>,
    pub exe: String,
    pub args: String,
    pub proton: ProtonMode,
    pub custom_prefix: bool,
    pub prefix: String,
    pub error: Option<String>,
    /// Analyse de dossier en cours (ajout par lot).
    pub scan: ScanJob,
    /// Artworks proposés pour le jeu retenu (étape `AddStep::Artwork`).
    pub artwork: ArtworkPicker,
}

/// Artworks proposés à l'utilisateur pour le jeu en cours d'ajout.
#[derive(Default)]
pub struct ArtworkPicker {
    /// Recherche des propositions en cours.
    pub loading: bool,
    pub rx: Option<Receiver<(AssetPlan, Vec<String>)>>,
    pub plan: Option<AssetPlan>,
    /// Proposition retenue pour chaque type (index dans le plan).
    pub chosen: HashMap<AssetKind, usize>,
    pub error: Option<String>,
}

/// Un jeu pressenti lors de l'analyse d'un dossier.
pub struct ScanCandidate {
    pub exe: PathBuf,
    pub title: String,
    pub include: bool,
    pub hit: Option<SearchHit>,
    pub detail: String,
    pub searching: bool,
}

/// Résultat de l'analyse d'un dossier (thread → interface).
pub struct ScanOutcome {
    pub candidates: Vec<ScanCandidate>,
    pub error: Option<String>,
}

pub struct ScanJob {
    pub folder: Option<PathBuf>,
    pub running: bool,
    pub rx: Option<Receiver<ScanOutcome>>,
    pub candidates: Vec<ScanCandidate>,
    pub search_rx: Option<(usize, Receiver<SearchOutcome>)>,
    pub error: Option<String>,
}

impl Default for ScanJob {
    fn default() -> Self {
        Self {
            folder: None,
            running: false,
            rx: None,
            candidates: Vec::new(),
            search_rx: None,
            error: None,
        }
    }
}

impl AddGame {
    pub fn reset() -> Self {
        Self {
            open: true,
            step: AddStep::Name,
            name: String::new(),
            state: SearchState::Idle,
            rx: None,
            last_typed: None,
            searched_for: String::new(),
            hit: None,
            exe: String::new(),
            args: String::new(),
            proton: ProtonMode::Default,
            custom_prefix: false,
            prefix: String::new(),
            error: None,
            scan: ScanJob::default(),
            artwork: ArtworkPicker::default(),
        }
    }
}

/// État des actualités d'un jeu (mises en cache par identifiant de jeu).
pub enum NewsState {
    Loading,
    Loaded(Vec<NewsItem>),
    Failed(String),
}

/// Chargement d'actualités en cours.
pub struct NewsJob {
    pub game_id: String,
    pub rx: Receiver<Result<Vec<NewsItem>, String>>,
}

/// Note ProtonDB d'un jeu (mise en cache par appid).
pub enum ProtonDbState {
    Loading,
    Ready(Report),
    Failed(String),
}

/// Requête ProtonDB en cours.
pub struct ProtonDbJob {
    pub app_id: u32,
    pub rx: Receiver<Result<Report, String>>,
}

/// Fenêtre « Journal » : ce que la dernière session a affiché.
pub struct LogView {
    pub game_id: String,
    pub path: PathBuf,
    pub lines: Vec<String>,
    pub truncated: bool,
    pub error: Option<String>,
    pub updated: Instant,
}

/// État de la vérification des mises à jour.
#[derive(Clone)]
pub enum UpdateState {
    Idle,
    Checking,
    /// La version installée est la plus récente publiée.
    UpToDate,
    /// Une version plus récente est disponible.
    Available(Release),
    Downloading(Release),
    /// Archive téléchargée (jamais installée à notre place).
    Downloaded(PathBuf),
    Failed(String),
}

/// Réponse d'un contrôle de mise à jour ou d'un téléchargement.
pub enum UpdateMessage {
    Checked(Result<Release, String>),
    Downloaded(String, Result<PathBuf, String>),
}

/// Ce qu'un téléchargement d'artworks doit récupérer.
pub enum AssetSource {
    /// Recherche automatique des meilleurs artworks.
    Auto(SearchHit),
    /// Choix manuels de l'utilisateur, les autres propositions restant en secours.
    Chosen(AssetPlan, String),
}

/// Téléchargement d'artworks en cours.
pub struct AssetJob {
    pub rx: Receiver<(Artwork, Vec<String>)>,
    pub game_id: String,
    pub label: String,
}

/// Cible du sélecteur de fichiers natif.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PickerTarget {
    AddExe,
    AddPrefix,
    GameExe,
    GamePrefix,
    DefaultPrefix,
    ScanFolder,
    /// Image choisie sur le disque pour un artwork du jeu personnalisé.
    CustomArtwork(AssetKind),
    EpicInstall,
    SaveFolder(crate::providers::ProviderGameKey),
}

/// Sélecteur de fichiers du système, exécuté dans un thread dédié pour ne pas
/// bloquer le rendu pendant que le dialogue est ouvert.
pub struct NativePicker {
    pub rx: Receiver<Option<PathBuf>>,
    pub target: PickerTarget,
}

/// Onglet de la fenêtre Paramètres : chaque famille de réglages a sa page.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Appearance,
    /// Ce qui est prérempli dans chaque jeu ajouté (ce n'est pas global) :
    /// préfixe, arguments, runtime Proton, anti-triche, options, variables.
    Defaults,
    Artworks,
    /// Historique de jeu : heures par semaine, calendrier, séries.
    Stats,
    /// Poids des préfixes, du cache, des journaux et des jeux.
    Disk,
    About,
    Providers,
}

impl SettingsTab {
    /// Ordre de lecture : l'apparence, puis ce que reçoivent les nouveaux
    /// jeux, les boutiques (le seul onglet avec des actions), les données, et
    /// enfin la version installée.
    pub const ALL: [(SettingsTab, &'static str); 7] = [
        (SettingsTab::Appearance, "Apparence"),
        (SettingsTab::Defaults, "Nouveaux jeux"),
        (SettingsTab::Artworks, "Artworks"),
        (SettingsTab::Providers, "Boutiques & outils"),
        (SettingsTab::Stats, "Statistiques"),
        (SettingsTab::Disk, "Espace disque"),
        (SettingsTab::About, "À propos"),
    ];

    /// Phrase d'introduction de chaque page : elle dit ce qu'on vient y faire.
    pub fn hint(self) -> &'static str {
        match self {
            SettingsTab::Appearance => {
                "Thème, affichage de la bibliothèque, accueil et notifications."
            }
            SettingsTab::Defaults => {
                "Ce que reçoit chaque jeu ajouté : préfixe, runtime, options et variables. \
                 Modifiable jeu par jeu."
            }
            SettingsTab::Artworks => {
                "Sources des images, téléchargement et remplacement des artworks."
            }
            SettingsTab::Providers => {
                "Synchronisation, comptes Steam / Epic / GOG et outils gérés par Quark."
            }
            SettingsTab::Stats => "Historique de jeu : heures, calendrier et séries.",
            SettingsTab::Disk => "Poids des préfixes, du cache, des journaux et des jeux.",
            SettingsTab::About => "Version installée, mises à jour et licences.",
        }
    }
}

/// Fenêtre « Personnaliser » d'un jeu (images et apparence).
pub struct Customize {
    pub game_id: String,
    /// Type dont les propositions sont affichées.
    pub kind: Option<AssetKind>,
    pub loading: bool,
    pub rx: Option<Receiver<AssetPlan>>,
    pub plan: Option<AssetPlan>,
    pub error: Option<String>,
}

impl Customize {
    pub fn for_game(game_id: &str) -> Self {
        Self {
            game_id: game_id.to_string(),
            kind: None,
            loading: false,
            rx: None,
            plan: None,
            error: None,
        }
    }
}

/// Un jeu Steam installé proposé à l'import.
pub struct SteamCandidate {
    pub app_id: u32,
    pub name: String,
    pub dir: PathBuf,
    pub exe: Option<PathBuf>,
    pub include: bool,
    /// Déjà dans la bibliothèque (même AppID) : coché, il ne sera pas dupliqué.
    pub owned: bool,
}

/// Fenêtre « Importer depuis Steam ».
pub struct SteamImport {
    pub loading: bool,
    pub rx: Option<Receiver<Result<Vec<SteamGame>, String>>>,
    pub games: Vec<SteamCandidate>,
    pub error: Option<String>,
    pub filter: String,
}

impl SteamImport {
    pub fn new() -> Self {
        Self {
            loading: true,
            rx: None,
            games: Vec::new(),
            error: None,
            filter: String::new(),
        }
    }
}

/// Connexion d'une boutique depuis le launcher : le code d'autorisation est
/// copié depuis le navigateur, puis échangé par l'outil officiel.
pub struct ProviderLogin {
    pub provider: crate::providers::Provider,
    pub code: String,
    pub status: Option<String>,
    pub error: Option<String>,
    pub rx: Option<Receiver<Result<String, String>>>,
}

/// Lecture de la fiche Steam (développeur, genres, date, description).
pub struct MetadataJob {
    pub game_id: String,
    pub rx: Receiver<Result<GameMetadata, String>>,
}

pub struct SaveJob {
    pub key: crate::providers::ProviderGameKey,
    pub title: String,
    pub rx: Receiver<Result<SaveTaskOutput, String>>,
}

pub enum SaveTaskOutput {
    BackupPreview(crate::saves::BackupPreview),
    Backups(Vec<crate::saves::BackupInfo>),
    RestorePreview(crate::saves::BackupInfo, crate::saves::BackupPreview),
}

pub struct SaveBackgroundJob {
    pub title: String,
    pub phase: crate::saves::BackupPhase,
    pub rx: Receiver<Result<crate::saves::BackupRecord, String>>,
}

pub struct QuarkApp {
    pub lib: Library,
    pub pal: Palette,
    pub tab: Tab,
    pub selected: Option<String>,
    pub filter: String,
    pub store_query: String,
    pub store_snapshot: Option<crate::steam_store::StoreSnapshot>,
    pub store_stale: bool,
    pub store_loading: bool,
    pub store_error: Option<String>,
    pub store_search_results: Option<Vec<crate::steam_store::StoreItem>>,
    pub store_search_loading: bool,
    pub store_search_error: Option<String>,
    pub store_featured_rx: Option<Receiver<Result<crate::steam_store::StoreFetch, String>>>,
    pub store_search_rx: Option<Receiver<Result<Vec<crate::steam_store::StoreItem>, String>>>,
    pub store_offline_mode: bool,
    /// Recommandations recalculées depuis le catalogue et la meilleure
    /// bibliothèque disponible (Steam Web API si configurée, sinon Quark).
    pub store_recommendations: Vec<crate::steam_store::StoreItem>,
    pub store_recommendation_source: Option<String>,
    pub store_steam_library: Option<Vec<crate::steam_store::OwnedSteamGame>>,
    /// Note non bloquante (classement indisponible, profil Steam privé…).
    pub store_notice: Option<String>,
    store_state: StoreLoadState,
    pub add: AddGame,
    pub settings_open: bool,
    pub settings_tab: SettingsTab,
    pub editing: Option<Game>,
    /// Fenêtre « Personnaliser » du jeu en cours (artworks).
    pub customize: Option<Customize>,
    pub picker: Option<NativePicker>,
    /// Jeux dont le retrait attend confirmation (vide = aucune demande).
    pub confirm_remove: Vec<String>,
    /// Jeux marqués par Ctrl+clic : ils se retirent en lot.
    pub marked: Vec<String>,
    pub runtimes: Vec<ProtonRuntime>,
    /// Outils externes trouvés sur le PATH (gamescope, gamemoderun, umu-run).
    pub tools: proton::Tools,
    pub running: HashMap<String, Session>,
    pub job: Option<AssetJob>,
    pub toasts: Vec<Toast>,
    pub dirty: bool,
    pub last_change: Instant,
    pub applied_theme: Option<ThemeMode>,
    /// Panneau d'informations du jeu (masqué par défaut, bouton ⓘ).
    pub show_info: bool,
    /// Artworks à télécharger après le job en cours.
    pub job_queue: VecDeque<(String, AssetSource)>,
    /// Textures des artworks (décodées par Quark lui-même).
    pub textures: crate::images::TextureCache,
    /// Couleur d'accent de chaque jeu, déduite de sa couverture (une seule
    /// analyse par image).
    pub accents: HashMap<PathBuf, egui::Color32>,
    /// Images claires (bannière bleu ciel…) : une seule analyse par fichier,
    /// pour y poser des icônes sombres.
    pub light_images: HashMap<PathBuf, bool>,
    /// Actualités par jeu (chargées une fois, mises en cache).
    pub news: HashMap<String, NewsState>,
    /// Chargement d'actualités en cours.
    pub news_job: Option<NewsJob>,
    /// État de la vérification des mises à jour.
    pub update: UpdateState,
    pub update_rx: Option<Receiver<UpdateMessage>>,
    /// Notes ProtonDB, une recherche par appid.
    pub protondb: HashMap<u32, ProtonDbState>,
    pub protondb_job: Option<ProtonDbJob>,
    /// Journal de session ouvert (bouton « Journal »).
    pub logs: Option<LogView>,
    pub save_job: Option<SaveJob>,
    pub save_preview: Option<(
        crate::providers::ProviderGameKey,
        String,
        crate::saves::BackupPreview,
    )>,
    pub save_confirm: Option<(crate::providers::ProviderGameKey, String)>,
    pub save_confirm_rx: Option<Receiver<Result<crate::saves::BackupRecord, String>>>,
    pub save_inventory: Option<(
        crate::providers::ProviderGameKey,
        String,
        Vec<crate::saves::BackupInfo>,
    )>,
    pub save_restore_preview: Option<(
        crate::providers::ProviderGameKey,
        String,
        crate::saves::BackupInfo,
        crate::saves::BackupPreview,
    )>,
    pub save_restore_confirm: Option<(
        crate::providers::ProviderGameKey,
        String,
        String,
        Vec<String>,
    )>,
    pub save_restore_rx: Option<Receiver<Result<(), String>>>,
    pub save_background_jobs: Vec<SaveBackgroundJob>,
    /// Pourquoi l'icône de notification n'a pas pu démarrer (dit dans les
    /// paramètres, jamais avalé).
    pub tray_error: Option<String>,
    /// Page d'accueil (carrousel des jeux récents) au lieu de la fiche du jeu.
    pub home: bool,
    /// Vignette affichée par le carrousel de l'accueil.
    pub carousel: usize,
    /// Début du glissement du carrousel (animation).
    pub carousel_at: Instant,
    /// Instant où la page d'accueil a été ouverte (animation d'entrée).
    pub home_at: Instant,
    /// Début de l'animation d'entrée de la page visible (changement d'onglet
    /// ou de vue) : les grilles s'y accrochent pour apparaître en cascade.
    pub page_at: Instant,
    /// Dernier onglet et dernière vue rendus, pour détecter un changement.
    last_tab: Tab,
    last_view: ViewMode,
    /// Bannières floutées (fond de la fiche jeu), une par fichier.
    pub blurs: crate::images::BlurCache,
    /// Fiche du jeu affichée en surimpression (vue grille).
    pub overlay: Option<String>,
    /// Import de la bibliothèque Steam installée.
    pub steam_import: Option<SteamImport>,
    /// Identifiants de boutiques lus depuis le fichier local privé.
    pub credentials: crate::providers::credentials::Credentials,
    /// Erreur de chargement des identifiants, sans contenu sensible.
    pub credentials_error: Option<String>,
    /// Champ temporaire Steam : jamais écrit dans la bibliothèque Quark.
    pub steam_key_draft: String,
    /// État d'installation/vérification des outils gérés, par outil.
    pub managed_tools: [Option<String>; 3],
    pub tool_jobs: [Option<Receiver<Result<String, String>>>; 3],
    /// Connexion d'une boutique depuis le launcher (code copié du navigateur).
    pub provider_login: Option<ProviderLogin>,
    pub steam_sync_loading: bool,
    pub steam_sync_error: Option<String>,
    pub steam_sync_rx: Option<Receiver<SteamSyncOutcome>>,
    pub epic_sync_loading: bool,
    pub epic_sync_error: Option<String>,
    pub epic_sync_rx: Option<Receiver<Result<Vec<crate::providers::ProviderGame>, String>>>,
    pub epic_install_sync_rx: Option<
        Receiver<
            Result<
                (
                    Vec<crate::providers::ProviderGame>,
                    Vec<crate::providers::ProviderGame>,
                ),
                String,
            >,
        >,
    >,
    /// Synchronisation GOG : l'API GOG est interrogée avec la session gogdl.
    pub gog_sync_loading: bool,
    pub gog_sync_error: Option<String>,
    pub gog_sync_rx: Option<Receiver<Result<Vec<crate::providers::ProviderGame>, String>>>,
    /// Onglet de boutique actif dans « Mes jeux » (None = toutes).
    pub my_games_store: Option<crate::providers::Provider>,
    /// Synchronisation automatique lancée une seule fois, après l'affichage.
    pub startup_sync_done: bool,
    pub frames_alive: u32,
    pub epic_owned_fallback: Option<Vec<crate::providers::ProviderGame>>,
    pub epic_install_pending: Option<crate::providers::ProviderGameKey>,
    pub epic_installing: Option<crate::providers::ProviderGameKey>,
    pub epic_install_rx: Option<
        Receiver<(
            crate::providers::ProviderGameKey,
            Result<crate::providers::ProviderGame, String>,
        )>,
    >,
    pub steam_id_draft: String,
    /// Métadonnées Steam à charger, une requête à la fois.
    pub meta_job: Option<MetadataJob>,
    pub meta_queue: VecDeque<(String, u32)>,
    /// Mesure de l'espace disque en cours.
    pub disk_job: Option<Receiver<DiskReport>>,
    pub disk_report: Option<DiskReport>,
    /// Jeux dont le lancement a échoué faute d'exécutable : surlignés en rouge
    /// quelques secondes, même si le chemin n'est plus affiché.
    pub missing_flash: HashMap<String, Instant>,
    /// Ctrl+Q : on quitte vraiment, sans relancer l'icône au passage.
    pub quitting: bool,
}

impl QuarkApp {
    /// Construit l'application. Seul le contexte egui est nécessaire : il vient
    /// de la fenêtre réelle ou du rendu hors écran (`quark --ui`).
    pub fn new(ctx: &egui::Context) -> Self {
        let lib = Library::load();
        let store_snapshot = crate::steam_store::load_cache(&crate::steam_store::cache_path())
            .ok()
            .flatten();
        let store_stale = store_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.is_stale(crate::config::now_ts()));
        let (credentials, credentials_error) =
            match crate::providers::credentials::load(&crate::config::credentials_path()) {
                Ok(credentials) => (credentials, None),
                Err(error) => (
                    crate::providers::credentials::Credentials::default(),
                    Some(error),
                ),
            };
        let runtimes = proton::detect_runtimes();
        let selected = lib.games.first().map(|g| g.id.clone());
        theme::apply(ctx, lib.settings.theme);
        let pal = theme::palette(lib.settings.theme);
        let (store_recommendations, store_recommendation_source) =
            crate::steam_store::recommendations_for(store_snapshot.as_ref(), &lib.games, None);
        Self {
            applied_theme: Some(lib.settings.theme),
            selected,
            runtimes,
            tools: proton::Tools::detect(),
            lib,
            pal,
            tab: Tab::Library,
            filter: String::new(),
            store_query: String::new(),
            store_snapshot,
            store_stale,
            store_loading: false,
            store_error: None,
            store_search_results: None,
            store_search_loading: false,
            store_search_error: None,
            store_featured_rx: None,
            store_search_rx: None,
            store_offline_mode: false,
            store_recommendations,
            store_recommendation_source,
            store_steam_library: None,
            store_notice: None,
            store_state: StoreLoadState::Idle,
            add: AddGame {
                open: false,
                ..AddGame::reset()
            },
            settings_open: false,
            settings_tab: SettingsTab::Appearance,
            editing: None,
            customize: None,
            picker: None,
            confirm_remove: Vec::new(),
            marked: Vec::new(),
            running: HashMap::new(),
            job: None,
            toasts: Vec::new(),
            dirty: false,
            last_change: Instant::now(),
            show_info: false,
            job_queue: VecDeque::new(),
            textures: crate::images::TextureCache::default(),
            accents: HashMap::new(),
            light_images: HashMap::new(),
            news: HashMap::new(),
            news_job: None,
            update: UpdateState::Idle,
            update_rx: None,
            protondb: HashMap::new(),
            protondb_job: None,
            logs: None,
            save_job: None,
            save_preview: None,
            save_confirm: None,
            save_confirm_rx: None,
            save_inventory: None,
            save_restore_preview: None,
            save_restore_confirm: None,
            save_restore_rx: None,
            save_background_jobs: Vec::new(),
            tray_error: None,
            home: false,
            carousel: 0,
            carousel_at: Instant::now(),
            home_at: Instant::now(),
            page_at: Instant::now(),
            last_tab: Tab::Library,
            last_view: ViewMode::List,
            blurs: crate::images::BlurCache::default(),
            overlay: None,
            steam_import: None,
            credentials,
            credentials_error,
            steam_key_draft: String::new(),
            steam_id_draft: String::new(),
            steam_sync_loading: false,
            steam_sync_error: None,
            steam_sync_rx: None,
            epic_sync_loading: false,
            epic_sync_error: None,
            epic_sync_rx: None,
            epic_install_sync_rx: None,
            gog_sync_loading: false,
            gog_sync_error: None,
            gog_sync_rx: None,
            my_games_store: None,
            startup_sync_done: false,
            frames_alive: 0,
            epic_owned_fallback: None,
            epic_install_pending: None,
            epic_installing: None,
            epic_install_rx: None,
            managed_tools: std::array::from_fn(|_| None),
            provider_login: None,
            tool_jobs: std::array::from_fn(|_| None),
            meta_job: None,
            meta_queue: VecDeque::new(),
            disk_job: None,
            disk_report: None,
            missing_flash: HashMap::new(),
            quitting: false,
        }
    }

    pub fn touch(&mut self) {
        self.dirty = true;
        self.last_change = Instant::now();
    }

    pub fn toast_info(&mut self, text: impl Into<String>) {
        let now = Instant::now();
        self.toasts.push(Toast {
            text: text.into(),
            kind: ToastKind::Info,
            created: now,
            until: now + Duration::from_secs(6),
        });
    }

    pub fn toast_error(&mut self, text: impl Into<String>) {
        let now = Instant::now();
        self.toasts.push(Toast {
            text: text.into(),
            kind: ToastKind::Error,
            created: now,
            until: now + Duration::from_secs(10),
        });
    }

    /// Ouvre l'ajout de jeu en préremplissant les réglages « par défaut ».
    pub fn open_add_dialog(&mut self) {
        self.add = AddGame::reset();
        let settings = &self.lib.settings;
        self.add.args = settings.default_args.clone();
        self.add.proton = settings
            .default_proton
            .clone()
            .map(ProtonMode::Runtime)
            .unwrap_or_default();
        self.add.custom_prefix = settings.default_prefix.is_some();
        self.add.prefix = settings.default_prefix.clone().unwrap_or_default();
    }

    /// Vrai si l'image est claire, calculé une seule fois par fichier.
    pub fn is_light_image(&mut self, path: &std::path::Path) -> bool {
        if let Some(light) = self.light_images.get(path) {
            return *light;
        }
        let light = crate::images::is_light(path);
        self.light_images.insert(path.to_path_buf(), light);
        light
    }

    // -----------------------------------------------------------------------
    // Boucle
    // -----------------------------------------------------------------------

    fn tick(&mut self, ctx: &egui::Context) {
        self.poll_running_games();
        self.poll_update(ctx);
        self.poll_artwork_plan(ctx);
        self.poll_asset_job(ctx);
        self.poll_news();
        self.poll_picker(ctx);
        self.poll_customize_plan(ctx);
        self.poll_search();
        self.poll_scan();
        self.auto_search(ctx);
        self.ensure_news(ctx);
        self.ensure_protondb(ctx);
        self.poll_protondb();
        self.poll_metadata(ctx);
        self.poll_disk(ctx);
        self.poll_steam_import(ctx);
        self.poll_managed_tools(ctx);
        self.poll_provider_login(ctx);
        self.poll_steam_sync(ctx);
        self.poll_epic_sync(ctx);
        self.poll_gog_sync(ctx);
        self.poll_epic_install(ctx);
        self.poll_save_jobs(ctx);
        self.poll_save_background_jobs(ctx);
        self.poll_store(ctx);
        if self.tab == Tab::Store && !self.store_offline_mode {
            self.ensure_store(ctx);
        }

        if self.epic_install_rx.is_some() {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
        if self.add.scan.running || self.add.scan.search_rx.is_some() {
            ctx.request_repaint_after(Duration::from_millis(120));
        }

        // Recherche en cours : entretient l'animation du spinner.
        if matches!(self.add.state, SearchState::Loading) {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        if self.dirty && self.last_change.elapsed() > Duration::from_millis(800) {
            match self.lib.save() {
                Ok(()) => self.dirty = false,
                Err(err) => {
                    self.dirty = false;
                    self.toast_error(format!("Sauvegarde impossible : {err}"));
                }
            }
            self.last_change = Instant::now();
        }

        let now = Instant::now();
        self.toasts.retain(|t| t.until > now);

        if ctx.input(|i| i.viewport().close_requested()) {
            let _ = self.lib.save();
            // Icône de notification : la croix ferme vraiment Quark, donc la
            // fenêtre disparaît aussi de la barre des tâches, et l'icône reste,
            // tenue par le processus dédié (`quark --tray`), prête à relancer le
            // launcher. Cacher la fenêtre n'était pas une option : `set_visible`
            // n'existe pas sur Wayland, où la fenêtre restait ouverte.
            // Ctrl+Q, lui, arrête tout (voir `quit`).
            if self.lib.settings.tray_icon && !self.quitting {
                if let Err(err) = tray::ensure_helper() {
                    eprintln!("[quark] {err}");
                }
            }
        }

        // Journal ouvert sur un jeu en cours : il se rafraîchit tout seul.
        let refresh_due = self.logs.as_ref().is_some_and(|logs| {
            self.running.contains_key(&logs.game_id)
                && logs.updated.elapsed() >= Duration::from_millis(800)
        });
        if refresh_due {
            self.refresh_log();
        }
        if self
            .logs
            .as_ref()
            .is_some_and(|logs| self.running.contains_key(&logs.game_id))
        {
            ctx.request_repaint_after(Duration::from_millis(800));
        }

        if self.job.is_some()
            || self.news_job.is_some()
            || self.protondb_job.is_some()
            || !self.running.is_empty()
            || self.logs.is_some()
        {
            ctx.request_repaint_after(Duration::from_millis(400));
        }
    }

    fn poll_running_games(&mut self) {
        let mut finished: Vec<(String, u64, i64, Option<crate::saves::SaveIdentity>)> = Vec::new();
        for (id, game) in self.running.iter_mut() {
            match game.child.try_wait() {
                Ok(None) => {
                    game.session = game.started.elapsed().as_secs();
                    // Arrêt demandé mais le jeu s'accroche : on tue le groupe.
                    if game.should_escalate() {
                        crate::session::force_kill(game);
                        game.escalated();
                    }
                }
                Ok(Some(_)) => finished.push((
                    id.clone(),
                    game.started.elapsed().as_secs(),
                    game.started_ts,
                    game.save_identity.clone(),
                )),
                Err(_) => finished.push((
                    id.clone(),
                    game.started.elapsed().as_secs(),
                    game.started_ts,
                    game.save_identity.clone(),
                )),
            }
        }
        for (id, secs, started_ts, save_identity) in finished {
            self.running.remove(&id);
            if let Some(identity) = save_identity {
                self.start_background_save(identity, crate::saves::BackupPhase::AfterExit);
            }
            if let Some(game) = self.lib.get_mut(&id) {
                let name = game.name.clone();
                game.record_session(started_ts, secs);
                self.toast_info(format!(
                    "{name} : session de {}",
                    human_playtime(secs.max(1))
                ));
            }
            self.touch();
        }
    }

    fn poll_asset_job(&mut self, ctx: &egui::Context) {
        let received = match self.job.as_ref() {
            Some(job) => Some((job.game_id.clone(), job.label.clone(), job.rx.try_recv())),
            None => None,
        };
        let Some((id, label, result)) = received else {
            return;
        };
        let (artwork, warnings) = match result {
            Ok(value) => value,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => (
                Artwork::default(),
                vec!["téléchargement interrompu".to_string()],
            ),
        };
        self.job = None;
        if let Some(game) = self.lib.get_mut(&id) {
            for kind in AssetKind::ALL {
                if let Some(path) = artwork.get(kind) {
                    game.artwork.set(kind, path.clone());
                }
            }
        }
        self.touch();
        let got_any = AssetKind::ALL
            .iter()
            .any(|kind| artwork.get(*kind).is_some());
        if !got_any {
            self.toast_error(format!("Artworks introuvables pour {label}"));
        } else if warnings.is_empty() {
            self.toast_info(format!("Artworks de {label} importés"));
        } else {
            self.toast_error(format!("{label} : {}", warnings.join(" · ")));
        }
        self.start_next_job(ctx);
    }

    fn start_next_job(&mut self, ctx: &egui::Context) {
        if self.job.is_some() {
            return;
        }
        if let Some((id, source)) = self.job_queue.pop_front() {
            self.queue_asset_job(ctx, &id, source);
        }
    }

    fn poll_search(&mut self) {
        let received = self.add.rx.as_ref().map(|rx| rx.try_recv());
        match received {
            Some(Ok((query, outcome))) => {
                self.add.rx = None;
                if self.add.name.trim() == query {
                    self.add.searched_for = query;
                    self.add.state = SearchState::Done(outcome.hits, outcome.warnings);
                } else if self.add.name.trim().len() >= 2 {
                    // Réponse obsolète : la nouvelle requête part au même tick.
                    self.add.state = SearchState::Loading;
                } else {
                    self.add.state = SearchState::Idle;
                }
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.add.rx = None;
                self.add.state =
                    SearchState::Done(Vec::new(), vec!["Recherche interrompue".to_string()]);
            }
            _ => {}
        }
    }

    fn auto_search(&mut self, ctx: &egui::Context) {
        if !self.add.open || self.add.step != AddStep::Name || self.add.rx.is_some() {
            return;
        }
        let query = self.add.name.trim().to_string();
        if query.len() < 2 {
            // Champ vidé : plus rien à afficher.
            if matches!(self.add.state, SearchState::Done(..)) {
                self.add.state = SearchState::Idle;
            }
            return;
        }
        if self.add.searched_for == query {
            return;
        }
        // Anti-rebond : egui ne redessine que sur événement, il faut donc
        // demander explicitement une frame quand le délai sera écoulé.
        if let Some(typed) = self.add.last_typed {
            if let Some(remaining) = Duration::from_millis(500).checked_sub(typed.elapsed()) {
                ctx.request_repaint_after(remaining);
                return;
            }
        }
        self.start_search(ctx);
    }

    pub fn start_search(&mut self, ctx: &egui::Context) {
        let query = self.add.name.trim().to_string();
        if query.len() < 2 {
            return;
        }
        let (tx, rx) = channel();
        let settings = self.lib.settings.clone();
        let repaint = ctx.clone();
        let request = query.clone();
        let sent = query.clone();
        std::thread::spawn(move || {
            let client = artwork::http();
            let outcome = artwork::search(&client, &request, &settings);
            let _ = tx.send((sent, outcome));
            repaint.request_repaint();
        });
        self.add.rx = Some(rx);
        self.add.state = SearchState::Loading;
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    // -----------------------------------------------------------------------
    // Sélecteur de fichiers natif (XDG Desktop Portal, GTK/KDE du bureau)
    // -----------------------------------------------------------------------

    /// Ouvre le sélecteur de fichiers du système. Non bloquant : le résultat
    /// est récupéré au tick suivant (`poll_picker`).
    pub fn pick_file(&mut self, ctx: &egui::Context, start: PathBuf, target: PickerTarget) {
        self.spawn_picker(ctx, start, target, false);
    }

    /// Ouvre le sélecteur de dossiers du système.
    pub fn pick_directory(&mut self, ctx: &egui::Context, start: PathBuf, target: PickerTarget) {
        self.spawn_picker(ctx, start, target, true);
    }

    fn spawn_picker(
        &mut self,
        ctx: &egui::Context,
        start: PathBuf,
        target: PickerTarget,
        directory: bool,
    ) {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        let thread_target = target.clone();
        let start = if start.is_dir() {
            start
        } else {
            crate::config::home_dir()
        };
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new()
                .set_title(if directory {
                    "Choisir un dossier"
                } else {
                    "Choisir un exécutable"
                })
                .set_directory(&start);
            if matches!(&thread_target, PickerTarget::CustomArtwork(_)) {
                dialog = dialog.add_filter(
                    "Images",
                    &["png", "PNG", "jpg", "JPG", "jpeg", "JPEG", "webp", "WEBP"],
                );
            } else if !directory {
                dialog = dialog
                    .add_filter(
                        "Programmes",
                        &[
                            "exe", "EXE", "sh", "bat", "run", "desktop", "AppImage", "appimage",
                        ],
                    )
                    .add_filter("Tous les fichiers", &["*"]);
            }
            let picked = if directory {
                dialog.pick_folder()
            } else {
                dialog.pick_file()
            };
            let _ = tx.send(picked);
            ctx.request_repaint();
        });
        self.picker = Some(NativePicker { rx, target });
    }

    fn poll_picker(&mut self, ctx: &egui::Context) {
        let received = self
            .picker
            .as_ref()
            .map(|picker| (picker.target.clone(), picker.rx.try_recv()));
        let Some((target, result)) = received else {
            return;
        };
        match result {
            Ok(Some(path)) => {
                self.picker = None;
                self.apply_pick(ctx, target, path);
            }
            Ok(None) => self.picker = None,
            Err(TryRecvError::Empty) => {
                // Le dialogue natif vit dans son thread : on garde l'UI vivante.
                ctx.request_repaint_after(Duration::from_millis(80));
            }
            Err(TryRecvError::Disconnected) => {
                self.picker = None;
                self.toast_error(
                    "Sélecteur de fichiers indisponible (xdg-desktop-portal ou zenity manquant)",
                );
            }
        }
    }

    fn apply_pick(&mut self, ctx: &egui::Context, target: PickerTarget, path: PathBuf) {
        let text = path.display().to_string();
        match target {
            PickerTarget::ScanFolder => self.start_scan(ctx, path),
            PickerTarget::AddExe => self.add.exe = text,
            PickerTarget::AddPrefix => {
                self.add.prefix = text;
                self.add.custom_prefix = true;
            }
            PickerTarget::GameExe => {
                if let Some(game) = self.editing.as_mut() {
                    game.exe = text;
                }
            }
            PickerTarget::GamePrefix => {
                if let Some(game) = self.editing.as_mut() {
                    game.prefix = Some(text);
                }
            }
            PickerTarget::DefaultPrefix => {
                self.lib.settings.default_prefix = Some(text);
                self.touch();
            }
            PickerTarget::CustomArtwork(kind) => self.import_artwork(kind, path),
            PickerTarget::EpicInstall => self.start_epic_install(ctx, path),
            PickerTarget::SaveFolder(key) => {
                if let Some(game) = self
                    .lib
                    .provider_library
                    .games
                    .iter_mut()
                    .find(|game| game.key == key)
                {
                    if !game.custom_save_paths.contains(&path) {
                        game.custom_save_paths.push(path);
                        self.touch();
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Actualités (flux Steam du jeu, sous la bannière)
    // -----------------------------------------------------------------------

    /// Charge les actualités du jeu sélectionné, une seule fois par jeu.
    fn ensure_news(&mut self, ctx: &egui::Context) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        if self.news.contains_key(&id) {
            return;
        }
        if self.news_job.as_ref().is_some_and(|job| job.game_id == id) {
            return;
        }
        let Some(app_id) = self.lib.get(&id).and_then(|game| game.steam_app_id) else {
            return;
        };
        self.start_news(ctx, &id, app_id);
    }

    pub fn start_news(&mut self, ctx: &egui::Context, game_id: &str, app_id: u32) {
        // Un seul jeu chargé à la fois : le précédent perd sa place (mais garde
        // ce qu'il avait déjà obtenu).
        if let Some(previous) = self.news_job.take() {
            if previous.game_id != game_id
                && matches!(self.news.get(&previous.game_id), Some(NewsState::Loading))
            {
                self.news.remove(&previous.game_id);
            }
        }
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let client = artwork::http();
            let result = news::fetch(&client, app_id, news::COUNT);
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.news.insert(game_id.to_string(), NewsState::Loading);
        self.news_job = Some(NewsJob {
            game_id: game_id.to_string(),
            rx,
        });
        ctx.request_repaint_after(Duration::from_millis(200));
    }

    fn poll_news(&mut self) {
        let received = self
            .news_job
            .as_ref()
            .map(|job| (job.game_id.clone(), job.rx.try_recv()));
        let Some((id, result)) = received else {
            return;
        };
        match result {
            Ok(Ok(items)) => {
                self.news_job = None;
                self.news.insert(id, NewsState::Loaded(items));
            }
            Ok(Err(message)) => {
                self.news_job = None;
                self.news.insert(id, NewsState::Failed(message));
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.news_job = None;
                self.news
                    .insert(id, NewsState::Failed("flux interrompu".to_string()));
            }
        }
    }

    /// Relance le chargement des actualités du jeu sélectionné.
    pub fn reload_news(&mut self, ctx: &egui::Context) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let Some(app_id) = self.lib.get(&id).and_then(|game| game.steam_app_id) else {
            return;
        };
        self.news.remove(&id);
        self.start_news(ctx, &id, app_id);
    }

    pub fn start_asset_job(&mut self, ctx: &egui::Context, game_id: &str, hit: SearchHit) {
        self.queue_asset_job(ctx, game_id, AssetSource::Auto(hit));
    }

    /// Télécharge les artworks choisis à la main dans la fenêtre d'ajout.
    pub fn start_chosen_job(
        &mut self,
        ctx: &egui::Context,
        game_id: &str,
        label: String,
        plan: AssetPlan,
    ) {
        self.queue_asset_job(ctx, game_id, AssetSource::Chosen(plan, label));
    }

    fn queue_asset_job(&mut self, ctx: &egui::Context, game_id: &str, source: AssetSource) {
        // Un seul téléchargement à la fois : les suivants attendent leur tour.
        if self.job.is_some() {
            self.job_queue.push_back((game_id.to_string(), source));
            return;
        }
        let (tx, rx) = channel();
        let settings = self.lib.settings.clone();
        let id = game_id.to_string();
        let label = match &source {
            AssetSource::Auto(hit) => hit.title.clone(),
            AssetSource::Chosen(_, label) => label.clone(),
        };
        let thread_id = id.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let client = artwork::http();
            let result = match source {
                AssetSource::Auto(hit) => artwork::fetch(&client, &thread_id, &hit, &settings),
                AssetSource::Chosen(plan, _) => artwork::apply_plan(
                    &client,
                    &crate::config::artwork_root().join(&thread_id),
                    &plan,
                ),
            };
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        self.job = Some(AssetJob {
            rx,
            game_id: game_id.to_string(),
            label,
        });
    }

    /// Recherche les artworks proposables pour le jeu retenu (fenêtre d'ajout).
    pub fn start_artwork_plan(&mut self, ctx: &egui::Context) {
        let Some(hit) = self.add.hit.clone() else {
            self.add.step = AddStep::Exe;
            return;
        };
        self.add.step = AddStep::Artwork;
        self.add.artwork = ArtworkPicker {
            loading: true,
            ..ArtworkPicker::default()
        };
        let (tx, rx) = channel();
        let settings = self.lib.settings.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let client = artwork::http();
            let plan = artwork::build_plan(&client, &hit, &settings);
            let _ = tx.send((plan, Vec::new()));
            ctx.request_repaint();
        });
        self.add.artwork.rx = Some(rx);
    }

    fn poll_artwork_plan(&mut self, ctx: &egui::Context) {
        let received = self.add.artwork.rx.as_ref().map(|rx| rx.try_recv());
        match received {
            Some(Ok((plan, warnings))) => {
                self.add.artwork.rx = None;
                self.add.artwork.loading = false;
                self.add.artwork.error = if warnings.is_empty() {
                    None
                } else {
                    Some(warnings.join(" · "))
                };
                self.add.artwork.plan = Some(plan);
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.add.artwork.rx = None;
                self.add.artwork.loading = false;
                self.add.artwork.error = Some("Recherche des artworks interrompue".to_string());
            }
            _ => {
                if self.add.artwork.loading {
                    ctx.request_repaint_after(Duration::from_millis(120));
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Ajout par dossier : analyse, correspondances, ajout en lot
    // -----------------------------------------------------------------------

    /// Analyse un dossier : détecte les exécutables et cherche chaque titre.
    pub fn start_scan(&mut self, ctx: &egui::Context, folder: PathBuf) {
        self.add.step = AddStep::Scan;
        self.add.scan = ScanJob {
            folder: Some(folder.clone()),
            running: true,
            ..ScanJob::default()
        };
        let (tx, rx) = channel();
        let settings = self.lib.settings.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let client = artwork::http();
            let discovered = crate::scan::scan(&folder, 4, 8);
            let mut candidates = Vec::new();
            for item in discovered {
                let outcome = artwork::search(&client, &item.title, &settings);
                let hit = artwork::best_hit(&outcome, &item.title);
                let (title, detail) = match &hit {
                    Some(h) => (
                        h.title.clone(),
                        format!("{} · {}", h.source.label(), h.detail),
                    ),
                    None => (item.title.clone(), "aucune correspondance".to_string()),
                };
                candidates.push(ScanCandidate {
                    exe: item.exe,
                    title,
                    include: true,
                    hit,
                    detail,
                    searching: false,
                });
            }
            let _ = tx.send(ScanOutcome {
                candidates,
                error: None,
            });
            repaint.request_repaint();
        });
        self.add.scan.rx = Some(rx);
        ctx.request_repaint_after(Duration::from_millis(150));
    }

    fn poll_scan(&mut self) {
        if let Some(rx) = self.add.scan.rx.as_ref() {
            match rx.try_recv() {
                Ok(outcome) => {
                    self.add.scan.rx = None;
                    self.add.scan.running = false;
                    self.add.scan.candidates = outcome.candidates;
                    self.add.scan.error = outcome.error;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.add.scan.rx = None;
                    self.add.scan.running = false;
                    self.add.scan.error = Some("Analyse interrompue".to_string());
                }
            }
        }

        let pending = self
            .add
            .scan
            .search_rx
            .as_ref()
            .map(|(index, rx)| (*index, rx.try_recv()));
        match pending {
            Some((index, Ok(outcome))) => {
                self.add.scan.search_rx = None;
                let title = self
                    .add
                    .scan
                    .candidates
                    .get(index)
                    .map(|c| c.title.clone())
                    .unwrap_or_default();
                let hit = artwork::best_hit(&outcome, &title);
                if let Some(candidate) = self.add.scan.candidates.get_mut(index) {
                    candidate.searching = false;
                    candidate.detail = match &hit {
                        Some(h) => format!("{} · {}", h.source.label(), h.detail),
                        None => "aucune correspondance".to_string(),
                    };
                    if let Some(hit) = hit.clone() {
                        candidate.title = hit.title.clone();
                    }
                    candidate.hit = hit;
                }
            }
            Some((_, Err(TryRecvError::Disconnected))) => {
                self.add.scan.search_rx = None;
                self.add.scan.error = Some("Recherche interrompue".to_string());
            }
            _ => {}
        }
    }

    /// Relance la recherche pour un candidat dont le titre a été corrigé.
    pub fn start_scan_search(&mut self, ctx: &egui::Context, index: usize) {
        let Some(title) = self
            .add
            .scan
            .candidates
            .get(index)
            .map(|c| c.title.trim().to_string())
        else {
            return;
        };
        if title.is_empty() {
            return;
        }
        let (tx, rx) = channel();
        let settings = self.lib.settings.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let client = artwork::http();
            let _ = tx.send(artwork::search(&client, &title, &settings));
            repaint.request_repaint();
        });
        if let Some(candidate) = self.add.scan.candidates.get_mut(index) {
            candidate.searching = true;
        }
        self.add.scan.search_rx = Some((index, rx));
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    /// Ajoute tous les candidats retenus comme jeux.
    pub fn add_scanned(&mut self, ctx: &egui::Context) {
        let picked: Vec<(String, PathBuf, Option<SearchHit>)> = self
            .add
            .scan
            .candidates
            .iter()
            .filter(|c| c.include && !c.title.trim().is_empty())
            .map(|c| (c.title.trim().to_string(), c.exe.clone(), c.hit.clone()))
            .collect();
        if picked.is_empty() {
            self.toast_error("Aucun jeu sélectionné");
            return;
        }
        let proton = self.add.proton.clone();
        let settings = self.lib.settings.clone();
        let mut added = 0usize;
        for (title, exe, hit) in picked {
            let mut game = Game::new(title, exe.display().to_string());
            game.proton = proton.clone();
            // Chaque jeu ajouté reçoit sa copie des réglages « par défaut ».
            game.args = settings.default_args.trim().to_string();
            game.prefix = settings
                .default_prefix
                .clone()
                .filter(|p| !p.trim().is_empty());
            game.anticheat = settings.default_anticheat;
            game.launch = settings.default_launch.clone();
            game.proton_log = settings.default_proton_log;
            game.umu = settings.default_umu;
            game.env = settings.default_env.clone();
            if let Some(hit) = &hit {
                game.steam_app_id = hit.steam_app_id;
                game.sgdb_id = hit.sgdb_id;
            }
            let id = game.id.clone();
            self.selected = Some(id.clone());
            self.lib.games.push(game);
            if let Some(hit) = hit {
                self.start_asset_job(ctx, &id, hit);
            }
            added += 1;
        }
        self.add.open = false;
        self.add.scan = ScanJob::default();
        self.touch();
        self.toast_info(format!("{added} jeu(x) ajouté(s)"));
    }

    // -----------------------------------------------------------------------
    // Personnalisation (artworks d'un jeu)
    // -----------------------------------------------------------------------

    pub fn open_customize(&mut self, id: &str) {
        self.customize = Some(Customize::for_game(id));
    }

    /// Cherche les propositions d'images d'un type (SteamGridDB, puis Steam).
    pub fn start_customize_plan(&mut self, ctx: &egui::Context, kind: AssetKind) {
        let Some(game_id) = self.customize.as_ref().map(|c| c.game_id.clone()) else {
            return;
        };
        let Some(game) = self.lib.get(&game_id).cloned() else {
            return;
        };
        let settings = self.lib.settings.clone();
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let client = artwork::http();
            let hit = SearchHit::from_game(&game);
            let plan = artwork::build_plan(&client, &hit, &settings).only(kind);
            let _ = tx.send(plan);
            repaint.request_repaint();
        });
        if let Some(customize) = self.customize.as_mut() {
            customize.kind = Some(kind);
            customize.loading = true;
            customize.plan = None;
            customize.error = None;
            customize.rx = Some(rx);
        }
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    fn poll_customize_plan(&mut self, ctx: &egui::Context) {
        let received = self
            .customize
            .as_ref()
            .and_then(|customize| customize.rx.as_ref().map(|rx| rx.try_recv()));
        match received {
            Some(Ok(plan)) => {
                if let Some(customize) = self.customize.as_mut() {
                    customize.rx = None;
                    customize.loading = false;
                    customize.plan = Some(plan);
                }
            }
            Some(Err(TryRecvError::Disconnected)) => {
                if let Some(customize) = self.customize.as_mut() {
                    customize.rx = None;
                    customize.loading = false;
                    customize.error = Some("Recherche des artworks interrompue".to_string());
                }
            }
            Some(Err(TryRecvError::Empty)) | None => {
                if self
                    .customize
                    .as_ref()
                    .is_some_and(|customize| customize.loading)
                {
                    ctx.request_repaint_after(Duration::from_millis(120));
                }
            }
        }
    }

    /// télécharge la proposition cliquée, et elle seule.
    pub fn apply_customize_pick(&mut self, ctx: &egui::Context, kind: AssetKind, index: usize) {
        let Some((game_id, plan)) = self.customize.as_ref().and_then(|customize| {
            customize
                .plan
                .clone()
                .map(|plan| (customize.game_id.clone(), plan))
        }) else {
            return;
        };
        let label = self
            .lib
            .get(&game_id)
            .map(|game| game.name.clone())
            .unwrap_or_default();
        // Plan restreint à ce type : les trois autres images ne bougent pas.
        let plan = plan.only(kind).with_choices(&[(kind, index)]);
        self.queue_asset_job(ctx, &game_id, AssetSource::Chosen(plan, label));
        if let Some(customize) = self.customize.as_mut() {
            customize.kind = None;
            customize.plan = None;
        }
    }

    /// Importe une image choisie sur le disque (sélecteur natif).
    pub fn import_artwork(&mut self, kind: AssetKind, source: PathBuf) {
        let Some(game_id) = self.customize.as_ref().map(|c| c.game_id.clone()) else {
            return;
        };
        let dir = crate::config::artwork_root().join(&game_id);
        match artwork::import_file(&source, &dir, kind) {
            Ok(path) => {
                if let Some(game) = self.lib.get_mut(&game_id) {
                    game.artwork.set(kind, path);
                }
                self.toast_info(format!("{} importée", kind.label()));
                self.touch();
            }
            Err(err) => self.toast_error(format!("Import impossible : {err}")),
        }
    }

    /// Retire l'image d'un type : le rendu de secours reprend sa place.
    pub fn remove_artwork(&mut self, game_id: &str, kind: AssetKind) {
        let Some(game) = self.lib.get_mut(game_id) else {
            return;
        };
        if let Some(path) = game.artwork.get(kind).cloned() {
            // Uniquement nos propres fichiers : jamais une image d'ailleurs.
            if path.starts_with(crate::config::artwork_root()) {
                let _ = std::fs::remove_file(&path);
            }
        }
        game.artwork.clear(kind);
        self.toast_info(format!("{} retirée", kind.label()));
        self.touch();
    }

    pub fn pick_file_for_artwork(&mut self, ctx: &egui::Context, start: PathBuf, kind: AssetKind) {
        self.spawn_picker(ctx, start, PickerTarget::CustomArtwork(kind), false);
    }

    // -----------------------------------------------------------------------
    // Lancement
    // -----------------------------------------------------------------------

    pub fn launch(&mut self, ctx: &egui::Context, id: &str) {
        if self.running.contains_key(id) {
            return;
        }
        let Some(game) = self.lib.get(id).cloned() else {
            return;
        };
        // Exécutable disparu (jeu déplacé, disque démonté) : le lancement est
        // refusé et le jeu passe en rouge dans la liste.
        if game.exe_missing() {
            self.missing_flash.insert(id.to_string(), Instant::now());
            self.toast_error(format!(
                "{} : exécutable introuvable — {}",
                game.name,
                if game.exe.trim().is_empty() {
                    "aucun chemin défini".to_string()
                } else {
                    game.exe.clone()
                }
            ));
            return;
        }
        match proton::resolve(&game, &self.lib.settings, &self.runtimes) {
            Ok(resolved) => {
                // Une option demandée mais impossible (gamescope absent) ne
                // bloque pas le jeu : elle est ignorée, et dite ici.
                for warning in &resolved.warnings {
                    self.toast_error(format!("{} : {warning}", game.name));
                }
                if let Some(prefix) = &resolved.prefix {
                    if let Err(err) = std::fs::create_dir_all(prefix) {
                        self.toast_error(format!("Préfixe Proton : {err}"));
                        return;
                    }
                }
                // La sauvegarde « avant lancement » démarre *avant* le
                // processus du jeu : lancée après le spawn, elle
                // photographierait des fichiers déjà ouverts en écriture.
                let save_identity = self.save_identity_for_quark_game(id);
                if let Some(identity) = save_identity.clone() {
                    self.start_background_save(identity, crate::saves::BackupPhase::BeforeLaunch);
                }
                match crate::session::start(&game, &resolved) {
                    Ok(mut session) => {
                        session.save_identity = save_identity;
                        self.running.insert(id.to_string(), session);
                        self.toast_info(format!("{} lancé · {}", game.name, resolved.label()));
                        // Nouvelle session : le journal ouvert repart de zéro.
                        if self.logs.as_ref().is_some_and(|logs| logs.game_id == id) {
                            self.refresh_log();
                        }
                        ctx.request_repaint();
                    }
                    Err(err) => self.toast_error(format!("Lancement impossible : {err}")),
                }
            }
            Err(err) => self.toast_error(err),
        }
    }

    /// Arrête le jeu : `wineserver -k` du préfixe puis le groupe de processus.
    pub fn stop(&mut self, id: &str) {
        let name = self
            .lib
            .get(id)
            .map(|game| game.name.clone())
            .unwrap_or_default();
        let Some(running) = self.running.get_mut(id) else {
            return;
        };
        crate::session::stop(running);
        self.toast_info(format!("Arrêt de {name}\u{2026}"));
        if self.logs.as_ref().is_some_and(|logs| logs.game_id == id) {
            self.refresh_log();
        }
    }

    // -----------------------------------------------------------------------
    // Journal de session (bouton « Journal »)
    // -----------------------------------------------------------------------

    /// Ouvre le journal de la dernière session du jeu.
    pub fn open_logs(&mut self, id: &str) {
        // Le chemin de la session en cours d'abord : c'est le fichier que le
        // jeu écrit maintenant.
        let path = self
            .running
            .get(id)
            .map(|session| session.log.clone())
            .unwrap_or_else(|| crate::config::session_log(id));
        self.logs = Some(LogView {
            game_id: id.to_string(),
            path,
            lines: Vec::new(),
            truncated: false,
            error: None,
            updated: Instant::now(),
        });
        self.refresh_log();
    }

    /// Relit la fin du journal (le fichier peut grossir pendant la partie).
    pub fn refresh_log(&mut self) {
        let Some(view) = self.logs.as_mut() else {
            return;
        };
        match crate::session::read_log_tail(&view.path, 500) {
            Ok((lines, truncated)) => {
                view.lines = lines;
                view.truncated = truncated;
                view.error = None;
            }
            Err(err) => {
                view.lines.clear();
                view.error = Some(err);
            }
        }
        view.updated = Instant::now();
    }

    // -----------------------------------------------------------------------
    // Note ProtonDB
    // -----------------------------------------------------------------------

    /// Charge la note ProtonDB du jeu sélectionné, une seule fois par appid.
    fn ensure_protondb(&mut self, ctx: &egui::Context) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let Some(app_id) = self.lib.get(&id).and_then(|game| game.steam_app_id) else {
            return;
        };
        if self.protondb.contains_key(&app_id) {
            return;
        }
        if self
            .protondb_job
            .as_ref()
            .is_some_and(|job| job.app_id == app_id)
        {
            return;
        }
        self.start_protondb(ctx, app_id);
    }

    pub fn start_protondb(&mut self, ctx: &egui::Context, app_id: u32) {
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let client = artwork::http();
            let _ = tx.send(protondb::fetch(&client, app_id));
            repaint.request_repaint();
        });
        self.protondb.insert(app_id, ProtonDbState::Loading);
        self.protondb_job = Some(ProtonDbJob { app_id, rx });
        ctx.request_repaint_after(Duration::from_millis(200));
    }

    fn poll_protondb(&mut self) {
        let received = self
            .protondb_job
            .as_ref()
            .map(|job| (job.app_id, job.rx.try_recv()));
        match received {
            Some((app_id, Ok(Ok(report)))) => {
                self.protondb_job = None;
                self.protondb.insert(app_id, ProtonDbState::Ready(report));
            }
            Some((app_id, Ok(Err(message)))) => {
                self.protondb_job = None;
                self.protondb.insert(app_id, ProtonDbState::Failed(message));
            }
            Some((app_id, Err(TryRecvError::Disconnected))) => {
                self.protondb_job = None;
                self.protondb.insert(
                    app_id,
                    ProtonDbState::Failed("requête ProtonDB interrompue".to_string()),
                );
            }
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // Icône de la zone de notification
    // -----------------------------------------------------------------------

    /// Démarrage de la fenêtre : inscrit son PID et lance l'icône si elle est
    /// activée. Appelé une seule fois par `main`, jamais par les rendus hors
    /// écran (`quark --ui`), qui ne doivent démarrer aucun processus.
    pub fn startup(&mut self) {
        tray::write_gui_pid();
        if self.lib.settings.tray_icon {
            self.tray_error = tray::ensure_helper().err();
        }
    }

    /// Applique le réglage « zone de notification » : l'icône est tenue par un
    /// processus dédié, on la démarre ou on l'arrête donc ici.
    pub fn set_tray_icon(&mut self, enabled: bool) {
        self.lib.settings.tray_icon = enabled;
        if enabled {
            self.tray_error = tray::ensure_helper().err();
        } else {
            tray::stop_helper();
            self.tray_error = None;
        }
        self.touch();
    }

    // -----------------------------------------------------------------------
    // Barre supérieure & éléments communs
    // -----------------------------------------------------------------------

    fn top_bar(&mut self, ctx: &egui::Context) {
        let pal = self.pal;
        egui::TopBottomPanel::top("quark_topbar")
            .exact_height(56.0)
            .frame(
                egui::Frame::new()
                    .fill(pal.header)
                    .stroke(Stroke::new(1.0_f32, pal.border)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(18.0);
                    let (mark_rect, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                    let painter = ui.painter_at(mark_rect);
                    painter.circle_stroke(
                        mark_rect.center(),
                        8.0,
                        Stroke::new(2.0_f32, pal.accent),
                    );
                    painter.circle_stroke(
                        mark_rect.center(),
                        3.0,
                        Stroke::new(2.0_f32, pal.text_dim),
                    );
                    painter.circle_filled(
                        mark_rect.center() + Vec2::new(7.0, 4.0),
                        2.5,
                        pal.accent_hover,
                    );
                    ui.add_space(4.0);
                    ui.label(RichText::new("QUARK").size(16.0).strong().color(pal.text));
                    ui.add_space(30.0);

                    if widgets::tab(ui, "Boutique", self.tab == Tab::Store, &pal).clicked() {
                        self.tab = Tab::Store;
                    }
                    if widgets::tab(ui, "Mes jeux", self.tab == Tab::MyGames, &pal).clicked() {
                        self.tab = Tab::MyGames;
                    }
                    if widgets::tab(ui, "Bibliothèque", self.tab == Tab::Library, &pal).clicked() {
                        self.tab = Tab::Library;
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(16.0);
                        if widgets::icon_button(
                            ui,
                            32.0,
                            Icon::Gear,
                            &pal,
                            "Paramètres · Ctrl+Q pour quitter",
                        )
                        .clicked()
                        {
                            self.settings_open = true;
                        }
                        ui.add_space(6.0);
                        // Seul endroit où l'on bascule de vue : la colonne de
                        // gauche et la page grille ne les dupliquent plus.
                        if self.tab == Tab::Library {
                            ui.add_space(4.0);
                            if widgets::icon_button(ui, 28.0, Icon::Grid, &pal, "Vue grille")
                                .clicked()
                            {
                                self.set_view(ViewMode::Grid);
                            }
                            if widgets::icon_button(ui, 28.0, Icon::List, &pal, "Vue liste (Steam)")
                                .clicked()
                            {
                                self.set_view(ViewMode::List);
                            }
                            if widgets::icon_button(ui, 28.0, Icon::Plus, &pal, "Ajouter un jeu")
                                .clicked()
                            {
                                self.open_add_dialog();
                            }
                            ui.add_space(6.0);
                        }
                        if self.tab == Tab::Library {
                            ui.add_sized(
                                Vec2::new(260.0, 30.0),
                                egui::TextEdit::singleline(&mut self.filter)
                                    .id(egui::Id::new("quark_filter"))
                                    .hint_text("Rechercher dans la bibliothèque  ·  Ctrl+F")
                                    .font(FontId::proportional(12.5)),
                            );
                        } else if self.tab == Tab::Store {
                            ui.label(
                                RichText::new("Catalogue Steam")
                                    .size(11.5)
                                    .color(pal.text_faint),
                            );
                        }
                    });
                });
            });
    }

    fn toasts_ui(&mut self, ctx: &egui::Context) {
        if self.toasts.is_empty() {
            return;
        }
        let pal = self.pal;
        egui::Area::new(egui::Id::new("quark_toasts"))
            .order(egui::Order::Foreground)
            .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-18.0, -18.0))
            .show(ctx, |ui| {
                let now = Instant::now();
                ui.with_layout(Layout::bottom_up(Align::RIGHT), |ui| {
                    for toast in self.toasts.iter().rev().take(4) {
                        let color = match toast.kind {
                            ToastKind::Info => pal.border,
                            ToastKind::Error => pal.danger,
                        };
                        // Apparition puis disparition en fondu. Aucune image
                        // n'est demandée en dehors de ces deux transitions.
                        let age = now.saturating_duration_since(toast.created).as_secs_f32();
                        let left = toast.until.saturating_duration_since(now).as_secs_f32();
                        let alpha = (age / 0.18)
                            .clamp(0.0, 1.0)
                            .min((left / 0.45).clamp(0.0, 1.0));
                        if alpha < 1.0 {
                            ui.ctx().request_repaint_after(Duration::from_millis(16));
                        }
                        let text_color = if toast.kind == ToastKind::Error {
                            pal.danger
                        } else {
                            pal.text
                        };
                        egui::Frame::new()
                            .fill(pal.surface.gamma_multiply(alpha))
                            .stroke(Stroke::new(1.0_f32, color.gamma_multiply(alpha)))
                            .corner_radius(6.0)
                            .inner_margin(egui::Margin::symmetric(14, 9))
                            .show(ui, |ui| {
                                ui.set_max_width(380.0);
                                ui.label(
                                    RichText::new(&toast.text)
                                        .size(12.5)
                                        .color(text_color.gamma_multiply(alpha)),
                                );
                            });
                        ui.add_space(6.0);
                    }
                });
            });
    }

    /// Jeux affichés dans la colonne de gauche : filtre de recherche, de
    /// genre, de tag et de statut, comme les catégories Steam.
    pub fn display_ids(&self) -> Vec<String> {
        let needle = self.filter.trim().to_lowercase();
        let settings = &self.lib.settings;
        self.lib
            .sorted_ids()
            .into_iter()
            .filter(|id| {
                let Some(game) = self.lib.get(id) else {
                    return false;
                };
                if !self.lib.matches(
                    game,
                    &settings.home_genre,
                    &settings.home_tag,
                    settings.home_status,
                ) {
                    return false;
                }
                if needle.is_empty() {
                    return true;
                }
                game.name.to_lowercase().contains(&needle)
                    || game.developer.to_lowercase().contains(&needle)
                    || game
                        .tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&needle))
                    || game
                        .genres
                        .iter()
                        .any(|genre| genre.to_lowercase().contains(&needle))
            })
            .collect()
    }

    /// Sélectionne un jeu ; bascule sur la bibliothèque si besoin. Un clic
    /// simple abandonne la sélection multiple (Ctrl+clic la construit).
    pub fn select(&mut self, id: &str) {
        self.selected = Some(id.to_string());
        self.marked.clear();
        // Choisir un jeu quitte l'accueil : la fiche prend sa place.
        self.home = false;
    }

    /// Ctrl+clic : ajoute ou retire un jeu de la sélection multiple. Le panneau
    /// de droite suit le dernier jeu coché.
    pub fn toggle_marked(&mut self, id: &str) {
        let position = self.marked.iter().position(|marked| marked == id);
        match position {
            Some(index) => {
                self.marked.remove(index);
            }
            None => {
                self.marked.push(id.to_string());
                self.selected = Some(id.to_string());
            }
        }
    }

    /// Couleur d'accent d'un jeu : la teinte dominante de sa couverture, sinon
    /// celle du thème. Le calcul est fait une seule fois par image.
    pub fn game_accent(&mut self, game: &Game) -> egui::Color32 {
        let Some(path) = game
            .artwork
            .get(AssetKind::Cover)
            .filter(|path| path.exists())
            .cloned()
        else {
            return self.pal.accent;
        };
        if let Some(color) = self.accents.get(&path) {
            return *color;
        }
        match crate::images::accent_from_path(&path) {
            Some(color) => {
                self.accents.insert(path, color);
                color
            }
            None => self.pal.accent,
        }
    }

    pub fn is_marked(&self, id: &str) -> bool {
        self.marked.iter().any(|marked| marked == id)
    }

    /// Demande le retrait de toute la sélection multiple (confirmation ensuite).
    pub fn ask_remove_marked(&mut self) {
        if self.marked.is_empty() {
            return;
        }
        self.confirm_remove = self.marked.clone();
    }

    /// Demande le retrait depuis le clic droit : tout le lot si le jeu visé
    /// fait partie de la sélection multiple, lui seul sinon.
    pub fn request_remove(&mut self, id: &str) {
        let targets = removal_targets(&self.marked, id);
        if targets.len() == 1 {
            self.select(id);
        }
        self.confirm_remove = targets;
    }
}

/// Jeux à retirer quand on demande la suppression de `id` : la sélection
/// multiple entière si `id` en fait partie, sinon ce seul jeu.
pub fn removal_targets(marked: &[String], id: &str) -> Vec<String> {
    if marked.len() > 1 && marked.iter().any(|item| item == id) {
        marked.to_vec()
    } else {
        vec![id.to_string()]
    }
}

impl QuarkApp {
    /// Une image de l'interface. Le rendu est le même à l'écran et hors écran
    /// (`quark --ui`), qui n'a pas de `eframe::Frame`.
    pub fn ui(&mut self, ctx: &egui::Context) {
        self.tick(ctx);
        self.keyboard(ctx);

        if self.applied_theme != Some(self.lib.settings.theme) {
            theme::apply(ctx, self.lib.settings.theme);
            self.pal = theme::palette(self.lib.settings.theme);
            self.applied_theme = Some(self.lib.settings.theme);
        }

        self.frames_alive = self.frames_alive.saturating_add(1);
        // Nouvelle page (ou nouvelle disposition) : la grille repart de zéro et
        // rejoue son apparition.
        if self.tab != self.last_tab || self.lib.settings.view != self.last_view {
            self.last_tab = self.tab;
            self.last_view = self.lib.settings.view;
            self.page_at = Instant::now();
        }
        self.startup_sync(ctx);
        self.top_bar(ctx);
        match self.tab {
            Tab::Store => self.store_tab(ctx),
            Tab::Library => self.library_tab(ctx),
            Tab::MyGames => self.my_games_tab(ctx),
        }

        self.add_dialog(ctx);
        self.settings_dialog(ctx);
        self.steam_import_dialog(ctx);
        self.customize_dialog(ctx);
        self.game_settings_dialog(ctx);
        self.confirm_remove_dialog(ctx);
        self.logs_dialog(ctx);
        self.save_backup_dialog(ctx);
        self.toasts_ui(ctx);
    }

    /// Navigation au clavier, sans souris : flèches pour changer de jeu, Entrée
    /// pour lancer, Ctrl+F (ou « / ») pour la recherche, Échap pour la vider.
    /// Les raccourcis sont ignorés tant qu'une fenêtre ou un champ de saisie a
    /// le focus, pour ne jamais voler une frappe.
    fn keyboard(&mut self, ctx: &egui::Context) {
        let quit = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Q));
        if quit {
            self.quit(ctx);
            return;
        }
        if self.add.open
            || self.settings_open
            || self.editing.is_some()
            || self.customize.is_some()
            || self.logs.is_some()
            || self.steam_import.is_some()
            || !self.confirm_remove.is_empty()
        {
            return;
        }
        let typing = ctx.wants_keyboard_input();
        let filter_id = egui::Id::new("quark_filter");
        let (down, up, left, right, enter, escape, find, slash) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
                i.modifiers.command && i.key_pressed(egui::Key::F),
                i.key_pressed(egui::Key::Slash),
            )
        });

        if find || (!typing && slash) {
            ctx.memory_mut(|memory| memory.request_focus(filter_id));
            return;
        }
        if typing {
            if escape {
                ctx.memory_mut(|memory| memory.surrender_focus(filter_id));
            }
            return;
        }
        if escape {
            if self.tab != Tab::Library {
                self.tab = Tab::Library;
            } else if self.home {
                self.home = false;
            } else if !self.filter.is_empty() {
                self.filter.clear();
            } else if !self.marked.is_empty() {
                self.marked.clear();
            }
            return;
        }

        let Some(current_id) = self.selected.clone() else {
            return;
        };
        if enter {
            self.launch(ctx, &current_id);
            return;
        }
        if !(down || up || left || right) {
            return;
        }
        let ids = self.display_ids();
        if ids.is_empty() {
            return;
        }
        // Dans la grille, haut/bas suivent les colonnes ; en liste, tout
        // déplace d'une ligne.
        let columns = if self.lib.settings.view == ViewMode::Grid {
            3
        } else {
            1
        };
        let step: i64 = if down {
            columns
        } else if up {
            -columns
        } else if right {
            1
        } else {
            -1
        };
        let index = ids
            .iter()
            .position(|id| id == &current_id)
            .unwrap_or_default();
        let next = (index as i64 + step).clamp(0, ids.len() as i64 - 1) as usize;
        if next != index {
            let id = ids[next].clone();
            self.select(&id);
        }
    }
}

impl eframe::App for QuarkApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Les sessions encore en cours sont comptabilisées avant la fermeture,
        // sinon le temps de jeu est perdu en quittant le launcher.
        let sessions: Vec<(String, u64, i64)> = self
            .running
            .iter()
            .map(|(id, game)| {
                (
                    id.clone(),
                    game.started.elapsed().as_secs(),
                    game.started_ts,
                )
            })
            .collect();
        for (id, secs, started_ts) in sessions {
            if let Some(game) = self.lib.get_mut(&id) {
                game.record_session(started_ts, secs);
            }
        }
        let _ = self.lib.save();
        // La fenêtre n'existe plus : l'icône ne doit pas la croire ouverte.
        tray::clear_gui_pid();
    }
}

impl QuarkApp {
    pub fn is_running(&self, id: &str) -> bool {
        self.running.contains_key(id)
    }

    pub fn running_since(&self, id: &str) -> u64 {
        self.running.get(id).map(|r| r.session).unwrap_or(0)
    }

    // -----------------------------------------------------------------------
    // Mises à jour (releases GitHub)
    // -----------------------------------------------------------------------

    /// Interroge la dernière release publiée (non bloquant).
    pub fn check_updates(&mut self, ctx: &egui::Context) {
        if matches!(
            self.update,
            UpdateState::Checking | UpdateState::Downloading(_)
        ) {
            return;
        }
        self.update = UpdateState::Checking;
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let client = artwork::http();
            let _ = tx.send(UpdateMessage::Checked(update::latest(&client)));
            ctx.request_repaint();
        });
        self.update_rx = Some(rx);
    }

    /// Télécharge l'archive de la release proposée (jamais installée à notre
    /// place : le binaire courant n'est pas remplacé).
    pub fn download_update(&mut self, ctx: &egui::Context) {
        let UpdateState::Available(release) = self.update.clone() else {
            return;
        };
        let Some(asset) = release.archive().cloned() else {
            self.update = UpdateState::Failed("aucune archive dans cette release".to_string());
            return;
        };
        let tag = release.tag_name.clone();
        self.update = UpdateState::Downloading(release);
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        let dir = crate::config::data_dir().join("updates");
        std::thread::spawn(move || {
            let client = artwork::http();
            let result = update::download(&client, &asset, &dir);
            let _ = tx.send(UpdateMessage::Downloaded(tag, result));
            ctx.request_repaint();
        });
        self.update_rx = Some(rx);
    }

    fn poll_update(&mut self, ctx: &egui::Context) {
        let received = self.update_rx.as_ref().map(|rx| rx.try_recv());
        match received {
            Some(Ok(UpdateMessage::Checked(Ok(release)))) => {
                self.update_rx = None;
                self.update = if update::is_newer(&release.tag_name, update::current_version()) {
                    UpdateState::Available(release)
                } else {
                    UpdateState::UpToDate
                };
            }
            Some(Ok(UpdateMessage::Checked(Err(err)))) => {
                self.update_rx = None;
                self.update = UpdateState::Failed(err);
            }
            Some(Ok(UpdateMessage::Downloaded(tag, Ok(path)))) => {
                self.update_rx = None;
                self.update = UpdateState::Downloaded(path.clone());
                self.toast_info(format!(
                    "{tag} téléchargée dans {}",
                    path.parent().unwrap_or(&path).display()
                ));
            }
            Some(Ok(UpdateMessage::Downloaded(_, Err(err)))) => {
                self.update_rx = None;
                self.update = UpdateState::Failed(err);
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.update_rx = None;
                self.update = UpdateState::Failed("vérification interrompue".to_string());
            }
            _ => {
                if matches!(
                    self.update,
                    UpdateState::Checking | UpdateState::Downloading(_)
                ) {
                    ctx.request_repaint_after(Duration::from_millis(150));
                }
            }
        }
    }

    /// Jeux du carrousel de l'accueil (les dernières sessions).
    pub fn recent_ids(&self) -> Vec<String> {
        self.lib.recent_ids(crate::config::HomeSort::CAROUSEL)
    }

    /// Vue par défaut = réglage ; la vue courante est stockée dans les réglages.
    pub fn set_view(&mut self, view: ViewMode) {
        if self.lib.settings.view != view {
            self.lib.settings.view = view;
            self.touch();
        }
    }

    /// Ctrl+Q : termine Quark **et** l'icône de notification. La croix, elle,
    /// respecte le réglage de la zone de notification.
    pub fn quit(&mut self, ctx: &egui::Context) {
        self.quitting = true;
        let _ = self.lib.save();
        tray::stop_helper();
        tray::clear_gui_pid();
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    // -----------------------------------------------------------------------
    // Import de la bibliothèque Steam installée
    // -----------------------------------------------------------------------

    pub fn open_steam_import(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(steamlib::installed());
            repaint.request_repaint();
        });
        let mut import = SteamImport::new();
        import.rx = Some(rx);
        self.steam_import = Some(import);
        ctx.request_repaint_after(Duration::from_millis(150));
    }

    fn poll_steam_import(&mut self, ctx: &egui::Context) {
        let received = self
            .steam_import
            .as_ref()
            .and_then(|import| import.rx.as_ref())
            .map(|rx| rx.try_recv());
        match received {
            Some(Ok(Ok(games))) => {
                let owned: Vec<u32> = self
                    .lib
                    .games
                    .iter()
                    .filter_map(|game| game.steam_app_id)
                    .collect();
                if let Some(import) = self.steam_import.as_mut() {
                    import.loading = false;
                    import.rx = None;
                    import.games = games
                        .into_iter()
                        .map(|game| SteamCandidate {
                            owned: owned.contains(&game.app_id),
                            include: !owned.contains(&game.app_id),
                            app_id: game.app_id,
                            name: game.name,
                            dir: game.dir,
                            exe: game.exe,
                        })
                        .collect();
                }
            }
            Some(Ok(Err(err))) => {
                if let Some(import) = self.steam_import.as_mut() {
                    import.loading = false;
                    import.rx = None;
                    import.error = Some(err);
                }
            }
            Some(Err(TryRecvError::Disconnected)) => {
                if let Some(import) = self.steam_import.as_mut() {
                    import.loading = false;
                    import.rx = None;
                    import.error = Some("lecture de Steam interrompue".to_string());
                }
            }
            _ => {}
        }
        if self
            .steam_import
            .as_ref()
            .is_some_and(|import| import.loading)
        {
            // Le thread de lecture ne peut pas réveiller egui : on demande une
            // image de temps en temps pendant l'analyse des manifestes.
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }

    /// Ajoute les jeux Steam cochés (jamais deux fois le même AppID).
    fn ensure_store(&mut self, ctx: &egui::Context) {
        if self.store_loading || self.store_state != StoreLoadState::Idle {
            return;
        }
        if self.store_snapshot.is_none() || self.store_stale {
            self.refresh_store(ctx);
        } else {
            self.store_state = StoreLoadState::Ready;
        }
    }

    pub fn refresh_store(&mut self, ctx: &egui::Context) {
        if self.store_loading || self.store_offline_mode {
            return;
        }
        self.store_state = StoreLoadState::Loading;
        self.store_loading = true;
        self.store_error = None;
        self.store_notice = None;
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        let language = self.lib.settings.language.clone();
        let country = self.lib.settings.country.clone();
        // La clé Web API ne quitte jamais ce processus sauf vers api.steampowered.com.
        let steam_key = self.credentials.steam_web_api_key.clone();
        let steam_id = self.credentials.steam_id.clone();
        std::thread::spawn(move || {
            let result = crate::steam_store::fetch_all(
                &artwork::http(),
                &language,
                &country,
                steam_key.as_deref(),
                steam_id.as_deref(),
            );
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.store_featured_rx = Some(rx);
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    /// Recalcule les recommandations depuis l'état courant (catalogue chargé
    /// et meilleure bibliothèque connue).
    fn rebuild_store_recommendations(&mut self) {
        let (recommendations, source) = crate::steam_store::recommendations_for(
            self.store_snapshot.as_ref(),
            &self.lib.games,
            self.store_steam_library.as_deref(),
        );
        self.store_recommendations = recommendations;
        self.store_recommendation_source = source;
    }

    pub fn search_store(&mut self, ctx: &egui::Context) {
        let query = self.store_query.trim().to_string();
        if query.len() < 2 || self.store_search_loading || self.store_offline_mode {
            return;
        }
        self.store_search_loading = true;
        self.store_search_error = None;
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        let language = self.lib.settings.language.clone();
        let country = self.lib.settings.country.clone();
        std::thread::spawn(move || {
            let result = crate::steam_store::search(&artwork::http(), &query, &language, &country);
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.store_search_rx = Some(rx);
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    fn poll_store(&mut self, ctx: &egui::Context) {
        if let Some(result) = self.store_featured_rx.as_ref().map(Receiver::try_recv) {
            match result {
                Ok(Ok(fetch)) => {
                    let _ = crate::steam_store::save_cache_atomic(
                        &crate::steam_store::cache_path(),
                        &fetch.snapshot,
                    );
                    self.store_snapshot = Some(fetch.snapshot);
                    self.store_steam_library = fetch.steam_library;
                    self.store_notice = fetch.popular_error.or(fetch.steam_library_error);
                    self.store_stale = false;
                    self.store_error = None;
                    self.store_state = StoreLoadState::Ready;
                    self.store_loading = false;
                    self.store_featured_rx = None;
                    self.rebuild_store_recommendations();
                }
                Ok(Err(error)) => {
                    self.store_error = Some(error);
                    self.store_loading = false;
                    self.store_stale = self
                        .store_snapshot
                        .as_ref()
                        .is_some_and(|cache| cache.is_stale(crate::config::now_ts()));
                    self.store_state = if self.store_snapshot.is_some() {
                        StoreLoadState::Ready
                    } else {
                        StoreLoadState::Failed
                    };
                    self.store_featured_rx = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.store_error = Some("chargement des offres interrompu".to_string());
                    self.store_loading = false;
                    self.store_stale = self
                        .store_snapshot
                        .as_ref()
                        .is_some_and(|cache| cache.is_stale(crate::config::now_ts()));
                    self.store_state = if self.store_snapshot.is_some() {
                        StoreLoadState::Ready
                    } else {
                        StoreLoadState::Failed
                    };
                    self.store_featured_rx = None;
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(120)),
            }
        }
        if let Some(result) = self.store_search_rx.as_ref().map(Receiver::try_recv) {
            match result {
                Ok(Ok(items)) => {
                    self.store_search_results = Some(items);
                    self.store_search_error = None;
                    self.store_search_loading = false;
                    self.store_search_rx = None;
                }
                Ok(Err(error)) => {
                    self.store_search_error = Some(error);
                    self.store_search_loading = false;
                    self.store_search_rx = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.store_search_error = Some("recherche interrompue".to_string());
                    self.store_search_loading = false;
                    self.store_search_rx = None;
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(120)),
            }
        }
    }

    fn save_identity_for_key(
        &self,
        key: &crate::providers::ProviderGameKey,
        title: &str,
    ) -> Result<crate::saves::SaveIdentity, String> {
        let game = self
            .lib
            .provider_library
            .games
            .iter()
            .find(|game| {
                game.key == *key && game.install_state == crate::providers::InstallState::Installed
            })
            .ok_or_else(|| "installation fournisseur vérifiée requise".to_string())?;
        let install_path = game
            .install_path
            .clone()
            .filter(|path| path.is_dir())
            .ok_or_else(|| "chemin d'installation introuvable".to_string())?;
        Ok(crate::saves::SaveIdentity {
            provider_key: game.key.clone(),
            install_path: Some(install_path),
            ludusavi_title: title.to_string(),
            custom_paths: game.custom_save_paths.clone(),
        })
    }

    pub fn choose_save_folder(
        &mut self,
        ctx: &egui::Context,
        key: &crate::providers::ProviderGameKey,
    ) {
        self.pick_directory(
            ctx,
            crate::config::home_dir(),
            PickerTarget::SaveFolder(key.clone()),
        );
    }

    pub fn remove_save_folder(&mut self, key: &crate::providers::ProviderGameKey, path: &PathBuf) {
        if let Some(game) = self
            .lib
            .provider_library
            .games
            .iter_mut()
            .find(|game| &game.key == key)
        {
            game.custom_save_paths.retain(|candidate| candidate != path);
            self.touch();
        }
    }

    pub fn open_save_inventory(
        &mut self,
        ctx: &egui::Context,
        key: &crate::providers::ProviderGameKey,
    ) {
        if self.save_job.is_some()
            || self.save_confirm_rx.is_some()
            || self.save_restore_rx.is_some()
        {
            return;
        }
        let Some(game) = self
            .lib
            .provider_library
            .games
            .iter()
            .find(|game| &game.key == key)
            .cloned()
        else {
            return;
        };
        let identity = match self.save_identity_for_key(key, &game.title) {
            Ok(identity) => identity,
            Err(error) => {
                self.toast_error(format!("Inventaire indisponible : {error}"));
                return;
            }
        };
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = crate::saves::SaveManager::new()
                .backups(&identity)
                .map(SaveTaskOutput::Backups);
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.save_job = Some(SaveJob {
            key: game.key,
            title: game.title,
            rx,
        });
        self.toast_info("Inventaire Ludusavi en cours…");
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    pub fn preview_save_restore(&mut self, ctx: &egui::Context, backup: &crate::saves::BackupInfo) {
        if self.save_job.is_some()
            || self.save_confirm_rx.is_some()
            || self.save_restore_rx.is_some()
        {
            return;
        }
        let Some((key, title, backups)) = self.save_inventory.as_ref() else {
            return;
        };
        if !backups.iter().any(|item| item.id == backup.id) {
            return;
        }
        let key = key.clone();
        let title = title.clone();
        let backup = backup.clone();
        let identity = match self.save_identity_for_key(&key, &title) {
            Ok(identity) => identity,
            Err(error) => {
                self.toast_error(format!("Aperçu de restauration indisponible : {error}"));
                return;
            }
        };
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        let thread_backup = backup.clone();
        std::thread::spawn(move || {
            let result = crate::saves::SaveManager::new()
                .restore_preview(&identity, &thread_backup.id)
                .map(|preview| SaveTaskOutput::RestorePreview(thread_backup, preview));
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.save_job = Some(SaveJob { key, title, rx });
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    pub fn confirm_save_restore(&mut self, ctx: &egui::Context) {
        let Some((key, title, backup_id, expected_paths)) = self.save_restore_confirm.take() else {
            return;
        };
        let identity = match self.save_identity_for_key(&key, &title) {
            Ok(identity) => identity,
            Err(error) => {
                self.toast_error(format!("Restauration annulée : {error}"));
                return;
            }
        };
        let manager = crate::saves::SaveManager::new();
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = manager.restore(&identity, &backup_id, true, &expected_paths);
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.save_restore_rx = Some(rx);
        self.toast_info("Restauration Ludusavi en cours…");
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    pub fn preview_save_backup(
        &mut self,
        ctx: &egui::Context,
        key: &crate::providers::ProviderGameKey,
    ) {
        if self.save_job.is_some()
            || self.save_confirm_rx.is_some()
            || self.save_restore_rx.is_some()
        {
            return;
        }
        let Some(game) = self
            .lib
            .provider_library
            .games
            .iter()
            .find(|game| &game.key == key)
            .cloned()
        else {
            self.toast_error("Sauvegarde indisponible : installation fournisseur vérifiée requise");
            return;
        };
        let identity = match self.save_identity_for_key(key, &game.title) {
            Ok(identity) => identity,
            Err(error) => {
                self.toast_error(format!("Aperçu indisponible : {error}"));
                return;
            }
        };
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = crate::saves::SaveManager::new()
                .preview(&identity)
                .map(SaveTaskOutput::BackupPreview);
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.save_job = Some(SaveJob {
            key: game.key,
            title: game.title,
            rx,
        });
        self.toast_info("Aperçu Ludusavi en cours…");
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    pub fn confirm_save_backup(&mut self, ctx: &egui::Context) {
        let Some((key, title)) = self.save_confirm.take() else {
            return;
        };
        let identity = match self.save_identity_for_key(&key, &title) {
            Ok(identity) => identity,
            Err(error) => {
                self.toast_error(format!("Sauvegarde annulée : {error}"));
                return;
            }
        };
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = crate::saves::SaveManager::new().backup(
                &identity,
                crate::saves::BackupPhase::AfterExit,
                true,
            );
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.save_confirm_rx = Some(rx);
        self.toast_info("Création de la sauvegarde en cours…");
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    fn save_identity_for_quark_game(&self, game_id: &str) -> Option<crate::saves::SaveIdentity> {
        let provider = self.lib.provider_library.games.iter().find(|provider| {
            provider.quark_game_id.as_deref() == Some(game_id)
                && provider.install_state == crate::providers::InstallState::Installed
                && provider
                    .install_path
                    .as_ref()
                    .is_some_and(|path| path.is_dir())
        })?;
        Some(crate::saves::SaveIdentity {
            provider_key: provider.key.clone(),
            install_path: provider.install_path.clone(),
            ludusavi_title: provider.title.clone(),
            custom_paths: provider.custom_save_paths.clone(),
        })
    }

    fn start_background_save(
        &mut self,
        identity: crate::saves::SaveIdentity,
        phase: crate::saves::BackupPhase,
    ) {
        let (tx, rx) = channel();
        let title = identity.ludusavi_title.clone();
        std::thread::spawn(move || {
            let result = crate::saves::SaveManager::new().backup(&identity, phase, true);
            let _ = tx.send(result);
        });
        self.save_background_jobs
            .push(SaveBackgroundJob { title, phase, rx });
    }

    fn poll_save_background_jobs(&mut self, ctx: &egui::Context) {
        let mut index = 0;
        let mut pending = false;
        while index < self.save_background_jobs.len() {
            match self.save_background_jobs[index].rx.try_recv() {
                Ok(Ok(record)) => {
                    let label = match record.phase {
                        crate::saves::BackupPhase::BeforeLaunch => "avant lancement",
                        crate::saves::BackupPhase::AfterExit => "après session",
                    };
                    self.toast_info(format!(
                        "Sauvegarde Ludusavi {label} · {} · {} octets",
                        record.title, record.processed_bytes
                    ));
                    self.save_background_jobs.remove(index);
                }
                Ok(Err(error)) => {
                    let job = self.save_background_jobs.remove(index);
                    let label = match job.phase {
                        crate::saves::BackupPhase::BeforeLaunch => "avant lancement",
                        crate::saves::BackupPhase::AfterExit => "après session",
                    };
                    self.toast_error(format!(
                        "Sauvegarde Ludusavi ignorée {label} · {} : {error}",
                        job.title
                    ));
                }
                Err(TryRecvError::Disconnected) => {
                    let job = self.save_background_jobs.remove(index);
                    self.toast_error(format!("Sauvegarde Ludusavi interrompue · {}", job.title));
                }
                Err(TryRecvError::Empty) => {
                    pending = true;
                    index += 1;
                }
            }
        }
        if pending {
            ctx.request_repaint_after(Duration::from_millis(160));
        }
    }

    fn poll_save_jobs(&mut self, ctx: &egui::Context) {
        if let Some(job) = &self.save_job {
            match job.rx.try_recv() {
                Ok(Ok(SaveTaskOutput::BackupPreview(preview))) => {
                    let key = job.key.clone();
                    let title = job.title.clone();
                    self.save_preview = Some((key, title, preview));
                    self.save_job = None;
                }
                Ok(Ok(SaveTaskOutput::Backups(backups))) => {
                    let key = job.key.clone();
                    let title = job.title.clone();
                    self.save_inventory = Some((key, title, backups));
                    self.save_job = None;
                }
                Ok(Ok(SaveTaskOutput::RestorePreview(backup, preview))) => {
                    let key = job.key.clone();
                    let title = job.title.clone();
                    self.save_restore_preview = Some((key, title, backup, preview));
                    self.save_job = None;
                }
                Ok(Err(error)) => {
                    self.toast_error(format!("Aperçu des sauvegardes impossible : {error}"));
                    self.save_job = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.toast_error("Aperçu des sauvegardes interrompu");
                    self.save_job = None;
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(120)),
            }
        }
        if let Some(rx) = &self.save_restore_rx {
            match rx.try_recv() {
                Ok(Ok(())) => {
                    self.toast_info("Restauration Ludusavi terminée");
                    self.save_restore_rx = None;
                    self.save_restore_confirm = None;
                    self.save_restore_preview = None;
                    self.save_inventory = None;
                }
                Ok(Err(error)) => {
                    self.toast_error(format!("Restauration Ludusavi échouée : {error}"));
                    self.save_restore_rx = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.toast_error("Restauration Ludusavi interrompue");
                    self.save_restore_rx = None;
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(120)),
            }
        }
        if let Some(rx) = &self.save_confirm_rx {
            match rx.try_recv() {
                Ok(Ok(record)) => {
                    self.toast_info(format!(
                        "Sauvegarde Ludusavi créée · {} octets traités",
                        record.processed_bytes
                    ));
                    self.save_confirm_rx = None;
                    self.save_preview = None;
                }
                Ok(Err(error)) => {
                    self.toast_error(format!("Sauvegarde Ludusavi échouée : {error}"));
                    self.save_confirm_rx = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.toast_error("Création de sauvegarde interrompue");
                    self.save_confirm_rx = None;
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(120)),
            }
        }
    }

    pub fn save_backup_dialog(&mut self, ctx: &egui::Context) {
        let pal = self.pal;
        if let Some((key, title, preview)) = self.save_preview.as_ref() {
            let key = key.clone();
            let title = title.clone();
            let preview = preview.clone();
            let mut begin_confirmation = false;
            let mut close = false;
            egui::Modal::new(egui::Id::new("quark_save_backup_preview")).show(ctx, |ui| {
                ui.set_min_width(460.0);
                ui.label(RichText::new(format!("Aperçu de sauvegarde · {title}")).size(17.0).color(pal.text));
                ui.add_space(8.0);
                let changed = preview.changed_games.map_or_else(|| "—".to_string(), |value| value.to_string());
                let processed = preview.processed_games.map_or_else(|| "—".to_string(), |value| value.to_string());
                let bytes = preview.processed_bytes.map_or_else(|| "—".to_string(), |value| value.to_string());
                ui.label(RichText::new(format!("Jeux modifiés : {changed} · traités : {processed} · données : {bytes} octets")).size(12.0).color(pal.text_dim));
                if !preview.errors.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Ludusavi a signalé une erreur; aucune sauvegarde ne sera déclarée réussie.").size(12.0).color(pal.danger));
                }
                if preview.files.iter().any(|file| file.failed) {
                    ui.label(RichText::new("Certains fichiers ont échoué; vérifie les détails via Ludusavi.").size(11.5).color(pal.danger));
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let can_backup = preview.errors.is_empty() && !preview.files.iter().any(|file| file.failed);
                    if ui.add_enabled(can_backup, egui::Button::new("Continuer vers la confirmation")).clicked() { begin_confirmation = true; }
                    if ui.button("Fermer").clicked() { close = true; }
                });
            });
            if begin_confirmation {
                self.save_confirm = Some((key, title));
                self.save_preview = None;
            } else if close {
                self.save_preview = None;
            }
        }

        let Some((_, confirm_title)) = self.save_confirm.as_ref() else {
            return;
        };
        let confirm_title = confirm_title.clone();
        let mut confirm = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("quark_save_backup_confirmation")).show(ctx, |ui| {
            ui.set_min_width(440.0);
            ui.label(RichText::new("Confirmer la sauvegarde locale ?").size(17.0).color(pal.text));
            ui.add_space(8.0);
            ui.label(RichText::new(format!("Ludusavi va écrire les données de {confirm_title} dans le coffre local de cette installation. Les sauvegardes sont limitées à 10 versions complètes sans différentielles; les anciennes sauvegardes au-delà de la limite peuvent être supprimées par Ludusavi.")).size(12.0).color(pal.text_dim));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Créer la sauvegarde").clicked() { confirm = true; }
                if ui.button("Annuler").clicked() { cancel = true; }
            });
        });
        if confirm {
            self.confirm_save_backup(ctx);
        } else if cancel {
            self.save_confirm = None;
        }

        if let Some((key, title, backups)) = self.save_inventory.clone() {
            let mut close = false;
            let mut selected = None;
            egui::Modal::new(egui::Id::new("quark_save_inventory")).show(ctx, |ui| {
                ui.set_min_width(500.0);
                ui.label(
                    RichText::new(format!("Sauvegardes locales · {title}"))
                        .size(17.0)
                        .color(pal.text),
                );
                ui.add_space(8.0);
                if backups.is_empty() {
                    ui.label(
                        RichText::new("Aucune sauvegarde dans le coffre de cette installation.")
                            .size(12.0)
                            .color(pal.text_dim),
                    );
                }
                for backup in &backups {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&backup.when).size(12.0).color(pal.text));
                        if backup.locked {
                            ui.label(RichText::new("Protégée").size(10.0).color(pal.accent));
                        }
                        if ui.small_button("Aperçu restauration…").clicked() {
                            selected = Some(backup.clone());
                        }
                    });
                }
                if ui.button("Fermer").clicked() {
                    close = true;
                }
            });
            if let Some(backup) = selected {
                self.preview_save_restore(ctx, &backup);
            } else if close {
                self.save_inventory = None;
            }
            let _ = key;
        }

        if let Some((key, title, backup, preview)) = self.save_restore_preview.clone() {
            let mut confirm = false;
            let mut close = false;
            egui::Modal::new(egui::Id::new("quark_restore_preview")).show(ctx, |ui| {
                ui.set_min_width(560.0);
                ui.label(
                    RichText::new(format!("Aperçu de restauration · {title}"))
                        .size(17.0)
                        .color(pal.text),
                );
                ui.label(
                    RichText::new(format!(
                        "Snapshot {} · {} fichier(s) concernés",
                        backup.when,
                        preview.files.len()
                    ))
                    .size(12.0)
                    .color(pal.text_dim),
                );
                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        for file in &preview.files {
                            ui.label(RichText::new(&file.path).size(10.5).color(pal.text_dim));
                        }
                    });
                if !preview.errors.is_empty() || preview.files.iter().any(|file| file.failed) {
                    ui.label(
                        RichText::new("L'aperçu contient des erreurs; restauration désactivée.")
                            .size(11.5)
                            .color(pal.danger),
                    );
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let safe =
                        preview.errors.is_empty() && !preview.files.iter().any(|file| file.failed);
                    if ui
                        .add_enabled(
                            safe,
                            egui::Button::new("Restaurer et remplacer les fichiers concernés…"),
                        )
                        .clicked()
                    {
                        confirm = true;
                    }
                    if ui.button("Fermer").clicked() {
                        close = true;
                    }
                });
            });
            if confirm {
                let expected_paths = preview.files.iter().map(|file| file.path.clone()).collect();
                self.save_restore_confirm = Some((key, title, backup.id, expected_paths));
            }
            if confirm || close {
                self.save_restore_preview = None;
            }
        }

        if let Some((_, title, backup_id, _)) = self.save_restore_confirm.clone() {
            let mut confirm = false;
            let mut cancel = false;
            egui::Modal::new(egui::Id::new("quark_restore_confirmation")).show(ctx, |ui| {
                ui.set_min_width(440.0);
                ui.label(RichText::new("Confirmer le remplacement des sauvegardes ?").size(17.0).color(pal.danger));
                ui.label(RichText::new(format!("Les fichiers de {title} seront restaurés depuis le snapshot {backup_id}. Les fichiers présents aux destinations pourront être écrasés.")).size(12.0).color(pal.text_dim));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Confirmer la restauration").clicked() { confirm = true; }
                    if ui.button("Annuler").clicked() { cancel = true; }
                });
            });
            if confirm {
                self.confirm_save_restore(ctx);
            } else if cancel {
                self.save_restore_confirm = None;
            }
        }
    }

    pub fn open_store_item(&mut self, item: &crate::steam_store::StoreItem) {
        // URL de fiche générée localement à partir de l'AppID; aucun lien reçu
        // dans la réponse storefront n'est exécuté.
        let url = format!("https://store.steampowered.com/app/{}", item.app_id);
        if let Err(error) = crate::news::open_in_browser(&url) {
            self.toast_error(format!("Ouverture de Steam impossible : {error}"));
        }
    }

    pub fn start_managed_tool(
        &mut self,
        ctx: &egui::Context,
        tool: crate::providers::tools::ManagedToolId,
    ) {
        let index = tool.index();
        if self.tool_jobs[index].is_some() {
            return;
        }
        self.managed_tools[index] = Some("Recherche d'une release officielle…".to_string());
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let client = artwork::http();
                let release = crate::providers::tools::latest_release(&client, tool)?;
                let tag = release.tag.clone();
                let asset = release.asset(tool)?.clone();
                let path = crate::providers::tools::install_verified(
                    &client,
                    tool,
                    &asset,
                    &crate::config::data_dir().join("tools"),
                )?;
                Ok(format!("{tag} · {}", path.display()))
            })();
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.tool_jobs[index] = Some(rx);
        self.managed_tools[index] = Some("Téléchargement et vérification…".to_string());
        ctx.request_repaint_after(Duration::from_millis(150));
    }

    fn poll_managed_tools(&mut self, ctx: &egui::Context) {
        let mut pending = false;
        for index in 0..self.tool_jobs.len() {
            let result = self.tool_jobs[index].as_ref().map(|rx| rx.try_recv());
            match result {
                Some(Ok(Ok(message))) => {
                    self.tool_jobs[index] = None;
                    self.managed_tools[index] = Some(message);
                }
                Some(Ok(Err(error))) => {
                    self.tool_jobs[index] = None;
                    self.managed_tools[index] = Some(format!("Échec : {error}"));
                }
                Some(Err(TryRecvError::Disconnected)) => {
                    self.tool_jobs[index] = None;
                    self.managed_tools[index] = Some("Échec : opération interrompue".to_string());
                }
                Some(Err(TryRecvError::Empty)) => pending = true,
                None => {}
            }
        }
        if pending {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
    }

    /// Ouvre le formulaire de connexion d'une boutique (Epic ou GOG).
    pub fn open_provider_login(&mut self, provider: crate::providers::Provider) {
        self.provider_login = Some(ProviderLogin {
            provider,
            code: String::new(),
            status: None,
            error: None,
            rx: None,
        });
    }

    pub fn cancel_provider_login(&mut self) {
        self.provider_login = None;
    }

    /// Échange le code collé contre une session, via Legendary ou gogdl.
    pub fn submit_provider_login(&mut self, ctx: &egui::Context) {
        let Some(login) = self.provider_login.as_mut() else {
            return;
        };
        if login.rx.is_some() {
            return;
        }
        let code = login.code.trim().to_string();
        if code.is_empty() {
            login.error = Some("Colle le code d'autorisation avant de valider.".to_string());
            return;
        }
        if code.len() > 4096 || code.chars().any(char::is_whitespace) {
            login.error = Some("Le code d'autorisation ne doit pas contenir d'espace.".to_string());
            return;
        }
        let provider = login.provider;
        login.error = None;
        login.status = Some("Échange du code auprès de la boutique…".to_string());
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = match provider {
                crate::providers::Provider::Epic => {
                    crate::providers::legendary::LegendaryAdapter::new()
                        .login_with_code(&code)
                        .map(|()| "Compte Epic connecté (Legendary)".to_string())
                }
                crate::providers::Provider::Gog => crate::providers::gog::GogAdapter::new(
                    crate::providers::gog::auth_config_path(),
                )
                .login_with_code(&code)
                .map(|()| "Compte GOG connecté (gogdl)".to_string()),
                crate::providers::Provider::Steam => {
                    Err("Steam n'utilise pas cette connexion par code".to_string())
                }
            };
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        if let Some(login) = self.provider_login.as_mut() {
            login.rx = Some(rx);
        }
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    fn poll_provider_login(&mut self, ctx: &egui::Context) {
        let Some(login) = self.provider_login.as_mut() else {
            return;
        };
        let Some(result) = login.rx.as_ref().map(Receiver::try_recv) else {
            return;
        };
        match result {
            Ok(Ok(message)) => {
                login.rx = None;
                login.code.clear();
                login.status = Some(message.clone());
                login.error = None;
                let provider = login.provider;
                self.toast_info(message);
                if provider == crate::providers::Provider::Epic {
                    // Une session Epic fraîche permet de relire la bibliothèque.
                    self.epic_sync_error = None;
                }
            }
            Ok(Err(error)) => {
                login.rx = None;
                login.status = None;
                login.error = Some(error);
            }
            Err(TryRecvError::Disconnected) => {
                login.rx = None;
                login.status = None;
                login.error = Some("connexion interrompue".to_string());
            }
            Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(120)),
        }
    }

    pub fn request_epic_install(
        &mut self,
        ctx: &egui::Context,
        key: &crate::providers::ProviderGameKey,
    ) {
        if self.epic_install_rx.is_some() || self.epic_installing.is_some() {
            return;
        }
        let Some(game) = self
            .lib
            .provider_library
            .games
            .iter()
            .find(|game| &game.key == key)
            .cloned()
        else {
            return;
        };
        if key.provider != crate::providers::Provider::Epic || !game.owned {
            self.toast_error("Installation Epic indisponible pour ce jeu");
            return;
        }
        self.epic_install_pending = Some(key.clone());
        self.pick_directory(ctx, crate::config::home_dir(), PickerTarget::EpicInstall);
    }

    fn start_epic_install(&mut self, ctx: &egui::Context, destination: PathBuf) {
        let Some(key) = self.epic_install_pending.take() else {
            return;
        };
        if !destination.is_dir() {
            self.toast_error("Dossier de destination Epic invalide");
            return;
        }
        let Some(game) = self
            .lib
            .provider_library
            .games
            .iter()
            .find(|game| game.key == key)
            .cloned()
        else {
            return;
        };
        self.epic_installing = Some(key.clone());
        if let Some(record) = self
            .lib
            .provider_library
            .games
            .iter_mut()
            .find(|game| game.key == key)
        {
            record.install_state = crate::providers::InstallState::Installing;
        }
        self.touch();
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let adapter = crate::providers::legendary::LegendaryAdapter::new();
            let result = (|| {
                std::fs::create_dir_all(&destination).map_err(|_| {
                    "impossible de préparer la destination Epic choisie".to_string()
                })?;
                let mut operation = crate::providers::operations::InstallOperation::new(
                    key.clone(),
                    destination.clone(),
                );
                operation.start();
                let mut output = adapter.run_install(
                    crate::providers::legendary::LegendaryAdapter::install_args(
                        &key.product_id,
                        &destination,
                    ),
                    None,
                )?;
                if output.exit_code != Some(0) {
                    return Err("Legendary a échoué pendant l'installation".to_string());
                }
                output = adapter.run(
                    crate::providers::legendary::LegendaryAdapter::installed_args(),
                    None,
                )?;
                let installed = adapter.installed_json(&output, crate::config::now_ts())?;
                let installed = installed
                    .into_iter()
                    .find(|game| game.key == key)
                    .ok_or_else(|| {
                        "installation terminée sans manifeste local correspondant".to_string()
                    })?;
                let detected_path = installed
                    .install_path
                    .clone()
                    .ok_or_else(|| "installation sans chemin local".to_string())?;
                operation.complete_verified(detected_path.clone())?;
                let mut verified = installed;
                verified.install_path = Some(
                    detected_path
                        .canonicalize()
                        .map_err(|_| "chemin d'installation Epic introuvable".to_string())?,
                );
                Ok(verified)
            })();
            let _ = tx.send((key, result));
            repaint.request_repaint();
        });
        self.epic_install_rx = Some(rx);
        self.toast_info(format!(
            "Installation Epic de {} lancée par Legendary",
            game.title
        ));
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    fn poll_epic_install(&mut self, ctx: &egui::Context) {
        let Some(result) = self.epic_install_rx.as_ref().map(Receiver::try_recv) else {
            return;
        };
        match result {
            Ok((key, Ok(installed))) => {
                self.epic_install_rx = None;
                self.epic_installing = None;
                if let Some(record) = self
                    .lib
                    .provider_library
                    .games
                    .iter_mut()
                    .find(|game| game.key == key)
                {
                    let group = crate::providers::legendary::merge_epic(
                        std::slice::from_ref(record),
                        std::slice::from_ref(&installed),
                    );
                    if let Some(updated) = group.into_iter().find(|game| game.key == key) {
                        *record = updated;
                    }
                }
                self.touch();
                self.toast_info(format!(
                    "{} installé et détecté localement",
                    installed.title
                ));
            }
            Ok((key, Err(error))) => {
                self.epic_install_rx = None;
                self.epic_installing = None;
                if let Some(record) = self
                    .lib
                    .provider_library
                    .games
                    .iter_mut()
                    .find(|game| game.key == key)
                {
                    if record
                        .install_path
                        .as_ref()
                        .is_some_and(|path| path.is_dir())
                    {
                        record.install_state = crate::providers::InstallState::Installed;
                    } else {
                        record.install_state = crate::providers::InstallState::Failed;
                        record.install_path = None;
                    }
                }
                self.touch();
                self.toast_error(format!("Installation Epic non confirmée : {error}"));
            }
            Err(TryRecvError::Disconnected) => {
                self.epic_install_rx = None;
                if let Some(key) = self.epic_installing.take() {
                    if let Some(record) = self
                        .lib
                        .provider_library
                        .games
                        .iter_mut()
                        .find(|game| game.key == key)
                    {
                        record.install_state = if record
                            .install_path
                            .as_ref()
                            .is_some_and(|path| path.is_dir())
                        {
                            crate::providers::InstallState::Installed
                        } else {
                            crate::providers::InstallState::Unknown
                        };
                    }
                    self.touch();
                }
                self.toast_error("Installation Epic interrompue; aucun succès n'a été enregistré");
            }
            Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(120)),
        }
    }

    pub fn start_epic_sync(&mut self, ctx: &egui::Context) {
        if self.epic_sync_loading
            || !crate::providers::tools::ManagedToolId::Legendary
                .installed_path()
                .is_file()
        {
            return;
        }
        self.epic_sync_loading = true;
        self.epic_sync_error = None;
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = crate::providers::legendary::LegendaryAdapter::new()
                .sync_owned(crate::config::now_ts(), None);
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.epic_sync_rx = Some(rx);
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    fn start_epic_install_sync(
        &mut self,
        ctx: &egui::Context,
        owned: Vec<crate::providers::ProviderGame>,
    ) {
        if self.epic_install_sync_rx.is_some() {
            return;
        }
        self.epic_owned_fallback = Some(owned.clone());
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let adapter = crate::providers::legendary::LegendaryAdapter::new();
            let result = adapter
                .sync_installed(crate::config::now_ts(), None)
                .map(|installed| (owned, installed));
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.epic_install_sync_rx = Some(rx);
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    fn poll_epic_sync(&mut self, ctx: &egui::Context) {
        if let Some(result) = self.epic_install_sync_rx.as_ref().map(Receiver::try_recv) {
            match result {
                Ok(Ok((owned, installed))) => {
                    self.epic_install_sync_rx = None;
                    self.epic_owned_fallback = None;
                    self.lib.provider_library.games =
                        crate::providers::legendary::merge_epic_with_verified_installs(
                            &self.lib.provider_library.games,
                            &owned,
                            &installed,
                        );
                    // La détection locale a abouti : l'indicateur de chargement
                    // doit retomber, sinon l'écran reste bloqué sur « en cours ».
                    self.epic_sync_error = None;
                    self.epic_sync_loading = false;
                    self.touch();
                }
                Ok(Err(error)) => {
                    self.epic_install_sync_rx = None;
                    self.epic_sync_error = Some(format!(
                        "détection locale Epic indisponible; bibliothèque connue conservée : {error}"
                    ));
                    if let Some(owned) = self.epic_owned_fallback.take() {
                        self.lib.provider_library.games = crate::providers::legendary::merge_epic(
                            &self.lib.provider_library.games,
                            &owned,
                        );
                        self.touch();
                    }
                    self.epic_sync_loading = false;
                }
                Err(TryRecvError::Disconnected) => {
                    self.epic_install_sync_rx = None;
                    self.epic_sync_error =
                        Some("détection des installations Epic interrompue".into());
                    if let Some(owned) = self.epic_owned_fallback.take() {
                        self.lib.provider_library.games = crate::providers::legendary::merge_epic(
                            &self.lib.provider_library.games,
                            &owned,
                        );
                        self.touch();
                    }
                    self.epic_sync_loading = false;
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(120)),
            }
        }
        let Some(result) = self.epic_sync_rx.as_ref().map(Receiver::try_recv) else {
            return;
        };
        match result {
            Ok(Ok(games)) => {
                self.epic_sync_rx = None;
                self.epic_sync_loading = false;
                if games.is_empty() {
                    self.epic_sync_error = Some("bibliothèque Epic vide ou compte Legendary indisponible; données précédentes conservées".into());
                } else {
                    self.epic_sync_loading = true;
                    self.start_epic_install_sync(ctx, games);
                }
            }
            Ok(Err(error)) => {
                self.epic_sync_rx = None;
                self.epic_sync_loading = false;
                self.epic_sync_error = Some(error);
            }
            Err(TryRecvError::Disconnected) => {
                self.epic_sync_rx = None;
                self.epic_sync_loading = false;
                self.epic_sync_error =
                    Some("synchronisation Epic interrompue; données précédentes conservées".into());
            }
            Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(120)),
        }
    }

    /// Lance les synchronisations des boutiques connectées, une fois, peu
    /// après l'affichage : l'utilisateur n'a plus à cliquer après chaque
    /// démarrage, et chaque boutique échoue indépendamment des autres.
    pub fn startup_sync(&mut self, ctx: &egui::Context) {
        // Vingt images laissent la fenêtre apparaître avant de lancer des
        // lectures disque et réseau ; sans cela le premier rendu saccade.
        if self.startup_sync_done || self.frames_alive < 20 {
            return;
        }
        self.startup_sync_done = true;
        self.start_steam_sync(ctx);
        if crate::providers::tools::ManagedToolId::Legendary
            .installed_path()
            .is_file()
            && crate::providers::legendary::LegendaryAdapter::is_logged_in()
        {
            self.start_epic_sync(ctx);
        }
        if crate::providers::gog::GogAdapter::is_logged_in() {
            self.start_gog_sync(ctx);
        }
    }

    pub fn start_gog_sync(&mut self, ctx: &egui::Context) {
        if self.gog_sync_loading || self.gog_sync_rx.is_some() {
            return;
        }
        self.gog_sync_loading = true;
        self.gog_sync_error = None;
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let adapter =
                crate::providers::gog::GogAdapter::new(crate::providers::gog::auth_config_path());
            let result = adapter.sync_library(crate::config::now_ts());
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.gog_sync_rx = Some(rx);
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    fn poll_gog_sync(&mut self, ctx: &egui::Context) {
        let Some(result) = self.gog_sync_rx.as_ref().map(Receiver::try_recv) else {
            return;
        };
        match result {
            Ok(Ok(games)) => {
                self.gog_sync_rx = None;
                self.gog_sync_loading = false;
                self.gog_sync_error = None;
                if !games.is_empty() {
                    self.lib.provider_library.games =
                        crate::providers::gog::merge_gog(&self.lib.provider_library.games, &games);
                    self.touch();
                }
            }
            Ok(Err(error)) => {
                self.gog_sync_rx = None;
                self.gog_sync_loading = false;
                self.gog_sync_error = Some(error);
            }
            Err(TryRecvError::Disconnected) => {
                self.gog_sync_rx = None;
                self.gog_sync_loading = false;
                self.gog_sync_error =
                    Some("synchronisation GOG interrompue; données précédentes conservées".into());
            }
            Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(120)),
        }
    }

    pub fn start_steam_sync(&mut self, ctx: &egui::Context) {
        if self.steam_sync_loading {
            return;
        }
        self.steam_sync_loading = true;
        self.steam_sync_error = None;
        let key = self.credentials.steam_web_api_key.clone();
        let steam_id = self.credentials.steam_id.clone();
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let local_result = steamlib::installed();
            let (local, local_error) = match local_result {
                Ok(games) => (games, None),
                Err(error) => (Vec::new(), Some(error)),
            };
            let owned = match (key, steam_id) {
                (Some(key), Some(id))
                    if !key.trim().is_empty() && crate::providers::steam::valid_steam_id(&id) =>
                {
                    Some(crate::providers::steam::owned(&artwork::http(), &id, &key))
                }
                _ => None,
            };
            let _ = tx.send(SteamSyncOutcome {
                local,
                local_error,
                owned,
            });
            repaint.request_repaint();
        });
        self.steam_sync_rx = Some(rx);
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    fn poll_steam_sync(&mut self, ctx: &egui::Context) {
        let Some(result) = self.steam_sync_rx.as_ref().map(Receiver::try_recv) else {
            return;
        };
        match result {
            Ok(outcome) => {
                self.steam_sync_rx = None;
                self.steam_sync_loading = false;
                let local =
                    crate::providers::steam::installed_games(&outcome.local, &self.lib.games);
                let owned = match outcome.owned {
                    Some(Ok(records)) => Some(records),
                    Some(Err(error)) => {
                        self.steam_sync_error = Some(format!(
                            "API possédée indisponible : {error} · données précédentes conservées"
                        ));
                        None
                    }
                    None => None,
                };
                self.lib.provider_library.games = crate::providers::steam::merge_sync(
                    &self.lib.provider_library.games,
                    &local,
                    owned.as_deref(),
                    &self.lib.games,
                );
                self.touch();
                if let Some(error) = outcome.local_error {
                    if self.steam_sync_error.is_none() {
                        self.steam_sync_error = Some(error);
                    }
                }
            }
            Err(TryRecvError::Disconnected) => {
                self.steam_sync_rx = None;
                self.steam_sync_loading = false;
                self.steam_sync_error = Some("synchronisation Steam interrompue".to_string());
            }
            Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(120)),
        }
    }

    pub fn save_steam_api_key(&mut self) {
        let key = self.steam_key_draft.trim().to_string();
        let steam_id = self.steam_id_draft.trim().to_string();
        if key.is_empty() && steam_id.is_empty() {
            return;
        }
        if !steam_id.is_empty() && !crate::providers::steam::valid_steam_id(&steam_id) {
            self.toast_error(
                "SteamID64 invalide : il doit contenir 17 chiffres commençant par 7656119",
            );
            return;
        }
        let mut updated = self.credentials.clone();
        if !key.is_empty() {
            updated.steam_web_api_key = Some(key);
        }
        if !steam_id.is_empty() {
            updated.steam_id = Some(steam_id);
        }
        match crate::providers::credentials::save_atomic(
            &crate::config::credentials_path(),
            &updated,
        ) {
            Ok(()) => {
                self.credentials = updated;
                self.steam_key_draft.clear();
                self.steam_id_draft.clear();
                self.credentials_error = None;
                self.toast_info(
                    "Identifiants Steam enregistrés dans le coffre local (non chiffré)",
                );
            }
            Err(error) => self.toast_error(format!(
                "Enregistrement des identifiants impossible : {error}"
            )),
        }
    }

    pub fn import_steam(&mut self, ctx: &egui::Context) {
        let Some(import) = self.steam_import.as_ref() else {
            return;
        };
        let picked: Vec<(u32, String, Option<PathBuf>)> = import
            .games
            .iter()
            .filter(|candidate| candidate.include && !candidate.owned)
            .map(|candidate| {
                (
                    candidate.app_id,
                    candidate.name.clone(),
                    candidate.exe.clone(),
                )
            })
            .collect();
        if picked.is_empty() {
            self.toast_error("Aucun jeu à importer");
            return;
        }
        let settings = self.lib.settings.clone();
        let mut added = 0usize;
        let mut missing_exe = 0usize;
        let mut hits: Vec<(String, SearchHit)> = Vec::new();
        for (app_id, name, exe) in picked {
            if exe.is_none() {
                missing_exe += 1;
            }
            let mut game = Game::new(
                name,
                exe.as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
            );
            game.steam_app_id = Some(app_id);
            // Mêmes préremplissages que l'ajout manuel.
            game.args = settings.default_args.trim().to_string();
            game.prefix = settings
                .default_prefix
                .clone()
                .filter(|prefix| !prefix.trim().is_empty());
            game.anticheat = settings.default_anticheat;
            game.launch = settings.default_launch.clone();
            game.proton_log = settings.default_proton_log;
            game.umu = settings.default_umu;
            game.env = settings.default_env.clone();
            let id = game.id.clone();
            hits.push((id.clone(), SearchHit::from_game(&game)));
            self.lib.games.push(game);
            self.queue_metadata(ctx, &id);
            added += 1;
        }
        for (id, hit) in hits {
            self.start_asset_job(ctx, &id, hit);
        }
        self.steam_import = None;
        self.touch();
        self.toast_info(format!(
            "{added} jeu(x) importé(s) de Steam{}",
            if missing_exe > 0 {
                format!(" · {missing_exe} sans exécutable détecté")
            } else {
                String::new()
            }
        ));
    }

    // -----------------------------------------------------------------------
    // Métadonnées Steam (développeur, genres, date, description)
    // -----------------------------------------------------------------------

    /// Demande la fiche Steam d'un jeu (une requête à la fois).
    pub fn queue_metadata(&mut self, ctx: &egui::Context, game_id: &str) {
        let Some(app_id) = self.lib.get(game_id).and_then(|game| game.steam_app_id) else {
            return;
        };
        if self.meta_job.is_some() {
            self.meta_queue.push_back((game_id.to_string(), app_id));
            return;
        }
        self.start_metadata(ctx, game_id.to_string(), app_id);
    }

    fn start_metadata(&mut self, ctx: &egui::Context, game_id: String, app_id: u32) {
        let (tx, rx) = channel();
        let settings = self.lib.settings.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let client = artwork::http();
            let result = artwork::metadata(&client, app_id, &settings);
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.meta_job = Some(MetadataJob { game_id, rx });
        ctx.request_repaint_after(Duration::from_millis(200));
    }

    fn poll_metadata(&mut self, ctx: &egui::Context) {
        let received = self
            .meta_job
            .as_ref()
            .map(|job| (job.game_id.clone(), job.rx.try_recv()));
        if let Some((id, result)) = received {
            match result {
                Ok(Ok(meta)) => {
                    if let Some(game) = self.lib.get_mut(&id) {
                        // Ne jamais écraser une valeur corrigée à la main.
                        if game.developer.is_empty() {
                            game.developer = meta.developer;
                        }
                        if game.genres.is_empty() {
                            game.genres = meta.genres;
                        }
                        if game.released.is_empty() {
                            game.released = meta.released;
                        }
                        if game.description.is_empty() {
                            game.description = meta.description;
                        }
                    }
                    self.meta_job = None;
                    self.touch();
                }
                Ok(Err(err)) => {
                    self.meta_job = None;
                    self.toast_error(format!("Fiche Steam : {err}"));
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.meta_job = None;
                }
            }
        }
        if self.meta_job.is_none() {
            if let Some((game_id, app_id)) = self.meta_queue.pop_front() {
                self.start_metadata(ctx, game_id, app_id);
            }
        }
    }

    /// Relance la fiche Steam d'un jeu (bouton dans ses paramètres).
    pub fn refresh_metadata(&mut self, ctx: &egui::Context, game_id: &str) {
        if let Some(game) = self.lib.get_mut(game_id) {
            game.developer.clear();
            game.genres.clear();
            game.released.clear();
            game.description.clear();
        }
        self.queue_metadata(ctx, game_id);
    }

    // -----------------------------------------------------------------------
    // Espace disque
    // -----------------------------------------------------------------------

    pub fn start_disk_scan(&mut self, ctx: &egui::Context) {
        if self.disk_job.is_some() {
            return;
        }
        self.disk_report = None;
        self.disk_job = Some(disk::spawn(self.lib.clone()));
        ctx.request_repaint_after(Duration::from_millis(200));
    }

    fn poll_disk(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.disk_job.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(report) => {
                self.disk_job = None;
                self.disk_report = Some(report);
            }
            Err(TryRecvError::Empty) => {
                // Mesure en cours : la page garde son indicateur animé.
                ctx.request_repaint_after(Duration::from_millis(200));
            }
            Err(TryRecvError::Disconnected) => self.disk_job = None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::removal_targets;

    #[test]
    fn la_suppression_suit_la_selection_multiple() {
        let marked = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        // Clic droit sur un jeu sélectionné : tout le lot part.
        assert_eq!(removal_targets(&marked, "b"), marked);
        // Clic droit sur un jeu hors sélection : lui seul.
        assert_eq!(removal_targets(&marked, "z"), vec!["z".to_string()]);
        // Un seul jeu marqué, ou aucun : comportement d'origine.
        assert_eq!(
            removal_targets(&["a".to_string()], "a"),
            vec!["a".to_string()]
        );
        assert_eq!(removal_targets(&[], "a"), vec!["a".to_string()]);
    }
}
