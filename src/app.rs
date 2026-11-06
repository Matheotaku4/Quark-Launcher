//! État de l'application et assemblage de l'interface.

use crate::artwork::{self, AssetPlan, GameMetadata, SearchHit, SearchOutcome};
use crate::config::{
    Artwork, AssetKind, DllOverride, Game, Library, ProtonMode, StartPage, ThemeMode, ViewMode,
    human_playtime,
};
use crate::disk::{self, DiskReport};
use crate::i18n;
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
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Store,
    Library,
    MyGames,
}

/// Traduit la page d'ouverture en onglet et en état d'accueil. En interne,
/// l'accueil reste une page de la bibliothèque (`Tab::Library` + `home`) ; la
/// barre du haut lui donne son propre onglet.
fn startup_route(page: StartPage) -> (Tab, bool) {
    match page {
        StartPage::Home => (Tab::Library, true),
        StartPage::Library => (Tab::Library, false),
        StartPage::Store => (Tab::Store, false),
        StartPage::MyGames => (Tab::MyGames, false),
    }
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

/// État d'une source dans la fenêtre « chercher ce jeu ailleurs ».
pub enum SiteSearchState {
    Pending,
    Ready(Vec<crate::game_search::SiteHit>),
    Failed(String),
}

/// Recherche d'un jeu de la boutique sur les cinq sites de téléchargement.
pub struct SiteSearchJob {
    pub title: String,
    pub query: String,
    pub steam_url: String,
    pub states: Vec<(crate::game_search::SearchSite, SiteSearchState)>,
    pub pending: usize,
    pub rx: Receiver<(
        crate::game_search::SearchSite,
        Result<Vec<crate::game_search::SiteHit>, String>,
    )>,
}

/// Choisit une bannière pour le fond aléatoire. Le tirage est fait une seule
/// fois par session : changer de page ne change pas l'image sous les yeux.
pub(crate) fn random_hero_from(lib: &Library) -> Option<String> {
    let candidates: Vec<&Game> = lib
        .games
        .iter()
        .filter(|game| {
            game.artwork
                .get(AssetKind::Hero)
                .is_some_and(|path| path.exists())
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let index = (nanos ^ (std::process::id() as u128)) as usize % candidates.len();
    Some(candidates[index].id.clone())
}

/// Vrai si la fiche n'a pas encore sa jaquette ou sa bannière. Un chemin
/// enregistré mais disparu du disque compte comme manquant.
fn needs_provider_art(artwork: &Artwork) -> bool {
    [AssetKind::Cover, AssetKind::Hero]
        .iter()
        .any(|kind| artwork.get(*kind).is_none_or(|path| !path.exists()))
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
    /// Archive téléchargée et vérifiée ; le binaire va être remplacé.
    Installing(String),
    Failed(String),
}

/// Réponse d'un contrôle de mise à jour ou d'un téléchargement.
pub enum UpdateMessage {
    Checked(Result<Release, String>),
    Staged(String, Result<update::StagedUpdate, String>),
}

/// Ce qu'un téléchargement d'artworks doit récupérer.
pub enum AssetSource {
    /// Recherche automatique des meilleurs artworks.
    Auto(SearchHit),
    /// Choix manuels de l'utilisateur, les autres propositions restant en secours.
    Chosen(AssetPlan, String),
    /// Jaquette PS2 du dépôt ps2-covers, désignée par le numéro de série.
    Ps2(String),
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
    /// Image disque PS2 choisie depuis « Ajouter un jeu ».
    AddPs2Iso,
    AddPrefix,
    GameExe,
    GamePrefix,
    /// Dossier analysé pour proposer des DLL à surcharger.
    GameDllSource,
    DefaultPrefix,
    /// Dossier parent par défaut des installations Epic/GOG.
    DefaultInstallDir,
    /// Dossier choisi pour l'installation en cours, sans changer le réglage.
    InstallDestination,
    ScanFolder,
    /// Binaire PCSX2 choisi à la main, quand il n'est pas dans le PATH.
    Pcsx2Path,
    /// Image choisie sur le disque pour un artwork du jeu personnalisé.
    CustomArtwork(AssetKind),
    SaveFolder(crate::providers::ProviderGameKey),
}

/// Trois téléchargements simultanés au plus : assez pour lancer une file de
/// jeux, sans saturer la connexion ou le disque d'une machine modeste.
pub const MAX_CONCURRENT_INSTALLS: usize = 3;

/// Une installation Epic ou GOG indépendante, suivie jusqu'à son résultat.
pub struct ProviderInstallJob {
    pub key: crate::providers::ProviderGameKey,
    pub progress: Arc<Mutex<crate::providers::runner::InstallProgress>>,
    pub cancel: Arc<AtomicBool>,
    pub rx: Receiver<(
        crate::providers::ProviderGameKey,
        Result<crate::providers::ProviderGame, String>,
    )>,
}

/// Téléchargement de l'overlay Epic Online Services (EOS), puis activation
/// dans le préfixe du jeu. L'overlay est global à Epic mais activé par préfixe.
pub struct EosOverlayJob {
    pub progress: Arc<Mutex<crate::providers::runner::InstallProgress>>,
    pub cancel: Arc<AtomicBool>,
    pub rx: Receiver<Result<(PathBuf, Option<PathBuf>), String>>,
}

/// Confirmation d'installation d'un jeu Epic/GOG : taille annoncée et dossier
/// choisi avant de lancer le téléchargement. La taille est lue en arrière-plan
/// par `legendary info` / `gogdl info`, jamais inventée.
pub struct InstallRequest {
    pub key: crate::providers::ProviderGameKey,
    pub title: String,
    pub destination: PathBuf,
    pub size: Option<u64>,
    pub size_error: Option<String>,
    pub size_rx: Option<Receiver<Result<Option<u64>, String>>>,
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
    /// Overlay, captures, succès et synchronisation des sauvegardes.
    InGame,
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
    pub const ALL: [(SettingsTab, &'static str); 8] = [
        (SettingsTab::Appearance, "Apparence"),
        (SettingsTab::InGame, "En jeu"),
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
                "Thème, page d'ouverture, affichage de la bibliothèque, accueil et notifications."
            }
            SettingsTab::InGame => {
                "Overlay, captures, succès et synchronisation des sauvegardes cloud."
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

/// Onglet de la fiche d'un jeu. La fiche générale reste le point d'entrée ;
/// les DLL ont leur propre page pour ne pas allonger encore le formulaire.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GameTab {
    General,
    /// Numéro de série, jaquette et progression RetroAchievements.
    Ps2,
    DllOverrides,
}

impl GameTab {
    pub const ALL: [(GameTab, &'static str); 3] = [
        (GameTab::General, "Réglages"),
        (GameTab::Ps2, "PlayStation 2"),
        (GameTab::DllOverrides, "DLL overrides"),
    ];
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

/// Synchronisation cloud d'une installation, exécutée hors du rendu.
pub struct CloudSyncJob {
    pub key: crate::providers::ProviderGameKey,
    pub game_id: String,
    pub title: String,
    pub rx: Receiver<Result<crate::providers::CloudSaveReport, String>>,
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

/// Onglets de la fiche d'un jeu, sous la bannière.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DetailTab {
    News,
    Achievements,
    /// Sessions et heures de jeu du jeu affiché.
    History,
}

/// Succès d'un jeu de boutique, lus à la demande.
#[derive(Clone, Debug)]
pub enum AchievementsState {
    Loading,
    Ready(crate::providers::AchievementInfo),
    Failed(String),
}

/// Nombre de succès RetroAchievements obtenus, pour les badges de liste.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ps2Progress {
    pub unlocked: u32,
    pub total: u32,
}

impl Ps2Progress {
    pub(crate) fn ratio(self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.unlocked as f32 / self.total as f32).clamp(0.0, 1.0)
        }
    }
}

/// Bannière en cours de fondu : l'image du jeu précédent reste dessinée
/// par-dessus la nouvelle pendant quelques dixièmes de seconde, si bien que la
/// fiche change de jeu sans à-coup.
#[derive(Clone, Debug)]
pub struct HeroFade {
    pub game_id: String,
    pub from: Option<PathBuf>,
    pub started: Instant,
}

pub struct QuarkApp {
    pub lib: Library,
    pub pal: Palette,
    pub tab: Tab,
    pub selected: Option<String>,
    /// Nombre de colonnes de la dernière grille affichée, pour que les flèches
    /// du clavier suivent la mise en page réelle (2 à 8 colonnes).
    pub grid_columns: usize,
    pub filter: String,
    /// Palette de commandes (Ctrl+K) : ouverte, requête et ligne sélectionnée.
    pub command_palette_open: bool,
    pub command_query: String,
    pub command_selected: usize,
    pub command_last_query: String,
    /// Fenêtre « Vues enregistrées » : nom en cours de saisie et erreur.
    pub saved_views_open: bool,
    pub saved_view_name: String,
    pub saved_view_error: Option<String>,
    pub store_query: String,
    /// Recherche multi-sites ouverte depuis une carte de la boutique.
    pub site_search: Option<SiteSearchJob>,
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
    /// Parcours de bienvenue du premier lancement.
    pub onboarding_open: bool,
    pub onboarding_step: usize,
    /// Résumé de la version courante, montré une fois après une mise à jour.
    pub whats_new_open: bool,
    /// Hero tiré au hasard pour le fond de l'accueil, fixé pour cette session.
    pub random_hero_game: Option<String>,
    pub editing: Option<Game>,
    pub game_tab: GameTab,
    /// Overrides de DLL du brouillon ouvert, repris depuis `WINEDLLOVERRIDES`.
    pub game_dll_rows: Vec<DllOverride>,
    pub game_dll_for: Option<String>,
    /// Recherche de liaison Steam / SteamGridDB ouverte dans les paramètres
    /// d'un jeu qui n'a pas encore de source d'images.
    pub game_link_query: String,
    pub game_link_for: Option<String>,
    pub game_link_loading: bool,
    pub game_link_results: Vec<SearchHit>,
    pub game_link_warnings: Vec<String>,
    pub game_link_rx: Option<Receiver<SearchOutcome>>,
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
    /// Dernier point de contrôle des sessions en cours (voir
    /// `checkpoint_running_sessions`).
    session_checkpoint_at: Instant,
    /// Prochaine tentative après un échec d'écriture (backoff).
    save_retry_at: Instant,
    save_backoff: Duration,
    /// Erreur de configuration persistante, affichée sous la barre du haut.
    pub config_error: Option<String>,
    /// Fichier de succès de session illisible : sauvegarde refusée.
    pub session_error: Option<String>,
    /// Rendu hors écran : aucune sauvegarde réelle n'est effectuée.
    pub snapshot_mode: bool,
    pub applied_theme: Option<ThemeMode>,
    /// Panneau d'informations du jeu (masqué par défaut, bouton ⓘ).
    pub show_info: bool,
    /// Artworks à télécharger après le job en cours.
    pub job_queue: VecDeque<(String, AssetSource)>,
    /// Textures des artworks (décodées par Quark lui-même).
    pub textures: crate::images::TextureCache,
    /// Jaquettes réduites pour les grilles : le plein format reste réservé à la
    /// fiche et à la bannière floutée.
    pub thumbnails: crate::images::ThumbnailCache,
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
    /// Le contrôle courant vient du démarrage : ses erreurs restent discrètes.
    pub update_auto: bool,
    /// Le contrôle automatique du démarrage a déjà été tenté.
    pub startup_update_done: bool,
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
    /// Téléchargement de l'overlay Epic (EOS) en cours, s'il y en a un.
    pub eos_overlay_job: Option<EosOverlayJob>,
    /// Synchronisation des sauvegardes cloud Epic/GOG en cours.
    pub cloud_sync: Option<CloudSyncJob>,
    pub cloud_sync_status: HashMap<crate::providers::ProviderGameKey, String>,
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
    /// Dernière bannière affichée dans la fiche, pour le fondu enchaîné.
    pub hero_shown: Option<PathBuf>,
    pub hero_fade: Option<HeroFade>,
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
    /// Recherche libre dans les jeux possédés.
    pub my_games_query: String,
    /// Couvertures de boutiques manquantes, téléchargées une par une.
    pub provider_cover_job: Option<
        Receiver<(
            crate::providers::ProviderGameKey,
            Result<(PathBuf, Option<String>), String>,
        )>,
    >,
    pub provider_cover_queue: VecDeque<crate::providers::ProviderGameKey>,
    /// Couvertures actuellement téléchargées en parallèle (quatre au plus).
    pub provider_cover_inflight: usize,
    /// Couvertures déjà tentées avec succès : une image trouvée n'est plus
    /// redemandée.
    pub provider_cover_tried: std::collections::HashSet<crate::providers::ProviderGameKey>,
    /// Échecs par jeu : une erreur passagère (réseau, cache en cours d'écriture)
    /// est retentée, mais pas indéfiniment.
    pub provider_cover_failures: HashMap<crate::providers::ProviderGameKey, u8>,
    /// Images demandées après l'ajout d'un jeu installé à la bibliothèque.
    pub adopt_cover_job: Option<(
        String,
        crate::providers::ProviderGameKey,
        Receiver<(
            crate::providers::covers::ProviderArt,
            crate::providers::ProviderGameKey,
        )>,
    )>,
    /// Jeux de boutique dont la bannière a déjà été demandée : un échec réseau
    /// ne provoque pas une nouvelle requête à chaque image.
    pub provider_art_tried: std::collections::HashSet<crate::providers::ProviderGameKey>,
    /// Dernier balayage des jeux liés à une boutique sans bannière.
    pub provider_art_scan_at: Option<Instant>,
    /// Pont local des succès de session (127.0.0.1, jeton par lancement).
    pub bridge: Option<crate::achievements_bridge::Bridge>,
    pub bridge_rx: Option<Receiver<crate::achievements_bridge::Report>>,
    /// Déblocages rapportés pendant les parties, par jeu de boutique.
    pub session_unlocks:
        HashMap<crate::providers::ProviderGameKey, Vec<crate::providers::SessionUnlock>>,
    /// Écriture GOG d'un succès de session en cours.
    pub gog_unlock_job: Option<
        Receiver<(
            crate::providers::ProviderGameKey,
            String,
            Result<Option<String>, String>,
        )>,
    >,
    /// Fenêtre d'installation ouverte depuis une carte (taille + dossier).
    pub install_request: Option<InstallRequest>,
    /// Installations fournisseur en cours : plusieurs jeux à la fois.
    pub provider_install_jobs: Vec<ProviderInstallJob>,
    /// Dernier relevé de couvertures manquantes : la liste n'est pas reparcourue
    /// à chaque image.
    pub provider_cover_scan_at: Option<Instant>,
    /// Installation GOG (gogdl), même principe que l'installation Epic.
    pub gog_install_pending: Option<crate::providers::ProviderGameKey>,
    /// Succès Epic lus à la demande (jamais au démarrage).
    pub achievements: HashMap<crate::providers::ProviderGameKey, AchievementsState>,
    pub achievements_job: Option<
        Receiver<(
            crate::providers::ProviderGameKey,
            Result<crate::providers::legendary::AchievementInfo, String>,
        )>,
    >,
    /// Fenêtre des succès ouverte, s'il y en a une.
    pub achievements_open: Option<crate::providers::ProviderGameKey>,
    /// Progression RetroAchievements des jeux PS2, par identifiant Quark.
    pub ps2_ra: HashMap<String, AchievementsState>,
    pub ps2_ra_job: Option<(
        String,
        Receiver<Result<crate::providers::AchievementInfo, String>>,
    )>,
    /// Recherche d'une fiche RetroAchievements par titre.
    pub ps2_search_job: Option<(String, Receiver<Result<Option<crate::ps2::RaGame>, String>>)>,
    /// Onglet de la fiche jeu : actualités ou succès.
    pub detail_tab: DetailTab,
    /// Temps de jeu publié par la boutique, lu seulement pour la fiche
    /// affichée. Le temps local ne remplace jamais celui d'Epic ou GOG.
    pub provider_playtime:
        HashMap<crate::providers::ProviderGameKey, crate::providers::PlaytimeState>,
    pub playtime_job: Option<Receiver<(crate::providers::ProviderGameKey, Result<u64, String>)>>,
    /// Overlay en jeu (panneau au-dessus du jeu, captures F12, notifications).
    pub game_overlay: crate::overlay::Overlay,
    /// Vérification périodique des dossiers installés : un jeu supprimé hors de
    /// Quark doit repasser en « non installé ».
    pub install_probe_at: Option<Instant>,
    pub install_probe_rx: Option<Receiver<Vec<crate::providers::ProviderGameKey>>>,
    /// Désinstallation d'un jeu de boutique, après confirmation.
    pub confirm_uninstall: Option<crate::providers::ProviderGameKey>,
    pub uninstall_rx: Option<Receiver<(crate::providers::ProviderGameKey, Result<(), String>)>>,
    /// Panneau des téléchargements ouvert depuis l'icône de la barre du haut.
    pub downloads_open: bool,
    /// Choix de la confirmation « retirer de la bibliothèque » : supprimer
    /// aussi les fichiers de l'installation Epic/GOG liée.
    pub remove_with_files: bool,
    /// Désinstallations en attente, une seule à la fois.
    pub uninstall_queue: VecDeque<crate::providers::ProviderGameKey>,
    /// Synchronisation automatique lancée une seule fois, après l'affichage.
    pub startup_sync_done: bool,
    pub frames_alive: u32,
    pub epic_owned_fallback: Option<Vec<crate::providers::ProviderGame>>,
    pub epic_install_pending: Option<crate::providers::ProviderGameKey>,
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
        i18n::set_language(lib.settings.ui_language);
        let onboarding_open = !lib.settings.onboarding_done;
        let whats_new_open =
            !onboarding_open && lib.settings.last_seen_version != update::current_version();
        let random_hero_game = random_hero_from(&lib);
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
        // La page d'ouverture est lue une seule fois : changer le réglage
        // n'arrache pas l'utilisateur de la page où il se trouve.
        let (start_tab, start_home) = startup_route(lib.settings.start_page);
        theme::apply(ctx, lib.settings.theme);
        let pal = theme::palette(lib.settings.theme);
        let (store_recommendations, store_recommendation_source) =
            crate::steam_store::recommendations_for(store_snapshot.as_ref(), &lib.games, None);
        let (session_unlocks, session_unlocks_error) = crate::session_sync::load_session_unlocks();
        let config_error = lib.load_error.clone();
        Self {
            applied_theme: Some(lib.settings.theme),
            selected,
            grid_columns: 3,
            runtimes,
            tools: proton::Tools::detect(),
            lib,
            pal,
            tab: start_tab,
            filter: String::new(),
            command_palette_open: false,
            command_query: String::new(),
            command_selected: 0,
            command_last_query: String::new(),
            saved_views_open: false,
            saved_view_name: String::new(),
            saved_view_error: None,
            store_query: String::new(),
            site_search: None,
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
            onboarding_open,
            onboarding_step: 0,
            whats_new_open,
            random_hero_game,
            editing: None,
            game_tab: GameTab::General,
            game_dll_rows: Vec::new(),
            game_dll_for: None,
            game_link_query: String::new(),
            game_link_for: None,
            game_link_loading: false,
            game_link_results: Vec::new(),
            game_link_warnings: Vec::new(),
            game_link_rx: None,
            customize: None,
            picker: None,
            confirm_remove: Vec::new(),
            marked: Vec::new(),
            running: HashMap::new(),
            job: None,
            toasts: Vec::new(),
            dirty: false,
            last_change: Instant::now(),
            session_checkpoint_at: Instant::now(),
            save_retry_at: Instant::now(),
            save_backoff: Duration::from_secs(2),
            config_error,
            session_error: session_unlocks_error,
            snapshot_mode: false,
            show_info: false,
            job_queue: VecDeque::new(),
            textures: crate::images::TextureCache::default(),
            thumbnails: crate::images::ThumbnailCache::default(),
            accents: HashMap::new(),
            light_images: HashMap::new(),
            news: HashMap::new(),
            news_job: None,
            update: UpdateState::Idle,
            update_rx: None,
            update_auto: false,
            startup_update_done: false,
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
            eos_overlay_job: None,
            cloud_sync: None,
            cloud_sync_status: HashMap::new(),
            tray_error: None,
            home: start_home,
            carousel: 0,
            carousel_at: Instant::now(),
            home_at: Instant::now(),
            page_at: Instant::now(),
            hero_shown: None,
            hero_fade: None,
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
            my_games_query: String::new(),
            provider_cover_job: None,
            provider_cover_queue: VecDeque::new(),
            provider_cover_inflight: 0,
            provider_cover_scan_at: None,
            provider_cover_tried: std::collections::HashSet::new(),
            provider_cover_failures: HashMap::new(),
            adopt_cover_job: None,
            provider_art_tried: std::collections::HashSet::new(),
            provider_art_scan_at: None,
            bridge: None,
            bridge_rx: None,
            session_unlocks,
            gog_unlock_job: None,
            install_request: None,
            provider_install_jobs: Vec::new(),
            gog_install_pending: None,
            achievements: HashMap::new(),
            achievements_job: None,
            achievements_open: None,
            ps2_ra: HashMap::new(),
            ps2_ra_job: None,
            ps2_search_job: None,
            detail_tab: DetailTab::News,
            provider_playtime: HashMap::new(),
            playtime_job: None,
            game_overlay: crate::overlay::Overlay::default(),
            install_probe_at: None,
            install_probe_rx: None,
            confirm_uninstall: None,
            uninstall_rx: None,
            downloads_open: false,
            remove_with_files: false,
            uninstall_queue: VecDeque::new(),
            startup_sync_done: false,
            frames_alive: 0,
            epic_owned_fallback: None,
            epic_install_pending: None,
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

    /// Nouveau hero de fond, en évitant de retomber sur celui déjà affiché.
    pub fn pick_random_background(&mut self) {
        let candidates: Vec<String> = self
            .lib
            .games
            .iter()
            .filter(|game| {
                game.artwork
                    .get(AssetKind::Hero)
                    .is_some_and(|path| path.exists())
            })
            .map(|game| game.id.clone())
            .collect();
        if candidates.is_empty() {
            self.random_hero_game = None;
            return;
        }
        let current = self.random_hero_game.as_deref();
        let start = random_hero_from(&self.lib)
            .and_then(|id| candidates.iter().position(|candidate| *candidate == id))
            .unwrap_or(0);
        let next = (start..start + candidates.len())
            .map(|offset| offset % candidates.len())
            .find(|index| Some(candidates[*index].as_str()) != current)
            .unwrap_or(start);
        self.random_hero_game = Some(candidates[next].clone());
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
        self.poll_running_games(ctx);
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
        self.poll_provider_installs(ctx);
        self.poll_install_size(ctx);
        self.poll_provider_covers(ctx);
        self.poll_achievements(ctx);
        self.poll_ps2(ctx);
        self.poll_session_sync(ctx);
        self.poll_overlay(ctx);
        self.poll_provider_playtime();
        self.poll_install_probe(ctx);
        self.poll_uninstall(ctx);
        self.ensure_provider_playtime(ctx);
        self.poll_save_jobs(ctx);
        self.poll_save_background_jobs(ctx);
        self.poll_eos_overlay(ctx);
        self.poll_cloud_sync(ctx);
        self.poll_store(ctx);
        self.poll_site_search(ctx);
        if self.tab == Tab::Store && !self.store_offline_mode {
            self.ensure_store(ctx);
        }

        if !self.provider_install_jobs.is_empty()
            || self.provider_cover_job.is_some()
            || self.achievements_job.is_some()
            || self.gog_unlock_job.is_some()
            || self.adopt_cover_job.is_some()
            || self
                .site_search
                .as_ref()
                .is_some_and(|search| search.pending > 0)
            || self.install_request.is_some()
            || self.playtime_job.is_some()
            || self.install_probe_rx.is_some()
            || self.uninstall_rx.is_some()
        {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
        if self.add.scan.running || self.add.scan.search_rx.is_some() {
            ctx.request_repaint_after(Duration::from_millis(120));
        }

        // Recherche en cours : entretient l'animation du spinner.
        if matches!(self.add.state, SearchState::Loading) {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        if self.dirty
            && !self.snapshot_mode
            && self.last_change.elapsed() > Duration::from_millis(800)
            && Instant::now() >= self.save_retry_at
        {
            match self.lib.save() {
                Ok(()) => {
                    self.dirty = false;
                    self.config_error = None;
                    self.save_backoff = Duration::from_secs(2);
                }
                Err(err) => {
                    // Ne jamais perdre les modifications : on garde `dirty` et
                    // on réessaie avec un délai croissant.
                    let message = format!("Sauvegarde impossible : {err}");
                    self.config_error = Some(message.clone());
                    self.toast_error(message);
                    self.save_retry_at = Instant::now() + self.save_backoff;
                    self.save_backoff = (self.save_backoff * 2).min(Duration::from_secs(60));
                }
            }
            self.last_change = Instant::now();
        }

        let now = Instant::now();
        self.toasts.retain(|t| t.until > now);

        if ctx.input(|i| i.viewport().close_requested()) {
            let mut blocked = false;
            if !self.snapshot_mode && self.dirty {
                if let Err(err) = self.lib.save() {
                    let message = format!(
                        "Fermeture annulée : la bibliothèque n'a pas pu être enregistrée ({err})."
                    );
                    self.config_error = Some(message.clone());
                    self.toast_error(message);
                    // La fenêtre reste ouverte : fermer ici perdrait les
                    // dernières modifications.
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    blocked = true;
                } else {
                    self.dirty = false;
                }
            }
            if !blocked {
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

    /// Écrit chaque minute la durée des sessions en cours dans la bibliothèque.
    ///
    /// Sans cela, le temps de jeu n'était inscrit qu'à la fin de la partie ou à
    /// la fermeture normale de Quark : un plantage ou un `kill` perdait toute la
    /// session. L'enregistrement disque passe par le même mécanisme différé que
    /// les autres changements (`touch`).
    fn checkpoint_running_sessions(&mut self) {
        const INTERVAL: Duration = Duration::from_secs(60);
        if self.running.is_empty() || self.snapshot_mode {
            return;
        }
        if self.session_checkpoint_at.elapsed() < INTERVAL {
            return;
        }
        self.session_checkpoint_at = Instant::now();
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
        self.touch();
    }

    fn poll_running_games(&mut self, ctx: &egui::Context) {
        self.checkpoint_running_sessions();
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
                Ok(Some(_)) => {
                    // Le chef est mort, mais un wrapper ou Wine peut encore
                    // tourner dans le groupe : on continue l'escalade avant de
                    // retirer la session.
                    if game.stopping.is_some() && game.group_alive() {
                        if game.should_escalate() {
                            crate::session::force_kill(game);
                            game.escalated();
                        }
                        if !game.should_abandon() {
                            game.session = game.started.elapsed().as_secs();
                            continue;
                        }
                    }
                    finished.push((
                        id.clone(),
                        game.started.elapsed().as_secs(),
                        game.started_ts,
                        game.save_identity.clone(),
                    ));
                }
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
            // Le temps de jeu de la boutique vient de bouger : la prochaine
            // ouverture de la fiche le relit au lieu de garder l'ancien.
            if let Some(key) = self
                .provider_record_for_game(&id)
                .map(|record| record.key.clone())
            {
                self.provider_playtime.remove(&key);
            }
            self.touch();
            if self.lib.settings.cloud_saves
                && self.cloud_sync.is_none()
                && self.can_auto_sync_cloud(&id)
            {
                self.sync_cloud_saves(ctx, &id);
            }
        }
    }

    /// Vrai quand la synchronisation peut démarrer sans demander quoi que ce
    /// soit : jeu de boutique installé, et dossier de sauvegarde connu pour
    /// GOG. Une synchronisation automatique sans dossier GOG ne ferait
    /// qu'afficher une erreur après chaque partie.
    fn can_auto_sync_cloud(&self, game_id: &str) -> bool {
        let Some(record) = self.provider_record_for_game(game_id) else {
            return false;
        };
        if record.install_state != crate::providers::InstallState::Installed {
            return false;
        }
        match record.key.provider {
            // Sans outil ou sans session, la synchronisation automatique ne
            // pourrait qu'échouer après chaque partie : on la laisse alors
            // silencieuse. Le bouton manuel, lui, explique le problème.
            crate::providers::Provider::Epic => {
                let adapter = crate::providers::legendary::LegendaryAdapter::new();
                adapter.executable.is_file()
                    && crate::providers::legendary::LegendaryAdapter::is_logged_in()
            }
            crate::providers::Provider::Gog => {
                let adapter = crate::providers::gog::GogAdapter::new(
                    crate::providers::gog::auth_config_path(),
                );
                adapter.executable.is_file()
                    && crate::providers::gog::GogAdapter::is_logged_in()
                    && crate::providers::cloud_saves::save_path_for(self.lib.get(game_id), record)
                        .is_some()
            }
            crate::providers::Provider::Steam => false,
        }
    }

    /// Lance la synchronisation des sauvegardes cloud d'un jeu lié à Epic ou
    /// GOG. Epic peut calculer son dossier tout seul ; GOG utilise le dossier
    /// choisi dans la fiche du jeu.
    pub fn sync_cloud_saves(&mut self, ctx: &egui::Context, game_id: &str) {
        if self.cloud_sync.is_some() {
            self.toast_info("Une synchronisation de sauvegardes est déjà en cours");
            return;
        }
        let Some(record) = self.provider_record_for_game(game_id).cloned() else {
            self.toast_info("Ce jeu n'est lié à aucune boutique");
            return;
        };
        if record.key.provider == crate::providers::Provider::Steam {
            self.toast_info("Steam ne publie pas ses sauvegardes cloud hors du client Steam");
            return;
        }
        if record.install_state != crate::providers::InstallState::Installed {
            self.toast_info("Installe d'abord le jeu pour synchroniser ses sauvegardes");
            return;
        }
        let save_path =
            crate::providers::cloud_saves::save_path_for(self.lib.get(game_id), &record);
        if record.key.provider == crate::providers::Provider::Gog && save_path.is_none() {
            self.cloud_sync_status.insert(
                record.key.clone(),
                "Choisis un dossier de sauvegarde pour ce jeu GOG".to_string(),
            );
            self.toast_error("GOG : choisis d'abord un dossier de sauvegarde dans la fiche du jeu");
            return;
        }
        let timestamp = self
            .lib
            .installation_for_provider(game_id, &record.key)
            .and_then(|installation| installation.save_sync_ts);
        let key = record.key.clone();
        let title = record.title.clone();
        let game_id = game_id.to_string();
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result =
                crate::providers::cloud_saves::sync(&record, save_path.as_deref(), timestamp);
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.cloud_sync = Some(CloudSyncJob {
            key: key.clone(),
            game_id,
            title: title.clone(),
            rx,
        });
        self.cloud_sync_status
            .insert(key, format!("{title} : synchronisation en cours…"));
        self.toast_info(format!("{title} : synchronisation des sauvegardes cloud…"));
        ctx.request_repaint_after(Duration::from_millis(150));
    }

    fn poll_cloud_sync(&mut self, ctx: &egui::Context) {
        let Some(job) = self.cloud_sync.as_ref() else {
            return;
        };
        let key = job.key.clone();
        let game_id = job.game_id.clone();
        let title = job.title.clone();
        match job.rx.try_recv() {
            Ok(Ok(report)) => {
                if let Some(installation) = self.lib.installation_for_provider_mut(&game_id, &key) {
                    installation.last_synced = Some(crate::config::now_ts());
                    if let Some(timestamp) = report.timestamp {
                        installation.save_sync_ts = Some(timestamp);
                    }
                }
                self.cloud_sync_status
                    .insert(key, format!("{title} : {}", report.message));
                self.cloud_sync = None;
                self.toast_info(format!("{title} : {}", report.message));
                self.touch();
            }
            Ok(Err(error)) => {
                self.cloud_sync_status
                    .insert(key, format!("{title} : {error}"));
                self.cloud_sync = None;
                self.toast_error(format!("Sauvegardes cloud de {title} : {error}"));
            }
            Err(TryRecvError::Disconnected) => {
                self.cloud_sync = None;
                self.cloud_sync_status
                    .insert(key, format!("{title} : synchronisation interrompue"));
            }
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(150));
            }
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
            if matches!(&thread_target, PickerTarget::AddPs2Iso) {
                dialog = dialog
                    .add_filter(
                        "Images disque PS2",
                        &[
                            "iso", "ISO", "chd", "CHD", "cso", "CSO", "bin", "BIN", "cue", "CUE",
                            "gz", "GZ",
                        ],
                    )
                    .add_filter("Tous les fichiers", &["*"]);
            } else if matches!(&thread_target, PickerTarget::CustomArtwork(_)) {
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
                            "iso", "ISO", "chd", "CHD", "cso", "CSO", "cue", "CUE",
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
            PickerTarget::AddPs2Iso => self.add_ps2_image(ctx, path),
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
            PickerTarget::GameDllSource => {
                if let Some(game) = self.editing.as_mut() {
                    game.dll_source = Some(text);
                }
            }
            PickerTarget::Pcsx2Path => {
                self.lib.settings.pcsx2_path = text;
                self.touch();
            }
            PickerTarget::DefaultPrefix => {
                self.lib.settings.default_prefix = Some(text);
                self.touch();
            }
            PickerTarget::DefaultInstallDir => {
                self.lib.settings.default_install_dir = Some(text);
                self.touch();
            }
            PickerTarget::InstallDestination => {
                if let Some(request) = self.install_request.as_mut() {
                    request.destination = path;
                }
            }
            PickerTarget::CustomArtwork(kind) => self.import_artwork(kind, path),
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

    pub(crate) fn queue_asset_job(
        &mut self,
        ctx: &egui::Context,
        game_id: &str,
        source: AssetSource,
    ) {
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
            AssetSource::Ps2(serial) => format!("PS2 {serial}"),
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
                AssetSource::Ps2(serial) => {
                    crate::ps2::fetch_cover_for(&client, &thread_id, &serial)
                }
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

    /// Ajoute une image PS2 comme jeu : PCSX2, numéro de série lu dans l'ISO
    /// et jaquette communautaire demandée en arrière-plan.
    pub fn add_ps2_image(&mut self, ctx: &egui::Context, path: PathBuf) {
        if !crate::ps2::is_disc_image(&path) {
            self.toast_error("Choisis une image PS2 (.iso, .chd, .cso, .bin/.cue…).");
            return;
        }
        let serial = crate::ps2::serial_from_iso(&path).unwrap_or_default();
        let name = crate::ps2::title_from_path(&path);
        let settings = self.lib.settings.clone();
        let mut game = Game::new(name.clone(), path.display().to_string());
        game.proton = ProtonMode::Native;
        game.args = settings.default_args.trim().to_string();
        game.launch = settings.default_launch.clone();
        game.ps2 = Some(crate::config::Ps2Config {
            serial: serial.clone(),
            ra_game_id: None,
        });
        let id = game.id.clone();
        self.lib.games.push(game);
        self.selected = Some(id.clone());
        self.add.open = false;
        self.tab = Tab::Library;
        if serial.is_empty() {
            self.toast_info(format!(
                "{name} ajouté · numéro de série à saisir dans la fiche du jeu"
            ));
        } else {
            self.toast_info(format!("{name} ajouté · {serial}"));
            self.queue_asset_job(ctx, &id, AssetSource::Ps2(serial));
        }
        self.touch();
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

    /// Recherche un jeu à lier à une fiche Steam / SteamGridDB, pour récupérer
    /// ses images. Les résultats restent dans l'état tant que le même jeu est
    /// affiché dans les paramètres.
    pub fn start_game_link_search(&mut self, ctx: &egui::Context, game_id: &str, query: &str) {
        let query = query.trim();
        if query.len() < 2 {
            self.game_link_warnings = vec!["Saisis au moins deux caractères.".to_string()];
            return;
        }
        self.game_link_for = Some(game_id.to_string());
        self.game_link_loading = true;
        self.game_link_results.clear();
        self.game_link_warnings.clear();
        let (tx, rx) = channel();
        let settings = self.lib.settings.clone();
        let query = query.to_string();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let client = artwork::http();
            let outcome = artwork::search(&client, &query, &settings);
            let _ = tx.send(outcome);
            repaint.request_repaint();
        });
        self.game_link_rx = Some(rx);
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    fn poll_game_link_search(&mut self) {
        let received = self.game_link_rx.as_ref().map(Receiver::try_recv);
        match received {
            Some(Ok(outcome)) => {
                self.game_link_rx = None;
                self.game_link_loading = false;
                self.game_link_results = outcome.hits;
                self.game_link_warnings = outcome.warnings;
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.game_link_rx = None;
                self.game_link_loading = false;
                self.game_link_warnings = vec!["Recherche interrompue.".to_string()];
            }
            Some(Err(TryRecvError::Empty)) | None => {}
        }
    }

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

    /// Images déjà téléchargées pour les fiches de boutique reliées à ce jeu.
    /// Elles sont proposées dans « Personnaliser » sans nouvelle requête.
    pub(crate) fn provider_art_choices(
        &self,
        game_id: &str,
    ) -> Vec<(AssetKind, PathBuf, &'static str)> {
        let mut choices: Vec<(AssetKind, PathBuf, &'static str)> = Vec::new();
        for record in &self.lib.provider_library.games {
            if record.quark_game_id.as_deref() != Some(game_id) {
                continue;
            }
            let source = match record.key.provider {
                crate::providers::Provider::Steam => "Steam",
                crate::providers::Provider::Epic => "Epic Games",
                crate::providers::Provider::Gog => "GOG",
            };
            for (kind, path) in [
                (
                    AssetKind::Cover,
                    crate::providers::covers::cached(&record.key),
                ),
                (
                    AssetKind::Hero,
                    crate::providers::covers::cached_hero(&record.key),
                ),
            ] {
                if let Some(path) = path {
                    if !choices.iter().any(|(_, existing, _)| existing == &path) {
                        choices.push((kind, path, source));
                    }
                }
            }
        }
        choices
    }

    /// Importe une image de boutique dans les artworks du jeu, sans toucher aux
    /// autres emplacements.
    pub fn import_provider_art(&mut self, kind: AssetKind, source: &std::path::Path) {
        let Some(game_id) = self.customize.as_ref().map(|c| c.game_id.clone()) else {
            return;
        };
        let dir = crate::config::artwork_root().join(&game_id);
        match artwork::import_file(source, &dir, kind) {
            Ok(path) => {
                if let Some(game) = self.lib.get_mut(&game_id) {
                    game.artwork.set(kind, path);
                }
                self.toast_info(format!("{} importée depuis la boutique", kind.label()));
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
        let Some(mut game) = self.lib.get(id).cloned() else {
            return;
        };
        // Un jeu adopté depuis Epic ou GOG ne reçoit pas les variables
        // « par défaut » : elles ont été pensées pour les jeux ajoutés à la
        // main, et les injecter dans un jeu de boutique casse ses lanceurs.
        // Les variables propres au jeu restent, elles.
        let provider_record = self.provider_record_for_game(id).cloned();
        if provider_record.is_some() {
            let defaults = self.lib.settings.default_env.clone();
            game.env.retain(|variable| {
                !defaults
                    .iter()
                    .any(|default| default.name == variable.name && default.value == variable.value)
            });
        }
        // Jeu de boutique ajouté sans chemin d'exécutable (gogdl ne le publie
        // pas) : le chercher dans le dossier installé évite de refuser le
        // lancement alors que le jeu est bien là.
        if game.exe.trim().is_empty()
            && let Some(record) = provider_record.as_ref()
            && let Some(directory) = record
                .install_path
                .as_deref()
                .filter(|directory| directory.is_dir())
            && let Some(exe) = crate::steamlib::find_exe(
                directory,
                &record.title,
                &directory
                    .file_name()
                    .map(|name| name.to_string_lossy())
                    .unwrap_or_default(),
            )
        {
            game.exe = exe.display().to_string();
            if let Some(stored) = self.lib.get_mut(id) {
                stored.exe = game.exe.clone();
            }
            if let Some(stored) = self
                .lib
                .provider_library
                .games
                .iter_mut()
                .find(|candidate| candidate.key == record.key)
            {
                stored.install_exe = Some(exe);
            }
            self.touch();
            self.toast_info(format!(
                "{} : exécutable détecté automatiquement",
                game.name
            ));
        }
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
        // Un jeu Epic lancé directement par son exécutable n'a ni le jeton de
        // session ni les arguments `-epicapp`/`-EpicPortal`. Legendary les
        // calcule sans lancer le jeu (`launch --json`) ; sans eux, Epic répond
        // « impossible de se connecter ». Les paramètres entrent dans la
        // commande avant les wrappers, comme le fait Steam.
        let mut store_parameters: Vec<String> = Vec::new();
        if provider_record
            .as_ref()
            .is_some_and(|record| record.key.provider == crate::providers::Provider::Epic)
        {
            let adapter = crate::providers::legendary::LegendaryAdapter::new();
            let app_name = provider_record
                .as_ref()
                .map(|record| record.key.product_id.as_str())
                .unwrap_or_default();
            match adapter.launch_parameters(app_name) {
                Ok(parameters) => {
                    store_parameters = parameters
                        .game_parameters
                        .into_iter()
                        .chain(parameters.egl_parameters)
                        .collect();
                }
                Err(error) => self.toast_error(format!(
                    "{} : authentification Epic indisponible ({error})",
                    game.name
                )),
            }
        }
        match proton::resolve_with_parameters(
            &game,
            &self.lib.settings,
            &self.runtimes,
            &store_parameters,
        ) {
            Ok(mut resolved) => {
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
                // Pont des succès : adresse et jeton propres à ce lancement.
                for (name, value) in self.bridge_env(ctx, id) {
                    resolved.spec.env.push((name, value));
                }
                // Un jeu Epic lancé directement par son exécutable n'a ni le
                // jeton de session ni les arguments `-epicapp`/`-EpicPortal`.
                // Legendary les calcule sans lancer le jeu (`launch --json`) ;
                // sans eux, Epic répond « impossible de se connecter ».
                if provider_record
                    .as_ref()
                    .is_some_and(|record| record.key.provider == crate::providers::Provider::Epic)
                {
                    let adapter = crate::providers::legendary::LegendaryAdapter::new();
                    // L'overlay EOS est activé par préfixe : on le réactive au
                    // lancement si l'utilisateur l'a déjà installé.
                    if let Some(prefix) = resolved.prefix.as_ref() {
                        let wine_prefix = if game.umu {
                            prefix.clone()
                        } else {
                            prefix.join("pfx")
                        };
                        if crate::providers::legendary::LegendaryAdapter::eos_overlay_installed_path()
                            .is_some()
                        {
                            if wine_prefix.join("user.reg").is_file() {
                                match adapter.enable_eos_overlay(&wine_prefix) {
                                    Ok(()) => resolved.extras.push("overlay EOS".to_string()),
                                    Err(error) => {
                                        self.toast_error(format!("Overlay Epic : {error}"));
                                    }
                                }
                            } else {
                                // `eos-overlay enable` écrit dans user.reg : sur
                                // un préfixe neuf, il n'existe pas encore. Le
                                // premier lancement le crée ; le suivant active
                                // l'overlay avant le démarrage du jeu.
                                self.toast_info(format!(
                                    "{} : l'overlay Epic sera activé au prochain lancement (préfixe en cours de création)",
                                    game.name
                                ));
                            }
                        }
                    }
                    if !store_parameters.is_empty() {
                        resolved.extras.push("authentification Epic".to_string());
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
                        let started = session.started;
                        self.running.insert(id.to_string(), session);
                        let provider = provider_record.as_ref().map(|record| record.key.provider);
                        if self.lib.settings.overlay {
                            let screenshot = self.lib.settings.screenshot_key.clone();
                            let hotkey = self.lib.settings.overlay_hotkey.clone();
                            self.game_overlay.set_game(
                                crate::overlay::Target {
                                    name: game.name.clone(),
                                    provider,
                                    key: provider_record.as_ref().map(|record| record.key.clone()),
                                    game_id: id.to_string(),
                                    ra_game_id: game.ps2.as_ref().and_then(|ps2| ps2.ra_game_id),
                                    started,
                                },
                                &screenshot,
                                &hotkey,
                            );
                            self.game_overlay
                                .notice("Partie lancée", format!("{} · overlay prêt", game.name));
                        }
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
        // Une mise à jour interrompue peut avoir laissé une sauvegarde ou un
        // binaire préparé à côté de l'exécutable ; le nouveau processus les
        // nettoie une fois qu'il a démarré correctement.
        update::cleanup_stale();
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

    /// Bandeau persistant quand la configuration n'a pas pu être lue ou
    /// écrite : un toast fugace laisserait croire que tout est enregistré.
    fn config_error_banner(&mut self, ctx: &egui::Context) {
        let errors: Vec<String> = [self.config_error.clone(), self.session_error.clone()]
            .into_iter()
            .flatten()
            .collect();
        if errors.is_empty() {
            return;
        }
        let pal = self.pal;
        egui::TopBottomPanel::top("quark_config_error")
            .frame(
                egui::Frame::new()
                    .fill(pal.panel)
                    .inner_margin(egui::Margin::symmetric(14, 6))
                    .stroke(Stroke::new(1.0_f32, pal.danger)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        crate::i18n::rich(errors.join(" · "))
                            .size(11.5)
                            .color(pal.danger),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::ghost_button(ui, "Dossier", &pal, Icon::Folder).clicked() {
                            let _ = crate::session::open_folder(&crate::config::config_dir());
                        }
                    });
                });
            });
    }

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
                    ui.label(
                        crate::i18n::rich("QUARK")
                            .size(16.0)
                            .strong()
                            .color(pal.text),
                    );
                    ui.add_space(30.0);

                    // L'accueil a son propre onglet : il reste une page de la
                    // bibliothèque (`Tab::Library` + `home`), mais l'utilisateur
                    // le trouve au même niveau que les trois autres.
                    let home_active = self.tab == Tab::Library && self.home;
                    let library_active = self.tab == Tab::Library && !self.home;
                    if widgets::tab(ui, "Accueil", home_active, &pal).clicked() {
                        self.tab = Tab::Library;
                        self.home = true;
                    }
                    if widgets::tab(ui, "Boutique", self.tab == Tab::Store, &pal).clicked() {
                        self.tab = Tab::Store;
                    }
                    if widgets::tab(ui, "Mes jeux", self.tab == Tab::MyGames, &pal).clicked() {
                        self.tab = Tab::MyGames;
                    }
                    if widgets::tab(ui, "Bibliothèque", library_active, &pal).clicked() {
                        self.tab = Tab::Library;
                        self.home = false;
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
                        if widgets::icon_button(
                            ui,
                            32.0,
                            Icon::Search,
                            &pal,
                            "Palette de commandes · Ctrl+K",
                        )
                        .clicked()
                        {
                            self.open_command_palette(ctx);
                        }
                        ui.add_space(6.0);
                        // Téléchargements : le même panneau, accessible depuis
                        // tous les onglets, avec le nombre de transferts actifs.
                        let response =
                            widgets::icon_button(ui, 32.0, Icon::Download, &pal, "Téléchargements");
                        if response.clicked() {
                            self.downloads_open = !self.downloads_open;
                        }
                        let active = self.active_install_count();
                        if active > 0 {
                            let badge = egui::Pos2::new(
                                response.rect.right() - 3.0,
                                response.rect.top() + 5.0,
                            );
                            ui.painter().circle_filled(badge, 7.0, pal.accent);
                            ui.painter().text(
                                badge,
                                Align2::CENTER_CENTER,
                                active.to_string(),
                                FontId::proportional(9.0),
                                pal.bg,
                            );
                        }

                        ui.add_space(6.0);
                        // Seul endroit où l'on bascule de vue : un contrôle
                        // segmenté aux libellés visibles, et un bouton
                        // « Ajouter » explicite à la place de trois icônes.
                        if self.tab == Tab::Library {
                            ui.add_space(4.0);
                            if widgets::ghost_button(ui, "Ajouter", &pal, Icon::Plus).clicked() {
                                self.open_add_dialog();
                            }
                            ui.add_space(8.0);
                            let current = usize::from(self.lib.settings.view == ViewMode::List);
                            if let Some(index) = widgets::segmented(
                                ui,
                                &pal,
                                &[(Icon::Grid, "Grille"), (Icon::List, "Liste")],
                                current,
                            ) {
                                self.set_view(if index == 0 {
                                    ViewMode::Grid
                                } else {
                                    ViewMode::List
                                });
                            }
                            ui.add_space(6.0);
                        }
                        if self.tab == Tab::Library {
                            // La recherche cède de la place quand la fenêtre est
                            // étroite (1024 px minimum) au lieu de chevaucher
                            // les onglets.
                            let search_width = (ui.available_width() - 12.0).clamp(120.0, 260.0);
                            ui.add_sized(
                                Vec2::new(search_width, 30.0),
                                egui::TextEdit::singleline(&mut self.filter)
                                    .id(egui::Id::new("quark_filter"))
                                    .hint_text(crate::i18n::tr(
                                        "Rechercher dans la bibliothèque  ·  Ctrl+F",
                                    ))
                                    .font(FontId::proportional(12.5)),
                            );
                        } else if self.tab == Tab::Store {
                            ui.label(
                                crate::i18n::rich("Catalogue Steam")
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
                                    crate::i18n::rich(&toast.text)
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
        let settings = &self.lib.settings;
        self.lib
            .sorted_ids_by(settings.library_sort)
            .into_iter()
            .filter(|id| {
                let Some(game) = self.lib.get(id) else {
                    return false;
                };
                self.lib.matches(
                    game,
                    &settings.home_genre,
                    &settings.home_tag,
                    settings.home_status,
                ) && game.matches_query(&self.filter)
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
        self.remove_with_files = self
            .confirm_remove
            .iter()
            .any(|id| self.linked_provider_install(id).is_some());
    }

    /// Demande le retrait depuis le clic droit : tout le lot si le jeu visé
    /// fait partie de la sélection multiple, lui seul sinon.
    pub fn request_remove(&mut self, id: &str) {
        let targets = removal_targets(&self.marked, id);
        if targets.len() == 1 {
            self.select(id);
        }
        self.confirm_remove = targets;
        self.remove_with_files = self
            .confirm_remove
            .iter()
            .any(|id| self.linked_provider_install(id).is_some());
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
        // Défilement un peu plus rapide, à la souris comme au pavé tactile.
        // egui applique son propre lissage : on accélère le delta déjà lissé,
        // et la vitesse des molettes à crans. Aucun autre geste n'est touché.
        ctx.options_mut(|options| options.input_options.line_scroll_speed = 46.0);
        let scroll = ctx.input(|input| input.smooth_scroll_delta);
        if scroll != egui::Vec2::ZERO {
            ctx.input_mut(|input| input.smooth_scroll_delta = scroll * 1.2);
        }
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
        self.config_error_banner(ctx);
        match self.tab {
            Tab::Store => self.store_tab(ctx),
            Tab::Library => self.library_tab(ctx),
            Tab::MyGames => self.my_games_tab(ctx),
        }

        self.poll_game_link_search();
        self.onboarding_dialog(ctx);
        self.whats_new_dialog(ctx);
        self.downloads_panel(ctx);
        self.add_dialog(ctx);
        self.saved_views_dialog(ctx);
        self.settings_dialog(ctx);
        self.steam_import_dialog(ctx);
        self.customize_dialog(ctx);
        self.game_settings_dialog(ctx);
        self.confirm_remove_dialog(ctx);
        self.confirm_uninstall_dialog(ctx);
        self.install_dialog(ctx);
        self.logs_dialog(ctx);
        self.achievements_dialog(ctx);
        self.save_backup_dialog(ctx);
        self.toasts_ui(ctx);
        self.command_palette(ctx);
        // Dernière étape : une fenêtre séparée, au-dessus du jeu. Elle n'existe
        // que si un jeu tourne et si le réglage est actif.
        if self.lib.settings.overlay && !self.running.is_empty() {
            self.game_overlay.ui(ctx, &self.pal, &self.lib.settings);
        }
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
        let palette = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::K));
        if palette {
            if self.command_palette_open {
                self.close_command_palette(ctx);
            } else {
                self.open_command_palette(ctx);
            }
            return;
        }
        if self.command_palette_open {
            return;
        }
        if self.add.open
            || self.settings_open
            || self.saved_views_open
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
        let columns: i64 = if self.lib.settings.view == ViewMode::Grid {
            self.grid_columns.max(1) as i64
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

    /// Fiche de boutique liée à un jeu de la bibliothèque Quark, s'il y en a
    /// une. C'est le seul lien fiable : un titre identique ne suffit pas.
    pub fn provider_record_for_game(&self, id: &str) -> Option<&crate::providers::ProviderGame> {
        self.lib
            .provider_library
            .games
            .iter()
            .find(|game| game.quark_game_id.as_deref() == Some(id))
    }

    /// Installation de boutique liée à une fiche Quark, seulement si elle est
    /// réellement installée et que son dossier est connu.
    pub(crate) fn linked_provider_install(
        &self,
        id: &str,
    ) -> Option<crate::providers::ProviderGameKey> {
        self.provider_record_for_game(id)
            .filter(|record| {
                record.install_state == crate::providers::InstallState::Installed
                    && record
                        .install_path
                        .as_ref()
                        .is_some_and(|path| path.is_absolute())
            })
            .map(|record| record.key.clone())
    }

    /// Temps de jeu de la boutique pour le jeu affiché. Aucun repli sur le
    /// temps local : la fiche d'un jeu Epic/GOG montre la valeur de la
    /// boutique, ou dit qu'elle est indisponible.
    pub(crate) fn ensure_provider_playtime(&mut self, ctx: &egui::Context) {
        if self.playtime_job.is_some() {
            return;
        }
        let Some(id) = self.selected.clone() else {
            return;
        };
        let Some(record) = self.provider_record_for_game(&id).cloned() else {
            return;
        };
        if record.key.provider == crate::providers::Provider::Steam {
            return;
        }
        if self.provider_playtime.contains_key(&record.key) {
            return;
        }
        self.provider_playtime
            .insert(record.key.clone(), crate::providers::PlaytimeState::Loading);
        let key = record.key.clone();
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = crate::providers::playtime::fetch(&key);
            let _ = tx.send((key, result));
            repaint.request_repaint();
        });
        self.playtime_job = Some(rx);
        ctx.request_repaint_after(Duration::from_millis(200));
    }

    fn poll_provider_playtime(&mut self) {
        let Some(result) = self.playtime_job.as_ref().map(Receiver::try_recv) else {
            return;
        };
        match result {
            Ok((key, Ok(seconds))) => {
                self.provider_playtime
                    .insert(key, crate::providers::PlaytimeState::Ready(seconds));
                self.playtime_job = None;
            }
            Ok((key, Err(error))) => {
                self.provider_playtime
                    .insert(key, crate::providers::PlaytimeState::Failed(error));
                self.playtime_job = None;
            }
            Err(TryRecvError::Disconnected) => {
                self.playtime_job = None;
                // Le fil a disparu sans répondre : l'état ne doit pas rester
                // bloqué sur « chargement » indéfiniment.
                for state in self.provider_playtime.values_mut() {
                    if matches!(state, crate::providers::PlaytimeState::Loading) {
                        *state = crate::providers::PlaytimeState::Failed(
                            "lecture du temps de jeu interrompue".to_string(),
                        );
                    }
                }
            }
            Err(TryRecvError::Empty) => {}
        }
    }

    /// Panneau flottant des téléchargements, sous l'icône de la barre du haut.
    fn downloads_panel(&mut self, ctx: &egui::Context) {
        if !self.downloads_open {
            return;
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.downloads_open = false;
            return;
        }
        let pal = self.pal;
        let screen = ctx.screen_rect();
        egui::Area::new(egui::Id::new("quark_downloads_panel"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::Pos2::new((screen.right() - 400.0).max(12.0), 58.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(pal.header)
                    .stroke(egui::Stroke::new(1.0_f32, pal.border))
                    .corner_radius(10.0)
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.set_min_width(360.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                crate::i18n::rich("TÉLÉCHARGEMENTS")
                                    .size(10.0)
                                    .strong()
                                    .color(pal.accent),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if widgets::icon_button(ui, 22.0, Icon::Close, &pal, "Fermer")
                                    .clicked()
                                {
                                    self.downloads_open = false;
                                }
                            });
                        });
                        ui.add_space(4.0);
                        self.downloads_popup(ui, &pal);
                    });
            });
    }

    /// Captures F12 et événements de l'overlay. Le son et la notification ne
    /// sont joués que pour Epic/GOG, comme demandé ; Steam garde l'overlay
    /// silencieux.
    fn poll_overlay(&mut self, ctx: &egui::Context) {
        // Overlay désactivé : relâcher la touche globale F12, sinon elle
        // resterait prise au détriment du jeu.
        if !self.lib.settings.overlay && self.game_overlay.target.is_some() {
            self.game_overlay.clear_game();
        }
        // Overlay activé pendant une partie : le raccourci n'était pas pris au
        // lancement, on l'installe maintenant sans attendre la prochaine partie.
        if self.lib.settings.overlay && self.game_overlay.target.is_none() {
            let target = self.running.iter().next().and_then(|(id, session)| {
                let game = self.lib.get(id)?;
                let record = self.provider_record_for_game(id);
                Some(crate::overlay::Target {
                    name: game.name.clone(),
                    provider: record.map(|record| record.key.provider),
                    key: record.map(|record| record.key.clone()),
                    game_id: id.clone(),
                    ra_game_id: game.ps2.as_ref().and_then(|ps2| ps2.ra_game_id),
                    started: session.started,
                })
            });
            if let Some(target) = target {
                let name = target.name.clone();
                self.game_overlay.set_game(
                    target,
                    &self.lib.settings.screenshot_key,
                    &self.lib.settings.overlay_hotkey,
                );
                self.game_overlay
                    .notice("Overlay activé", format!("{name} · prêt"));
            }
        }
        // Une capture peut arriver juste après la fermeture du jeu : elle est
        // traitée avant de masquer l'overlay, sinon elle serait perdue.
        let captures = self.game_overlay.poll_captures();
        if self.game_overlay.open {
            if let Some(key) = self.game_overlay.target_key() {
                match self.achievements.get(&key).cloned() {
                    Some(AchievementsState::Ready(mut info)) => {
                        if let Some(unlocks) = self.session_unlocks.get(&key) {
                            crate::providers::achievements::merge_session(&mut info, unlocks);
                        }
                        self.game_overlay.set_achievements(info);
                    }
                    Some(AchievementsState::Failed(error)) => {
                        self.game_overlay.set_achievements_error(Some(error));
                    }
                    Some(AchievementsState::Loading) => {
                        self.game_overlay.set_achievements_loading(true);
                    }
                    None => {
                        self.game_overlay.set_achievements_loading(true);
                        self.request_achievements(ctx, &key);
                    }
                }
            } else if let Some((game_id, ra_id)) = self.game_overlay.target_ra() {
                match self.ps2_ra.get(&game_id).cloned() {
                    Some(AchievementsState::Ready(info)) => {
                        self.game_overlay.set_achievements(info);
                    }
                    Some(AchievementsState::Failed(error)) => {
                        self.game_overlay.set_achievements_error(Some(error));
                    }
                    Some(AchievementsState::Loading) => {
                        self.game_overlay.set_achievements_loading(true);
                    }
                    None => {
                        self.game_overlay.set_achievements_loading(true);
                        self.request_ps2_ra(ctx, &game_id, ra_id);
                    }
                }
            }
        }
        for capture in captures {
            let file = capture
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("capture.png")
                .to_string();
            let provider = capture.provider;
            if matches!(
                provider,
                Some(crate::providers::Provider::Epic) | Some(crate::providers::Provider::Gog)
            ) {
                crate::overlay::play_chime();
                crate::overlay::desktop_notification(
                    "Capture enregistrée",
                    &format!("{} · {file}", capture.game_name),
                );
            }
            self.toast_info(format!("Capture enregistrée · {}", capture.path.display()));
            ctx.request_repaint();
        }
        if self.running.is_empty() && self.game_overlay.target.is_some() {
            self.game_overlay.clear_game();
        }
    }

    /// Annonce un succès dans l'overlay et, pour Epic/GOG, avec le son et la
    /// notification du client de la boutique.
    pub(crate) fn announce_achievement(
        &mut self,
        key: &crate::providers::ProviderGameKey,
        title: &str,
    ) {
        let provider = key.provider;
        let label = match provider {
            crate::providers::Provider::Steam => "Steam",
            crate::providers::Provider::Epic => "Epic Games",
            crate::providers::Provider::Gog => "GOG",
        };
        self.game_overlay
            .notice(format!("Succès {label}"), format!("{title} · débloqué"));
        if provider != crate::providers::Provider::Steam {
            crate::overlay::play_chime();
            crate::overlay::desktop_notification(&format!("Succès {label}"), title);
        }
    }

    /// Vérifie régulièrement les dossiers installés sans interroger les
    /// boutiques : un jeu supprimé à la main repasse aussitôt en « non
    /// installé » au lieu d'afficher un bouton Jouer mort.
    fn poll_install_probe(&mut self, ctx: &egui::Context) {
        if let Some(result) = self.install_probe_rx.as_ref().map(Receiver::try_recv) {
            match result {
                Ok(missing) => {
                    self.install_probe_rx = None;
                    for key in missing {
                        let title = if let Some(record) = self
                            .lib
                            .provider_library
                            .games
                            .iter_mut()
                            .find(|game| game.key == key)
                        {
                            let title = record.title.clone();
                            record.install_state = crate::providers::InstallState::NotInstalled;
                            record.install_path = None;
                            title
                        } else {
                            continue;
                        };
                        self.toast_info(format!(
                            "{title} n'est plus installé : marqué comme désinstallé"
                        ));
                        self.touch();
                    }
                }
                Err(TryRecvError::Disconnected) => self.install_probe_rx = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        if self.install_probe_rx.is_some()
            || self
                .install_probe_at
                .is_some_and(|probe| probe.elapsed() < Duration::from_secs(5))
        {
            return;
        }
        self.install_probe_at = Some(Instant::now());
        let busy = self
            .provider_install_jobs
            .iter()
            .map(|job| job.key.clone())
            .collect::<Vec<_>>();
        let installed: Vec<(crate::providers::ProviderGameKey, PathBuf)> = self
            .lib
            .provider_library
            .games
            .iter()
            .filter(|game| game.install_state == crate::providers::InstallState::Installed)
            .filter(|game| !busy.contains(&game.key))
            .filter_map(|game| {
                game.install_path
                    .clone()
                    .filter(|path| path.is_absolute())
                    .map(|path| (game.key.clone(), path))
            })
            .collect();
        if installed.is_empty() {
            return;
        }
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let missing = installed
                .into_iter()
                .filter(|(_, path)| !path.is_dir())
                .map(|(key, _)| key)
                .collect::<Vec<_>>();
            let _ = tx.send(missing);
            repaint.request_repaint();
        });
        self.install_probe_rx = Some(rx);
    }

    /// Demande la désinstallation d'un jeu de boutique installé : la
    /// confirmation est affichée par `ui_dialogs`, puis le dossier est supprimé.
    pub(crate) fn request_uninstall(&mut self, key: &crate::providers::ProviderGameKey) {
        let installed = self.lib.provider_library.games.iter().any(|game| {
            &game.key == key && game.install_state == crate::providers::InstallState::Installed
        });
        if installed {
            self.confirm_uninstall = Some(key.clone());
        } else {
            self.toast_info("Ce jeu n'est pas installé");
        }
    }

    pub(crate) fn start_uninstall(
        &mut self,
        ctx: &egui::Context,
        key: &crate::providers::ProviderGameKey,
    ) {
        if self.uninstall_rx.is_some() {
            if !self.uninstall_queue.contains(key) {
                self.uninstall_queue.push_back(key.clone());
            }
            self.toast_info("Désinstallation ajoutée à la file");
            return;
        }
        let Some(record) = self
            .lib
            .provider_library
            .games
            .iter()
            .find(|game| &game.key == key)
            .cloned()
        else {
            return;
        };
        let Some(path) = record.install_path.clone() else {
            self.toast_error("Dossier d'installation inconnu");
            return;
        };
        let Some(root) = record.install_root.clone() else {
            self.toast_error(
                "Désinstallation refusée : la racine d'installation n'est pas connue pour ce jeu.",
            );
            return;
        };
        let title = record.title.clone();
        let sent = key.clone();
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = crate::providers::operations::remove_install_directory(&path, &root);
            let _ = tx.send((sent, result));
            repaint.request_repaint();
        });
        self.uninstall_rx = Some(rx);
        self.toast_info(format!("Désinstallation de {title}…"));
        ctx.request_repaint_after(Duration::from_millis(150));
    }

    fn start_next_uninstall(&mut self, ctx: &egui::Context) {
        if let Some(next) = self.uninstall_queue.pop_front() {
            self.start_uninstall(ctx, &next);
        }
    }

    fn poll_uninstall(&mut self, ctx: &egui::Context) {
        let Some(result) = self.uninstall_rx.as_ref().map(Receiver::try_recv) else {
            return;
        };
        match result {
            Ok((key, Ok(()))) => {
                self.uninstall_rx = None;
                let title = self
                    .lib
                    .provider_library
                    .games
                    .iter()
                    .find(|game| game.key == key)
                    .map(|game| game.title.clone())
                    .unwrap_or_else(|| key.product_id.clone());
                if let Some(record) = self
                    .lib
                    .provider_library
                    .games
                    .iter_mut()
                    .find(|game| game.key == key)
                {
                    record.install_state = crate::providers::InstallState::NotInstalled;
                    record.install_path = None;
                }
                self.game_overlay
                    .notice("Désinstallation", format!("{title} supprimé du disque"));
                self.toast_info(format!("{title} désinstallé"));
                self.touch();
                self.start_next_uninstall(ctx);
            }
            Ok((key, Err(error))) => {
                self.uninstall_rx = None;
                let title = self
                    .lib
                    .provider_library
                    .games
                    .iter()
                    .find(|game| game.key == key)
                    .map(|game| game.title.clone())
                    .unwrap_or_else(|| key.product_id.clone());
                self.toast_error(format!("Désinstallation de {title} impossible : {error}"));
            }
            Err(TryRecvError::Disconnected) => {
                self.uninstall_rx = None;
                self.start_next_uninstall(ctx);
            }
            Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(150)),
        }
    }

    pub fn running_since(&self, id: &str) -> u64 {
        self.running.get(id).map(|r| r.session).unwrap_or(0)
    }

    // -----------------------------------------------------------------------
    // Mises à jour (releases GitHub)
    // -----------------------------------------------------------------------

    /// Vérifie au démarrage si une release plus récente doit être installée.
    fn maybe_startup_update(&mut self, ctx: &egui::Context) {
        if self.startup_update_done {
            return;
        }
        self.startup_update_done = true;
        if !self.lib.settings.auto_update || self.snapshot_mode {
            return;
        }
        if !update::automatic_update_allowed() {
            return;
        }
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        // Dossier en lecture seule (paquet système) : ne télécharge pas 11 Mo
        // pour échouer ensuite. L'utilisateur peut toujours vérifier à la main.
        if update::can_replace(&exe).is_err() {
            return;
        }
        self.update_auto = true;
        self.check_updates(ctx, true);
    }

    /// Interroge la dernière release publiée (non bloquant).
    pub fn check_updates(&mut self, ctx: &egui::Context, auto: bool) {
        if matches!(
            self.update,
            UpdateState::Checking | UpdateState::Downloading(_) | UpdateState::Installing(_)
        ) {
            return;
        }
        self.update_auto = auto;
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

    /// Télécharge, vérifie et prépare la release proposée, puis remplace le
    /// binaire courant et relance Quark.
    pub fn install_update(&mut self, ctx: &egui::Context) {
        let UpdateState::Available(release) = self.update.clone() else {
            return;
        };
        if !self.running.is_empty() {
            self.update = UpdateState::Failed(
                "ferme le jeu en cours avant d'installer la mise à jour".to_string(),
            );
            self.update_auto = false;
            return;
        }
        if let Ok(exe) = std::env::current_exe()
            && let Err(err) = update::can_replace(&exe)
        {
            self.update = UpdateState::Failed(err);
            self.update_auto = false;
            return;
        }
        let Some(asset) = release.archive().cloned() else {
            self.update = UpdateState::Failed("aucune archive dans cette release".to_string());
            self.update_auto = false;
            return;
        };
        let tag = release.tag_name.clone();
        let download_release = release.clone();
        self.update = UpdateState::Downloading(release);
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        let dir = crate::config::data_dir().join("updates");
        std::thread::spawn(move || {
            let client = artwork::http();
            let target = match std::env::current_exe() {
                Ok(target) => target,
                Err(err) => {
                    let _ = tx.send(UpdateMessage::Staged(
                        tag,
                        Err(format!("chemin du binaire : {err}")),
                    ));
                    ctx.request_repaint();
                    return;
                }
            };
            let result = update::stage(&client, &download_release, &asset, &dir, &target);
            let _ = tx.send(UpdateMessage::Staged(tag, result));
            ctx.request_repaint();
        });
        self.update_rx = Some(rx);
    }

    fn poll_update(&mut self, ctx: &egui::Context) {
        let received = self.update_rx.as_ref().map(|rx| rx.try_recv());
        match received {
            Some(Ok(UpdateMessage::Checked(Ok(release)))) => {
                self.update_rx = None;
                if update::is_newer(&release.tag_name, update::current_version()) {
                    self.update = UpdateState::Available(release);
                    if self.update_auto {
                        self.install_update(ctx);
                    }
                } else {
                    self.update = UpdateState::UpToDate;
                    self.update_auto = false;
                }
            }
            Some(Ok(UpdateMessage::Checked(Err(err)))) => {
                self.update_rx = None;
                if !self.update_auto {
                    self.toast_error(format!("Vérification des mises à jour impossible : {err}"));
                }
                self.update = UpdateState::Failed(err);
                self.update_auto = false;
            }
            Some(Ok(UpdateMessage::Staged(tag, Ok(staged)))) => {
                self.update_rx = None;
                let automatic = self.update_auto;
                self.update = UpdateState::Installing(tag);
                if let Err(err) = self.finish_update(staged) {
                    if !automatic {
                        self.toast_error(format!("Mise à jour impossible : {err}"));
                    }
                    self.update = UpdateState::Failed(err);
                    self.update_auto = false;
                }
            }
            Some(Ok(UpdateMessage::Staged(_, Err(err)))) => {
                self.update_rx = None;
                if !self.update_auto {
                    self.toast_error(format!("Mise à jour impossible : {err}"));
                }
                self.update = UpdateState::Failed(err);
                self.update_auto = false;
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.update_rx = None;
                if !self.update_auto {
                    self.toast_error("Vérification des mises à jour interrompue");
                }
                self.update = UpdateState::Failed("vérification interrompue".to_string());
                self.update_auto = false;
            }
            _ => {
                if matches!(
                    self.update,
                    UpdateState::Checking
                        | UpdateState::Downloading(_)
                        | UpdateState::Installing(_)
                ) {
                    ctx.request_repaint_after(Duration::from_millis(150));
                }
            }
        }
    }

    /// Remplace le binaire et relance le processus. `exec` ne revient qu'en cas
    /// d'échec ; on restaure alors l'ancien binaire et l'icône de notification.
    fn finish_update(&mut self, staged: update::StagedUpdate) -> Result<(), String> {
        if !self.running.is_empty() {
            return Err("ferme le jeu en cours avant d'installer la mise à jour".to_string());
        }
        if self.snapshot_mode {
            return Err("mise à jour désactivée en rendu hors écran".to_string());
        }
        if self.dirty {
            self.lib
                .save()
                .map_err(|err| format!("la bibliothèque n'a pas pu être enregistrée ({err})"))?;
            self.dirty = false;
        }
        // L'icône tourne encore avec l'ancien inode : on l'arrête avant le
        // renommage, puis le nouveau processus la relancera. Sans cette pause,
        // le nouveau helper peut perdre la course contre l'ancien verrou.
        let tray_enabled = self.lib.settings.tray_icon;
        if tray_enabled {
            crate::tray::stop_helper();
            std::thread::sleep(Duration::from_millis(150));
        }
        let installed = match update::apply(&staged) {
            Ok(installed) => installed,
            Err(err) => {
                if tray_enabled {
                    self.tray_error = crate::tray::ensure_helper().err();
                }
                return Err(err);
            }
        };
        let _ = std::fs::remove_file(&staged.archive);
        crate::tray::clear_gui_pid();
        if let Err(err) = update::relaunch(&installed.target) {
            let restored = update::rollback(&installed);
            crate::tray::write_gui_pid();
            if tray_enabled {
                self.tray_error = crate::tray::ensure_helper().err();
            }
            return Err(match restored {
                Ok(()) => err,
                Err(rollback) => format!("{err} · {rollback}"),
            });
        }
        Ok(())
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
        if !self.snapshot_mode && self.dirty {
            if let Err(err) = self.lib.save() {
                let message = format!(
                    "Quitter annulé : la bibliothèque n'a pas pu être enregistrée ({err})."
                );
                self.config_error = Some(message.clone());
                self.toast_error(message);
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                return;
            }
            self.dirty = false;
        }
        self.quitting = true;
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

    /// Télécharge l'overlay Epic (EOS) une fois, puis l'active dans le préfixe
    /// Proton du jeu. L'opération est longue (plusieurs centaines de Mo), donc
    /// elle tourne en arrière-plan et l'interface suit son avancement.
    pub fn install_eos_overlay(&mut self, ctx: &egui::Context, game_id: &str) {
        if self.eos_overlay_job.is_some() {
            self.toast_info("L'overlay Epic est déjà en cours d'installation");
            return;
        }
        let Some(game) = self.lib.get(game_id).cloned() else {
            return;
        };
        let Some(record) = self.provider_record_for_game(game_id).cloned() else {
            self.toast_error("Overlay Epic : jeu de boutique requis");
            return;
        };
        if record.key.provider != crate::providers::Provider::Epic {
            self.toast_error("L'overlay EOS ne concerne que les jeux Epic");
            return;
        }
        let wine_prefix = game.wine_prefix_path(&self.lib.settings);
        self.start_eos_overlay_job(ctx, Some(wine_prefix));
    }

    /// Installation globale, sans préfixe : l'activation se fera au prochain
    /// lancement d'un jeu Epic. Utile depuis les paramètres.
    pub fn install_eos_overlay_global(&mut self, ctx: &egui::Context) {
        self.start_eos_overlay_job(ctx, None);
    }

    fn start_eos_overlay_job(&mut self, ctx: &egui::Context, wine_prefix: Option<PathBuf>) {
        if self.eos_overlay_job.is_some() {
            self.toast_info("L'overlay Epic est déjà en cours d'installation");
            return;
        }
        let progress = Arc::new(Mutex::new(crate::providers::runner::InstallProgress {
            stage: "Overlay Epic · préparation…".to_string(),
            ..Default::default()
        }));
        let job_progress = progress.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let job_cancel = cancel.clone();
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let adapter = crate::providers::legendary::LegendaryAdapter::new();
            let result = (|| {
                let installed =
                    match crate::providers::legendary::LegendaryAdapter::eos_overlay_installed_path(
                    ) {
                        Some(path) => path,
                        None => adapter.install_eos_overlay_streaming(
                            job_progress.clone(),
                            Some(job_cancel),
                        )?,
                    };
                let enabled = match wine_prefix {
                    Some(wine_prefix) if wine_prefix.join("user.reg").is_file() => {
                        adapter.enable_eos_overlay(&wine_prefix).map_err(|error| {
                            format!("overlay installé, mais activation impossible : {error}")
                        })?;
                        Some(wine_prefix)
                    }
                    _ => None,
                };
                Ok((installed, enabled))
            })();
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.eos_overlay_job = Some(EosOverlayJob {
            progress,
            cancel,
            rx,
        });
        self.toast_info("Téléchargement de l'overlay Epic (EOS)…");
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    fn poll_eos_overlay(&mut self, ctx: &egui::Context) {
        let Some(result) = self.eos_overlay_job.as_ref().map(|job| job.rx.try_recv()) else {
            return;
        };
        match result {
            Ok(Ok((installed, enabled))) => {
                self.eos_overlay_job = None;
                let suffix = enabled
                    .as_ref()
                    .map(|prefix| format!(" · activé dans {}", prefix.display()))
                    .unwrap_or_else(|| {
                        " · sera activé au prochain lancement, une fois le préfixe initialisé"
                            .to_string()
                    });
                self.toast_info(format!(
                    "Overlay Epic installé dans {}{suffix}",
                    installed.display()
                ));
            }
            Ok(Err(error)) => {
                self.eos_overlay_job = None;
                self.toast_error(format!("Overlay Epic : {error}"));
            }
            Err(TryRecvError::Disconnected) => {
                self.eos_overlay_job = None;
                self.toast_error("Overlay Epic : installation interrompue");
            }
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(160));
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
        let wine_prefix = game
            .quark_game_id
            .as_deref()
            .and_then(|game_id| self.lib.get(game_id))
            .map(|linked| linked.wine_prefix_path(&self.lib.settings))
            .filter(|prefix| prefix.join("drive_c").is_dir());
        Ok(crate::saves::SaveIdentity {
            provider_key: game.key.clone(),
            install_path: Some(install_path),
            wine_prefix,
            ludusavi_title: self.ludusavi_title(&game.key, title),
            custom_paths: game.custom_save_paths.clone(),
        })
    }

    /// Deux installations du même titre (Steam + Epic, ou deux copies) ne
    /// doivent pas partager l'entrée `customGames` de Ludusavi. La première
    /// garde le titre historique ; les suivantes reçoivent un suffixe stable.
    fn ludusavi_title(&self, key: &crate::providers::ProviderGameKey, title: &str) -> String {
        let normalized = crate::providers::operations::normalize_title(title);
        let mut matching = self.lib.provider_library.games.iter().filter(|game| {
            game.install_state == crate::providers::InstallState::Installed
                && crate::providers::operations::normalize_title(&game.title) == normalized
        });
        let Some(first) = matching.next() else {
            return title.to_string();
        };
        if matching.next().is_none() || first.key == *key {
            // La première installation garde le titre historique, donc les
            // sauvegardes existantes restent accessibles.
            return title.to_string();
        }
        let provider = match key.provider {
            crate::providers::Provider::Steam => "steam",
            crate::providers::Provider::Epic => "epic",
            crate::providers::Provider::Gog => "gog",
        };
        format!("{title} [{provider}:{}]", key.product_id)
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
        let wine_prefix = self
            .lib
            .get(game_id)
            .map(|linked| linked.wine_prefix_path(&self.lib.settings))
            .filter(|prefix| prefix.join("drive_c").is_dir());
        Some(crate::saves::SaveIdentity {
            provider_key: provider.key.clone(),
            install_path: provider.install_path.clone(),
            wine_prefix,
            ludusavi_title: self.ludusavi_title(&provider.key, &provider.title),
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
                ui.label(crate::i18n::rich(format!("Aperçu de sauvegarde · {title}")).size(17.0).color(pal.text));
                ui.add_space(8.0);
                let changed = preview.changed_games.map_or_else(|| "—".to_string(), |value| value.to_string());
                let processed = preview.processed_games.map_or_else(|| "—".to_string(), |value| value.to_string());
                let bytes = preview.processed_bytes.map_or_else(|| "—".to_string(), |value| value.to_string());
                ui.label(crate::i18n::rich(format!("Jeux modifiés : {changed} · traités : {processed} · données : {bytes} octets")).size(12.0).color(pal.text_dim));
                if !preview.errors.is_empty() {
                    ui.add_space(8.0);
                    ui.label(crate::i18n::rich("Ludusavi a signalé une erreur; aucune sauvegarde ne sera déclarée réussie.").size(12.0).color(pal.danger));
                }
                if preview.files.iter().any(|file| file.failed) {
                    ui.label(crate::i18n::rich("Certains fichiers ont échoué; vérifie les détails via Ludusavi.").size(11.5).color(pal.danger));
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let can_backup = preview.errors.is_empty() && !preview.files.iter().any(|file| file.failed);
                    if ui.add_enabled(can_backup, egui::Button::new(crate::i18n::tr("Continuer vers la confirmation"))).clicked() { begin_confirmation = true; }
                    if ui.button(crate::i18n::tr("Fermer")).clicked() { close = true; }
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
            ui.label(crate::i18n::rich("Confirmer la sauvegarde locale ?").size(17.0).color(pal.text));
            ui.add_space(8.0);
            ui.label(crate::i18n::rich(format!("Ludusavi va écrire les données de {confirm_title} dans le coffre local de cette installation. Les sauvegardes sont limitées à 10 versions complètes sans différentielles; les anciennes sauvegardes au-delà de la limite peuvent être supprimées par Ludusavi.")).size(12.0).color(pal.text_dim));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button(crate::i18n::tr("Créer la sauvegarde")).clicked() { confirm = true; }
                if ui.button(crate::i18n::tr("Annuler")).clicked() { cancel = true; }
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
                    crate::i18n::rich(format!("Sauvegardes locales · {title}"))
                        .size(17.0)
                        .color(pal.text),
                );
                ui.add_space(8.0);
                if backups.is_empty() {
                    ui.label(
                        crate::i18n::rich(
                            "Aucune sauvegarde dans le coffre de cette installation.",
                        )
                        .size(12.0)
                        .color(pal.text_dim),
                    );
                }
                for backup in &backups {
                    ui.horizontal(|ui| {
                        ui.label(crate::i18n::rich(&backup.when).size(12.0).color(pal.text));
                        if backup.locked {
                            ui.label(crate::i18n::rich("Protégée").size(10.0).color(pal.accent));
                        }
                        if ui.small_button("Aperçu restauration…").clicked() {
                            selected = Some(backup.clone());
                        }
                    });
                }
                if ui.button(crate::i18n::tr("Fermer")).clicked() {
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
                    crate::i18n::rich(format!("Aperçu de restauration · {title}"))
                        .size(17.0)
                        .color(pal.text),
                );
                ui.label(
                    crate::i18n::rich(format!(
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
                            ui.label(crate::i18n::rich(&file.path).size(10.5).color(pal.text_dim));
                        }
                    });
                if !preview.errors.is_empty() || preview.files.iter().any(|file| file.failed) {
                    ui.label(
                        crate::i18n::rich(
                            "L'aperçu contient des erreurs; restauration désactivée.",
                        )
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
                            egui::Button::new(crate::i18n::tr(
                                "Restaurer et remplacer les fichiers concernés…",
                            )),
                        )
                        .clicked()
                    {
                        confirm = true;
                    }
                    if ui.button(crate::i18n::tr("Fermer")).clicked() {
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
                ui.label(crate::i18n::rich("Confirmer le remplacement des sauvegardes ?").size(17.0).color(pal.danger));
                ui.label(crate::i18n::rich(format!("Les fichiers de {title} seront restaurés depuis le snapshot {backup_id}. Les fichiers présents aux destinations pourront être écrasés.")).size(12.0).color(pal.text_dim));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(crate::i18n::tr("Confirmer la restauration")).clicked() { confirm = true; }
                    if ui.button(crate::i18n::tr("Annuler")).clicked() { cancel = true; }
                });
            });
            if confirm {
                self.confirm_save_restore(ctx);
            } else if cancel {
                self.save_restore_confirm = None;
            }
        }
    }

    /// Ouvre la recherche d'un jeu de la boutique sur les cinq sites de
    /// téléchargement. Les résultats arrivent séparément, source par source.
    pub fn open_site_search(&mut self, ctx: &egui::Context, item: &crate::steam_store::StoreItem) {
        let query = item.name.trim().to_string();
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        for site in crate::game_search::SearchSite::ALL {
            let tx = tx.clone();
            let query = query.clone();
            let repaint = repaint.clone();
            std::thread::spawn(move || {
                let result = crate::game_search::search(site, &query);
                let _ = tx.send((site, result));
                repaint.request_repaint();
            });
        }
        drop(tx);
        self.site_search = Some(SiteSearchJob {
            title: item.name.clone(),
            query,
            steam_url: format!("https://store.steampowered.com/app/{}", item.app_id),
            states: crate::game_search::SearchSite::ALL
                .iter()
                .copied()
                .map(|site| (site, SiteSearchState::Pending))
                .collect(),
            pending: crate::game_search::SearchSite::ALL.len(),
            rx,
        });
        ctx.request_repaint();
    }

    fn poll_site_search(&mut self, ctx: &egui::Context) {
        let mut updates = Vec::new();
        if let Some(job) = self.site_search.as_mut() {
            loop {
                match job.rx.try_recv() {
                    Ok((site, result)) => {
                        job.pending = job.pending.saturating_sub(1);
                        updates.push((site, result));
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        if job.pending > 0 {
                            for (_, state) in &mut job.states {
                                if matches!(state, SiteSearchState::Pending) {
                                    *state = SiteSearchState::Failed(
                                        "recherche interrompue".to_string(),
                                    );
                                }
                            }
                            job.pending = 0;
                        }
                        break;
                    }
                }
            }
            for (site, result) in updates {
                if let Some((_, state)) = job.states.iter_mut().find(|(known, _)| *known == site) {
                    *state = match result {
                        Ok(hits) => SiteSearchState::Ready(hits),
                        Err(error) => SiteSearchState::Failed(error),
                    };
                }
            }
            if job.pending > 0 {
                ctx.request_repaint_after(Duration::from_millis(150));
            }
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

    /// Ouvre la petite fenêtre d'installation d'un jeu Epic/GOG. Elle affiche
    /// la taille annoncée par l'outil et le dossier parent proposé, puis laisse
    /// changer ce dossier avant de lancer le téléchargement.
    pub(crate) fn open_install_request(
        &mut self,
        ctx: &egui::Context,
        key: &crate::providers::ProviderGameKey,
    ) {
        if self.provider_install_jobs.iter().any(|job| job.key == *key) {
            self.toast_info("Cette installation est déjà en cours");
            return;
        }
        if self.provider_install_jobs.len() >= MAX_CONCURRENT_INSTALLS {
            self.toast_info(format!(
                "Maximum de {MAX_CONCURRENT_INSTALLS} téléchargements simultanés"
            ));
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
        if !game.owned {
            self.toast_error("Ce jeu n'est pas possédé sur cette boutique");
            return;
        }
        let ready = match key.provider {
            crate::providers::Provider::Epic => crate::providers::tools::ManagedToolId::Legendary
                .installed_path()
                .is_file(),
            crate::providers::Provider::Gog => crate::providers::tools::ManagedToolId::Gogdl
                .installed_path()
                .is_file(),
            crate::providers::Provider::Steam => false,
        };
        if !ready {
            let tool = if key.provider == crate::providers::Provider::Epic {
                "Legendary"
            } else {
                "gogdl"
            };
            self.toast_error(format!(
                "{tool} n'est pas installé : Paramètres → Boutiques & outils"
            ));
            return;
        }
        let destination = self.lib.settings.install_root();
        let (tx, rx) = channel();
        let sent_key = key.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = match sent_key.provider {
                crate::providers::Provider::Epic => {
                    crate::providers::legendary::epic_install_size(&sent_key.product_id)
                }
                crate::providers::Provider::Gog => {
                    crate::providers::gog::gog_install_size(&sent_key.product_id)
                }
                crate::providers::Provider::Steam => Ok(None),
            };
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.install_request = Some(InstallRequest {
            key: key.clone(),
            title: game.title,
            destination,
            size: None,
            size_error: None,
            size_rx: Some(rx),
        });
        ctx.request_repaint_after(Duration::from_millis(150));
    }

    /// Lit la taille demandée par la fenêtre d'installation, sans bloquer le
    /// rendu : l'info de Legendary/gogdl peut prendre quelques secondes.
    fn poll_install_size(&mut self, ctx: &egui::Context) {
        let Some(received) = self
            .install_request
            .as_ref()
            .and_then(|request| request.size_rx.as_ref())
            .map(Receiver::try_recv)
        else {
            return;
        };
        match received {
            Ok(result) => {
                if let Some(request) = self.install_request.as_mut() {
                    request.size_rx = None;
                    match result {
                        Ok(size) => request.size = size,
                        Err(error) => request.size_error = Some(error),
                    }
                }
            }
            Err(TryRecvError::Disconnected) => {
                if let Some(request) = self.install_request.as_mut() {
                    request.size_rx = None;
                    request.size_error = Some("lecture de la taille interrompue".to_string());
                }
            }
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(150));
            }
        }
    }

    /// Fenêtre de confirmation d'installation Epic/GOG, ouverte par un clic
    /// gauche ou droit sur une carte de « Mes jeux ».
    pub(crate) fn install_dialog(&mut self, ctx: &egui::Context) {
        if self.install_request.is_none() {
            return;
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.install_request = None;
            return;
        }
        let pal = self.pal;
        let mut change = false;
        let mut install = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("quark_install_request")).show(ctx, |ui| {
            let Some(request) = self.install_request.as_ref() else {
                return;
            };
            ui.set_width(560.0);
            ui.label(
                crate::i18n::rich(format!("Installer « {} »", request.title))
                    .size(17.0)
                    .color(pal.text),
            );
            ui.add_space(4.0);
            ui.label(
                crate::i18n::rich(match request.key.provider {
                    crate::providers::Provider::Epic => "Epic Games · via Legendary",
                    crate::providers::Provider::Gog => "GOG · via gogdl",
                    crate::providers::Provider::Steam => "Steam",
                })
                .size(11.5)
                .color(pal.text_dim),
            );
            ui.add_space(14.0);
            let size_text = if request.size_rx.is_some() {
                "Taille : calcul en cours…".to_string()
            } else if let Some(size) = request.size {
                format!(
                    "Taille à télécharger : {}",
                    disk::format_bytes(size, crate::i18n::is_english())
                )
            } else if let Some(error) = &request.size_error {
                format!("Taille non communiquée ({error})")
            } else {
                "Taille non communiquée par la boutique".to_string()
            };
            ui.label(crate::i18n::rich(size_text).size(12.5).color(pal.text));
            ui.add_space(8.0);
            ui.label(
                crate::i18n::rich("Dossier d'installation")
                    .size(10.5)
                    .color(pal.text_faint),
            );
            ui.horizontal(|ui| {
                ui.add(
                    egui::Label::new(
                        crate::i18n::rich(request.destination.display().to_string())
                            .size(11.5)
                            .color(pal.text_dim),
                    )
                    .truncate(),
                );
                if widgets::ghost_button(ui, "Changer…", &pal, Icon::Folder).clicked() {
                    change = true;
                }
            });
            ui.add_space(10.0);
            ui.label(
                crate::i18n::rich(
                    "Le jeu sera créé dans ce dossier. Les sauvegardes et le préfixe Proton \
                     restent conservés en cas de désinstallation.",
                )
                .size(10.5)
                .color(pal.text_faint),
            );
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if widgets::accent_button(
                    ui,
                    Vec2::new(112.0, 30.0),
                    "Installer",
                    &pal,
                    true,
                    Icon::Download,
                )
                .clicked()
                {
                    install = true;
                }
                if widgets::ghost_button(ui, "Annuler", &pal, Icon::None).clicked() {
                    cancel = true;
                }
            });
        });
        if modal.should_close() {
            cancel = true;
        }
        if change {
            let start = self
                .install_request
                .as_ref()
                .map(|request| request.destination.clone())
                .unwrap_or_else(|| self.lib.settings.install_root());
            self.pick_directory(ctx, start, PickerTarget::InstallDestination);
            return;
        }
        if cancel {
            self.install_request = None;
            return;
        }
        if install {
            let Some(request) = self.install_request.take() else {
                return;
            };
            match request.key.provider {
                crate::providers::Provider::Epic => {
                    self.epic_install_pending = Some(request.key.clone());
                    self.start_epic_install(ctx, request.destination);
                }
                crate::providers::Provider::Gog => {
                    self.gog_install_pending = Some(request.key.clone());
                    self.start_gog_install(ctx, request.destination);
                }
                crate::providers::Provider::Steam => {}
            }
        }
    }

    /// Ouvre la confirmation d'installation Epic : taille et dossier avant de
    /// lancer Legendary. Le dossier proposé vient des réglages.
    pub fn request_epic_install(
        &mut self,
        ctx: &egui::Context,
        key: &crate::providers::ProviderGameKey,
    ) {
        self.open_install_request(ctx, key);
    }

    fn start_epic_install(&mut self, ctx: &egui::Context, destination: PathBuf) {
        let Some(key) = self.epic_install_pending.take() else {
            return;
        };
        if self.provider_install_jobs.iter().any(|job| job.key == key) {
            self.toast_info("Cette installation est déjà en cours");
            return;
        }
        if self.provider_install_jobs.len() >= MAX_CONCURRENT_INSTALLS {
            self.toast_info(format!(
                "Maximum de {MAX_CONCURRENT_INSTALLS} téléchargements simultanés"
            ));
            return;
        }
        if let Err(error) = std::fs::create_dir_all(&destination) {
            self.toast_error(format!("Dossier de destination Epic invalide : {error}"));
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
        let progress = Arc::new(Mutex::new(
            crate::providers::runner::InstallProgress::default(),
        ));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        let job_key = key.clone();
        let job_progress = progress.clone();
        let job_cancel = cancel.clone();
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
                let mut output = adapter.run_install_streaming(
                    crate::providers::legendary::LegendaryAdapter::install_args(
                        &key.product_id,
                        &destination,
                    ),
                    Some(cancel.clone()),
                    progress.clone(),
                )?;
                if output.exit_code != Some(0) {
                    return Err(format!(
                        "Legendary a échoué pendant l'installation ({})",
                        crate::providers::runner::failure_hint(&output)
                    ));
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
        self.provider_install_jobs.push(ProviderInstallJob {
            key: job_key,
            progress: job_progress,
            cancel: job_cancel,
            rx,
        });
        self.toast_info(format!(
            "Installation Epic de {} lancée par Legendary",
            game.title
        ));
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    /// Termine les installations dont le processus a répondu. Les autres
    /// continuent en parallèle : aucune ne remplace l'avancement d'une autre.
    fn poll_provider_installs(&mut self, ctx: &egui::Context) {
        if self.provider_install_jobs.is_empty() {
            return;
        }
        let mut done = Vec::new();
        for (index, job) in self.provider_install_jobs.iter().enumerate() {
            match job.rx.try_recv() {
                Ok((key, result)) => done.push((index, key, result)),
                Err(TryRecvError::Disconnected) => done.push((
                    index,
                    job.key.clone(),
                    Err("installation interrompue".to_string()),
                )),
                Err(TryRecvError::Empty) => {}
            }
        }
        if done.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(120));
            return;
        }
        for (index, key, result) in done.into_iter().rev() {
            if index >= self.provider_install_jobs.len() {
                continue;
            }
            self.provider_install_jobs.remove(index);
            match result {
                Ok(installed) => self.finish_install_success(ctx, key, installed),
                Err(error) => self.finish_install_failure(key, error),
            }
        }
        if !self.provider_install_jobs.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
    }

    fn finish_install_success(
        &mut self,
        ctx: &egui::Context,
        key: crate::providers::ProviderGameKey,
        installed: crate::providers::ProviderGame,
    ) {
        let provider = key.provider;
        let title = installed.title.clone();
        if let Some(record) = self
            .lib
            .provider_library
            .games
            .iter_mut()
            .find(|game| game.key == key)
        {
            if provider == crate::providers::Provider::Epic {
                let group = crate::providers::legendary::merge_epic(
                    std::slice::from_ref(record),
                    std::slice::from_ref(&installed),
                );
                if let Some(updated) = group.into_iter().find(|game| game.key == key) {
                    *record = updated;
                }
            } else {
                record.install_state = crate::providers::InstallState::Installed;
                record.install_path = installed.install_path.clone();
                record.install_exe = installed.install_exe.clone();
            }
        }
        self.touch();
        self.toast_info(format!("{title} installé et détecté localement"));
        self.adopt_provider_install(ctx, &key);
        self.lib.migrate_installations();
        // Récupère immédiatement la sauvegarde cloud : un jeu installé sur une
        // nouvelle machine doit repartir de la progression du compte, comme
        // le ferait le launcher officiel.
        if self.lib.settings.cloud_saves {
            let game_id = self
                .lib
                .provider_library
                .games
                .iter()
                .find(|game| game.key == key)
                .and_then(|game| game.quark_game_id.clone());
            if let Some(game_id) = game_id
                && self.can_auto_sync_cloud(&game_id)
            {
                self.sync_cloud_saves(ctx, &game_id);
            }
        }
    }

    fn finish_install_failure(&mut self, key: crate::providers::ProviderGameKey, error: String) {
        let provider = key.provider;
        let name = match provider {
            crate::providers::Provider::Epic => "Epic",
            crate::providers::Provider::Gog => "GOG",
            crate::providers::Provider::Steam => "Steam",
        };
        let cancelled = error.to_lowercase().contains("annul");
        let mut title = key.product_id.clone();
        if let Some(record) = self
            .lib
            .provider_library
            .games
            .iter_mut()
            .find(|game| game.key == key)
        {
            title = record.title.clone();
            if record
                .install_path
                .as_ref()
                .is_some_and(|path| path.is_dir())
            {
                record.install_state = crate::providers::InstallState::Installed;
            } else {
                record.install_state = if cancelled {
                    crate::providers::InstallState::NotInstalled
                } else if error.contains("interrompue") {
                    crate::providers::InstallState::Unknown
                } else {
                    crate::providers::InstallState::Failed
                };
                record.install_path = None;
            }
        }
        self.touch();
        if cancelled {
            self.toast_info(format!("Installation {name} de {title} annulée"));
        } else {
            self.toast_error(format!(
                "Installation {name} de {title} non confirmée : {error}"
            ));
        }
    }

    /// Installation GOG depuis « Mes jeux » : confirmation avec taille et
    /// dossier, puis gogdl télécharge dans le dossier parent choisi.
    pub fn request_gog_install(
        &mut self,
        ctx: &egui::Context,
        key: &crate::providers::ProviderGameKey,
    ) {
        self.open_install_request(ctx, key);
    }

    fn start_gog_install(&mut self, ctx: &egui::Context, destination: PathBuf) {
        let Some(key) = self.gog_install_pending.take() else {
            return;
        };
        if self.provider_install_jobs.iter().any(|job| job.key == key) {
            self.toast_info("Cette installation est déjà en cours");
            return;
        }
        if self.provider_install_jobs.len() >= MAX_CONCURRENT_INSTALLS {
            self.toast_info(format!(
                "Maximum de {MAX_CONCURRENT_INSTALLS} téléchargements simultanés"
            ));
            return;
        }
        if let Err(error) = std::fs::create_dir_all(&destination) {
            self.toast_error(format!("Dossier de destination GOG invalide : {error}"));
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
        let progress = Arc::new(Mutex::new(
            crate::providers::runner::InstallProgress::default(),
        ));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        let title = game.title.clone();
        let job_key = key.clone();
        let job_progress = progress.clone();
        let job_cancel = cancel.clone();
        std::thread::spawn(move || {
            let adapter =
                crate::providers::gog::GogAdapter::new(crate::providers::gog::auth_config_path());
            let result = (|| {
                std::fs::create_dir_all(&destination)
                    .map_err(|_| "impossible de préparer la destination GOG choisie".to_string())?;
                let mut operation = crate::providers::operations::InstallOperation::new(
                    key.clone(),
                    destination.clone(),
                );
                operation.start();
                // gogdl ajoute lui-même le nom de dossier du jeu : `info` le
                // publie, ce qui évite de chercher au hasard après coup.
                let info = adapter.run(
                    crate::providers::gog::GogAdapter::info_args(&key.product_id, "windows"),
                    None,
                )?;
                let folder = crate::providers::gog::folder_from_info(&info.stdout)?;
                let output = adapter.run_install_streaming(
                    crate::providers::gog::GogAdapter::install_args(
                        &key.product_id,
                        &destination,
                        "windows",
                    ),
                    Some(cancel.clone()),
                    progress.clone(),
                )?;
                if output.exit_code != Some(0) {
                    return Err(format!(
                        "gogdl a échoué pendant l'installation ({})",
                        crate::providers::runner::failure_hint(&output)
                    ));
                }
                let install_path = crate::providers::gog::resolve_install_path(
                    &destination,
                    &folder,
                    &key.product_id,
                )
                .ok_or_else(|| {
                    format!(
                        "gogdl a terminé mais aucun dossier de jeu n'a été trouvé dans {}",
                        destination.display()
                    )
                })?;
                operation.complete_verified(install_path.clone())?;
                let executable = crate::providers::gog::find_executable(&install_path, &title);
                let installed = crate::providers::ProviderGame {
                    key: key.clone(),
                    title,
                    owned: true,
                    install_root: Some(destination.clone()),
                    install_path: Some(install_path),
                    custom_save_paths: Vec::new(),
                    cover_url: None,
                    install_exe: executable,
                    install_state: crate::providers::InstallState::Installed,
                    quark_game_id: None,
                    capabilities: crate::providers::GameCapabilities {
                        achievements: crate::providers::CapabilityState::Supported,
                        ..Default::default()
                    },
                    last_synced: Some(crate::config::now_ts()),
                };
                Ok(installed)
            })();
            let _ = tx.send((key, result));
            repaint.request_repaint();
        });
        self.provider_install_jobs.push(ProviderInstallJob {
            key: job_key,
            progress: job_progress,
            cancel: job_cancel,
            rx,
        });
        self.toast_info(format!(
            "Installation GOG de {} lancée par gogdl",
            game.title
        ));
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    /// Ajoute un jeu de boutique installé à la bibliothèque Quark : c'est ce qui
    /// le fait apparaître dans la grille, avec sa couverture de boutique.
    pub(crate) fn adopt_provider_install(
        &mut self,
        ctx: &egui::Context,
        key: &crate::providers::ProviderGameKey,
    ) {
        let Some(record) = self
            .lib
            .provider_library
            .games
            .iter()
            .find(|game| &game.key == key)
            .cloned()
        else {
            return;
        };
        if record.install_state != crate::providers::InstallState::Installed {
            return;
        }
        if !record
            .install_path
            .as_ref()
            .is_some_and(|path| path.is_dir())
        {
            return;
        }
        if let Some(id) = record
            .quark_game_id
            .clone()
            .filter(|id| self.lib.get(id).is_some())
        {
            // Une fiche liée avant l'arrivée des bannières n'a que sa
            // jaquette : on complète l'image manquante sans la remplacer.
            if self
                .lib
                .get(&id)
                .is_some_and(|game| needs_provider_art(&game.artwork))
            {
                self.start_provider_art(ctx, &record, &id);
            }
            self.selected = Some(id);
            self.touch();
            return;
        }
        // Le jeu peut déjà avoir été ajouté à la main : dans ce cas il est
        // relié au lieu de créer un doublon « installé » et un doublon pointé
        // par l'utilisateur, qui finissaient avec deux fiches différentes.
        //
        // Une fiche Steam du même titre est prioritaire : les images GOG/Epic
        // arrivent ainsi sur la fiche Steam, visible dans « Personnaliser »,
        // au lieu de rester sur une seconde fiche invisible.
        let steam_target = self
            .lib
            .provider_library
            .games
            .iter()
            .find(|candidate| {
                candidate.key.provider == crate::providers::Provider::Steam
                    && candidate
                        .title
                        .trim()
                        .eq_ignore_ascii_case(record.title.trim())
            })
            .and_then(|candidate| candidate.quark_game_id.clone())
            .or_else(|| {
                self.lib
                    .games
                    .iter()
                    .find(|candidate| {
                        candidate.steam_app_id.is_some()
                            && candidate
                                .name
                                .trim()
                                .eq_ignore_ascii_case(record.title.trim())
                    })
                    .map(|candidate| candidate.id.clone())
            })
            .filter(|id| self.lib.get(id).is_some());
        let existing = steam_target.or_else(|| {
            self.lib
                .games
                .iter()
                .find(|candidate| {
                    candidate
                        .name
                        .trim()
                        .eq_ignore_ascii_case(record.title.trim())
                        && (candidate.exe.trim().is_empty()
                            || record
                                .install_path
                                .as_ref()
                                .is_some_and(|path| candidate.exe_path().starts_with(path)))
                })
                .map(|candidate| candidate.id.clone())
        });
        if let Some(id) = existing {
            if let Some(game) = self.lib.get_mut(&id) {
                if game.exe.trim().is_empty() {
                    if let Some(exe) = &record.install_exe {
                        game.exe = exe.display().to_string();
                    }
                }
                if game
                    .artwork
                    .get(AssetKind::Cover)
                    .is_none_or(|path| !path.exists())
                {
                    if let Some(cover) = crate::providers::covers::cached(key) {
                        game.artwork.set(AssetKind::Cover, cover);
                    }
                }
            }
            if let Some(provider) = self
                .lib
                .provider_library
                .games
                .iter_mut()
                .find(|game| &game.key == key)
            {
                provider.quark_game_id = Some(id.clone());
            }
            if self
                .lib
                .get(&id)
                .is_some_and(|game| needs_provider_art(&game.artwork))
            {
                self.start_provider_art(ctx, &record, &id);
            }
            self.selected = Some(id.clone());
            self.toast_info(format!(
                "{} était déjà dans la bibliothèque : la fiche a été reliée à {}",
                record.title,
                match record.key.provider {
                    crate::providers::Provider::Steam => "Steam",
                    crate::providers::Provider::Epic => "Epic Games",
                    crate::providers::Provider::Gog => "GOG",
                }
            ));
            self.lib.migrate_installations();
            self.lib.activate_provider_installation(&id, key);
            self.touch();
            return;
        }
        let settings = self.lib.settings.clone();
        let mut game = Game::new(
            record.title.clone(),
            record
                .install_exe
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
        );
        game.args = settings.default_args.trim().to_string();
        game.prefix = settings
            .default_prefix
            .clone()
            .filter(|prefix| !prefix.trim().is_empty());
        game.anticheat = settings.default_anticheat;
        game.launch = settings.default_launch.clone();
        game.proton_log = settings.default_proton_log;
        game.umu = settings.default_umu;
        // Les variables « par défaut » ne s'appliquent qu'aux jeux ajoutés à la
        // main : un jeu Epic/GOG garde son environnement de boutique.
        game.env = Vec::new();
        if let Some(cover) = crate::providers::covers::cached(key) {
            game.artwork.set(AssetKind::Cover, cover);
        }
        let id = game.id.clone();
        let needs_art = needs_provider_art(&game.artwork);
        let title = game.name.clone();
        self.lib.games.push(game);
        if let Some(provider) = self
            .lib
            .provider_library
            .games
            .iter_mut()
            .find(|game| &game.key == key)
        {
            provider.quark_game_id = Some(id.clone());
        }
        if needs_art {
            self.start_provider_art(ctx, &record, &id);
        }
        self.selected = Some(id.clone());
        self.lib.migrate_installations();
        self.lib.activate_provider_installation(&id, key);
        self.touch();
        if record.install_exe.is_none() {
            self.toast_info(format!(
                "{title} ajouté à la bibliothèque · renseigne son exécutable dans les réglages du jeu"
            ));
        } else {
            self.toast_info(format!("{title} ajouté à la bibliothèque"));
        }
    }

    /// Lance le téléchargement de la jaquette et de la bannière d'un jeu de
    /// boutique. Un seul job à la fois : les images arrivent dans le cache,
    /// puis sont posées sur la fiche qui les attend.
    fn start_provider_art(
        &mut self,
        ctx: &egui::Context,
        record: &crate::providers::ProviderGame,
        game_id: &str,
    ) {
        if self.adopt_cover_job.is_some() {
            return;
        }
        self.provider_art_tried.insert(record.key.clone());
        let key = record.key.clone();
        let record = record.clone();
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        let sent_key = key.clone();
        std::thread::spawn(move || {
            let art = crate::providers::covers::ensure_all(&record);
            let _ = tx.send((art, sent_key));
            repaint.request_repaint();
        });
        self.adopt_cover_job = Some((game_id.to_string(), key, rx));
    }

    /// Complète en arrière-plan les fiches déjà liées à une boutique mais
    /// dépourvues de bannière. Un jeu à la fois, sans requête en boucle.
    pub(crate) fn ensure_provider_artwork(&mut self, ctx: &egui::Context) {
        if self.adopt_cover_job.is_some()
            || self
                .provider_art_scan_at
                .is_some_and(|scan| scan.elapsed() < Duration::from_secs(20))
        {
            return;
        }
        self.provider_art_scan_at = Some(Instant::now());
        let mut candidate = None;
        for record in &self.lib.provider_library.games {
            // Les imports Steam ont déjà leurs images ; ce rattrapage vise les
            // fiches Epic/GOG liées avant l'import de la bannière.
            if !matches!(
                record.key.provider,
                crate::providers::Provider::Epic | crate::providers::Provider::Gog
            ) || record.install_state != crate::providers::InstallState::Installed
                || self.provider_art_tried.contains(&record.key)
            {
                continue;
            }
            let needs = record
                .quark_game_id
                .as_ref()
                .and_then(|id| self.lib.get(id))
                .is_some_and(|game| needs_provider_art(&game.artwork));
            if needs {
                candidate = Some(record.clone());
                break;
            }
        }
        if let Some(record) = candidate {
            if let Some(id) = record.quark_game_id.clone() {
                self.start_provider_art(ctx, &record, &id);
            }
        }
    }

    /// Couvertures manquantes des jeux affichés : téléchargées une par une,
    /// jamais au démarrage, et seulement pour l'onglet visible.
    pub(crate) fn queue_provider_covers(
        &mut self,
        ctx: &egui::Context,
        games: &[crate::providers::ProviderGame],
    ) {
        if self.provider_cover_job.is_some() {
            return;
        }
        if self
            .provider_cover_scan_at
            .is_some_and(|scan| scan.elapsed() < Duration::from_secs(2))
        {
            return;
        }
        self.provider_cover_scan_at = Some(Instant::now());
        let legendary_ready = crate::providers::tools::ManagedToolId::Legendary
            .installed_path()
            .is_file();
        self.provider_cover_queue.clear();
        for game in games {
            if self.provider_cover_queue.len() >= 32 {
                break;
            }
            if crate::providers::covers::cached(&game.key).is_some()
                || self.provider_cover_tried.contains(&game.key)
                || self
                    .provider_cover_failures
                    .get(&game.key)
                    .is_some_and(|failures| *failures >= 3)
            {
                continue;
            }
            // Sans URL connue, Epic (métadonnées Legendary) et GOG (API v2)
            // peuvent encore en fournir une ; Steam la publie à la synchro.
            let resolvable = match game.key.provider {
                crate::providers::Provider::Epic => legendary_ready,
                crate::providers::Provider::Gog => true,
                crate::providers::Provider::Steam => game.cover_url.is_some(),
            };
            if game.cover_url.is_none() && !resolvable {
                continue;
            }
            self.provider_cover_queue.push_back(game.key.clone());
        }
        self.start_next_cover(ctx);
    }

    fn start_next_cover(&mut self, ctx: &egui::Context) {
        if self.provider_cover_job.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        let mut start = 0usize;
        while start < 4 {
            let Some(key) = self.provider_cover_queue.pop_front() else {
                break;
            };
            let Some(game) = self
                .lib
                .provider_library
                .games
                .iter()
                .find(|game| game.key == key)
                .cloned()
            else {
                continue;
            };
            let tx = tx.clone();
            let repaint = repaint.clone();
            std::thread::spawn(move || {
                let result = crate::providers::covers::ensure(&game);
                let _ = tx.send((key, result));
                repaint.request_repaint();
            });
            start += 1;
        }
        drop(tx);
        if start == 0 {
            return;
        }
        self.provider_cover_inflight = start;
        self.provider_cover_job = Some(rx);
        ctx.request_repaint_after(Duration::from_millis(150));
    }

    fn poll_provider_covers(&mut self, ctx: &egui::Context) {
        let Some(result) = self.provider_cover_job.as_ref().map(Receiver::try_recv) else {
            return;
        };
        match result {
            Ok((key, Ok((_path, Some(url))))) => {
                self.provider_cover_tried.insert(key.clone());
                self.provider_cover_failures.remove(&key);
                if let Some(game) = self
                    .lib
                    .provider_library
                    .games
                    .iter_mut()
                    .find(|game| game.key == key)
                {
                    if game.cover_url.as_deref() != Some(url.as_str()) {
                        game.cover_url = Some(url);
                        self.touch();
                    }
                }
            }
            Ok((key, Ok((_path, None)))) => {
                // Couverture déjà connue : rien à mémoriser.
                self.provider_cover_tried.insert(key);
            }
            Ok((key, Err(_))) => {
                // Une couverture manquante laisse le monogramme. Une erreur
                // passagère (réseau, cache en cours d'écriture) est retentée
                // aux balayages suivants, sans boucle infinie.
                let failures = self.provider_cover_failures.entry(key).or_insert(0);
                *failures = failures.saturating_add(1);
            }
            Err(TryRecvError::Disconnected) => {
                self.provider_cover_inflight = 0;
            }
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(150));
                return;
            }
        }
        self.provider_cover_inflight = self.provider_cover_inflight.saturating_sub(1);
        if self.provider_cover_inflight == 0 {
            self.provider_cover_job = None;
            self.start_next_cover(ctx);
        } else {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }

    /// Succès Epic, lus seulement quand la fenêtre est demandée.
    pub(crate) fn request_achievements(
        &mut self,
        ctx: &egui::Context,
        key: &crate::providers::ProviderGameKey,
    ) {
        if matches!(
            self.achievements.get(key),
            Some(AchievementsState::Ready(_)) | Some(AchievementsState::Loading)
        ) {
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
        match game.key.provider {
            crate::providers::Provider::Epic => {
                if !crate::providers::tools::ManagedToolId::Legendary
                    .installed_path()
                    .is_file()
                {
                    self.achievements.insert(
                        key.clone(),
                        AchievementsState::Failed(
                            "Legendary n'est pas installé : Paramètres → Boutiques & outils".into(),
                        ),
                    );
                    return;
                }
            }
            crate::providers::Provider::Gog => {
                if !crate::providers::gog::GogAdapter::is_logged_in() {
                    self.achievements.insert(
                        key.clone(),
                        AchievementsState::Failed(
                            "Session GOG absente : connecte la boutique dans Paramètres → \
                             Boutiques & outils"
                                .into(),
                        ),
                    );
                    return;
                }
            }
            crate::providers::Provider::Steam => {
                self.achievements.insert(
                    key.clone(),
                    AchievementsState::Failed(
                        "Steam n'expose pas de succès lisibles sans sa propre session".into(),
                    ),
                );
                return;
            }
        }
        self.achievements
            .insert(key.clone(), AchievementsState::Loading);
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        let product_id = key.product_id.clone();
        let sent_key = key.clone();
        let provider = key.provider;
        std::thread::spawn(move || {
            let result = match provider {
                crate::providers::Provider::Epic => {
                    crate::providers::legendary::epic_achievements(&product_id)
                }
                crate::providers::Provider::Gog => {
                    crate::providers::gog_library::game_achievements(
                        &crate::providers::gog::auth_config_path(),
                        &product_id,
                        crate::config::now_ts(),
                    )
                }
                crate::providers::Provider::Steam => {
                    Err("Steam n'expose pas de succès lisibles".to_string())
                }
            }
            .map(|mut info| {
                crate::providers::achievements::cache_icons(&mut info, provider, &product_id);
                info
            });
            let _ = tx.send((sent_key, result));
            repaint.request_repaint();
        });
        self.achievements_job = Some(rx);
        ctx.request_repaint_after(Duration::from_millis(150));
    }

    pub(crate) fn cancel_provider_install(&mut self, key: &crate::providers::ProviderGameKey) {
        let Some(job) = self
            .provider_install_jobs
            .iter()
            .find(|job| &job.key == key)
        else {
            return;
        };
        job.cancel.store(true, Ordering::SeqCst);
        self.toast_info("Annulation demandée");
    }

    fn poll_achievements(&mut self, ctx: &egui::Context) {
        let Some(result) = self.achievements_job.as_ref().map(Receiver::try_recv) else {
            return;
        };
        match result {
            Ok((key, Ok(info))) => {
                self.achievements
                    .insert(key, AchievementsState::Ready(info));
                self.achievements_job = None;
            }
            Ok((key, Err(error))) => {
                self.achievements
                    .insert(key, AchievementsState::Failed(error));
                self.achievements_job = None;
            }
            Err(TryRecvError::Disconnected) => {
                self.achievements_job = None;
            }
            Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(150)),
        }
    }

    /// Cherche la fiche RetroAchievements d'un jeu PS2 par son titre.
    pub fn start_ps2_ra_search(&mut self, ctx: &egui::Context, game_id: &str, title: &str) {
        if self.ps2_search_job.is_some() {
            self.toast_info("Une recherche RetroAchievements est déjà en cours");
            return;
        }
        let settings = self.lib.settings.clone();
        if !crate::ps2::ra_configured(&settings) {
            self.toast_error(
                "Renseigne le pseudo et la clé Web API RetroAchievements dans les paramètres",
            );
            return;
        }
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        let title = title.trim().to_string();
        std::thread::spawn(move || {
            let client = artwork::http();
            let result = crate::ps2::search_ra_game(&client, &settings, &title);
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.ps2_search_job = Some((game_id.to_string(), rx));
        ctx.request_repaint_after(Duration::from_millis(150));
    }

    /// Progression RetroAchievements connue d'un jeu PS2, pour les badges de
    /// liste. Un jeu sans fiche RA, ou dont la lecture a échoué, n'affiche rien.
    pub(crate) fn ps2_progress(&self, game: &Game) -> Option<Ps2Progress> {
        game.ps2.as_ref()?.ra_game_id?;
        match self.ps2_ra.get(&game.id) {
            Some(AchievementsState::Ready(info)) if info.total > 0 => Some(Ps2Progress {
                unlocked: info.unlocked.min(info.total),
                total: info.total,
            }),
            _ => None,
        }
    }

    /// Charge les progressions RA des jeux visibles, une requête à la fois.
    /// L'API n'accepte pas de lot pour les fiches complètes ; les appels
    /// restent donc espacés et ne se répètent pas une fois le résultat reçu.
    pub(crate) fn prefetch_ps2_progress(&mut self, ctx: &egui::Context, ids: &[String]) {
        if self.ps2_ra_job.is_some() || !crate::ps2::ra_configured(&self.lib.settings) {
            return;
        }
        for id in ids {
            let Some(game) = self.lib.get(id) else {
                continue;
            };
            let Some(ra_game_id) = game.ps2.as_ref().and_then(|ps2| ps2.ra_game_id) else {
                continue;
            };
            if self.ps2_ra.contains_key(id) {
                continue;
            }
            self.request_ps2_ra(ctx, id, ra_game_id);
            break;
        }
    }

    /// Demande la progression RetroAchievements d'un jeu PS2.
    pub fn request_ps2_ra(&mut self, ctx: &egui::Context, game_id: &str, ra_game_id: u64) {
        if self.ps2_ra_job.is_some() {
            return;
        }
        let settings = self.lib.settings.clone();
        if !crate::ps2::ra_configured(&settings) {
            self.ps2_ra.insert(
                game_id.to_string(),
                AchievementsState::Failed(
                    "RetroAchievements n'est pas configuré : Paramètres → Boutiques & outils."
                        .into(),
                ),
            );
            return;
        }
        self.ps2_ra
            .insert(game_id.to_string(), AchievementsState::Loading);
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        let id = game_id.to_string();
        std::thread::spawn(move || {
            let client = artwork::http();
            let result = crate::ps2::fetch_ra_achievements(&client, &settings, ra_game_id);
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.ps2_ra_job = Some((id, rx));
        ctx.request_repaint_after(Duration::from_millis(150));
    }

    fn poll_ps2(&mut self, ctx: &egui::Context) {
        let search = self
            .ps2_search_job
            .as_ref()
            .map(|(id, rx)| (id.clone(), rx.try_recv()));
        if let Some((id, result)) = search {
            match result {
                Ok(Ok(Some(found))) => {
                    let title = found.title.clone();
                    let ra_id = found.id;
                    if let Some(editing) = self.editing.as_mut()
                        && editing.id == id
                    {
                        if let Some(ps2) = editing.ps2.as_mut() {
                            ps2.ra_game_id = Some(ra_id);
                        }
                    } else if let Some(game) = self.lib.get_mut(&id)
                        && let Some(ps2) = game.ps2.as_mut()
                    {
                        ps2.ra_game_id = Some(ra_id);
                        self.touch();
                    }
                    self.toast_info(format!("RetroAchievements : {title} · #{ra_id}"));
                    self.ps2_search_job = None;
                }
                Ok(Ok(None)) => {
                    self.toast_info(
                        "Aucune fiche RetroAchievements trouvée pour ce titre : ajuste le                          titre ou saisis l'identifiant à la main",
                    );
                    self.ps2_search_job = None;
                }
                Ok(Err(error)) => {
                    self.toast_error(error);
                    self.ps2_search_job = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.toast_error("Recherche RetroAchievements interrompue");
                    self.ps2_search_job = None;
                }
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(150));
                }
            }
        }

        let achievements = self
            .ps2_ra_job
            .as_ref()
            .map(|(id, rx)| (id.clone(), rx.try_recv()));
        if let Some((id, result)) = achievements {
            match result {
                Ok(Ok(info)) => {
                    self.ps2_ra.insert(id, AchievementsState::Ready(info));
                    self.ps2_ra_job = None;
                }
                Ok(Err(error)) => {
                    self.ps2_ra.insert(id, AchievementsState::Failed(error));
                    self.ps2_ra_job = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.ps2_ra.insert(
                        id,
                        AchievementsState::Failed(
                            "Lecture RetroAchievements interrompue".to_string(),
                        ),
                    );
                    self.ps2_ra_job = None;
                }
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(150));
                }
            }
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
        self.maybe_startup_update(ctx);
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
    use super::{StartPage, Tab, removal_targets, startup_route};

    #[test]
    fn la_page_d_ouverture_choisit_le_bon_onglet_et_l_accueil() {
        assert_eq!(startup_route(StartPage::Home), (Tab::Library, true));
        assert_eq!(startup_route(StartPage::Library), (Tab::Library, false));
        assert_eq!(startup_route(StartPage::Store), (Tab::Store, false));
        assert_eq!(startup_route(StartPage::MyGames), (Tab::MyGames, false));
    }

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
