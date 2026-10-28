//! État de l'application et assemblage de l'interface.

use crate::artwork::{self, AssetPlan, SearchHit, SearchOutcome};
use crate::config::{
    Artwork, AssetKind, Game, Library, ProtonMode, ThemeMode, ViewMode, human_playtime, now_ts,
};
use crate::news::{self, NewsItem};
use crate::protondb::{self, Report};
use crate::proton::{self, ProtonRuntime};
use crate::session::Session;
use crate::tray::TrayHandle;
use crate::theme::{self, Palette};
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
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PickerTarget {
    AddExe,
    AddPrefix,
    GameExe,
    GamePrefix,
    DefaultPrefix,
    ScanFolder,
    /// Image choisie sur le disque pour un artwork du jeu personnalisé.
    CustomArtwork(AssetKind),
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
    /// Ce qui est prérempli dans chaque jeu ajouté (ce n'est pas global).
    Defaults,
    Proton,
    Artworks,
    About,
}

impl SettingsTab {
    pub const ALL: [(SettingsTab, &'static str); 5] = [
        (SettingsTab::Appearance, "Apparence"),
        (SettingsTab::Defaults, "Par défaut"),
        (SettingsTab::Proton, "Proton"),
        (SettingsTab::Artworks, "Artworks"),
        (SettingsTab::About, "À propos"),
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

pub struct QuarkApp {
    pub lib: Library,
    pub pal: Palette,
    pub tab: Tab,
    pub selected: Option<String>,
    pub filter: String,
    pub store_query: String,
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
    /// Rampe de dégradé des bannières (créée au premier usage).
    pub gradient: Option<egui::TextureHandle>,
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
    /// Icône de la zone de notification.
    pub tray: Option<TrayHandle>,
    pub tray_attempted: bool,
    pub tray_error: Option<String>,
    /// Fermeture autorisée malgré l'icône (menu « Quitter »).
    pub allow_close: bool,
}

impl QuarkApp {
    /// Construit l'application. Seul le contexte egui est nécessaire : il vient
    /// de la fenêtre réelle ou du rendu hors écran (`quark --ui`).
    pub fn new(ctx: &egui::Context) -> Self {
        let lib = Library::load();
        let runtimes = proton::detect_runtimes();
        let selected = lib.games.first().map(|g| g.id.clone());
        theme::apply(ctx, lib.settings.theme);
        let pal = theme::palette(lib.settings.theme);
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
            gradient: None,
            news: HashMap::new(),
            news_job: None,
            update: UpdateState::Idle,
            update_rx: None,
            protondb: HashMap::new(),
            protondb_job: None,
            logs: None,
            tray: None,
            tray_attempted: false,
            tray_error: None,
            allow_close: false,
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
        self.poll_tray(ctx);
        self.sync_tray(ctx);
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
            // Icône de notification : la croix range Quark dans la zone de
            // notification, seul « Quitter » (menu de l'icône) termine le
            // processus.
            if self.lib.settings.tray_icon
                && !self.allow_close
                && self.tray.as_ref().is_some_and(|tray| tray.alive())
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
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
        let mut finished: Vec<(String, u64)> = Vec::new();
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
                Ok(Some(_)) => finished.push((id.clone(), game.started.elapsed().as_secs())),
                Err(_) => finished.push((id.clone(), game.started.elapsed().as_secs())),
            }
        }
        for (id, secs) in finished {
            self.running.remove(&id);
            if let Some(game) = self.lib.get_mut(&id) {
                let name = game.name.clone();
                game.playtime_secs += secs;
                game.last_played = Some(now_ts());
                self.toast_info(format!("{name} : session de {}", human_playtime(secs.max(1))));
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
            if matches!(target, PickerTarget::CustomArtwork(_)) {
                dialog = dialog.add_filter(
                    "Images",
                    &["png", "PNG", "jpg", "JPG", "jpeg", "JPEG", "webp", "WEBP"],
                );
            } else if !directory {
                dialog = dialog
                    .add_filter(
                        "Programmes",
                        &["exe", "EXE", "sh", "bat", "run", "desktop", "AppImage", "appimage"],
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
            .map(|picker| (picker.target, picker.rx.try_recv()));
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
                    Some(h) => (h.title.clone(), format!("{} · {}", h.source.label(), h.detail)),
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
                if self.customize.as_ref().is_some_and(|customize| customize.loading) {
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
                match crate::session::start(&game, &resolved) {
                    Ok(session) => {
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
        let Some(app_id) = self
            .lib
            .get(&id)
            .and_then(|game| game.steam_app_id)
        else {
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
                self.protondb
                    .insert(app_id, ProtonDbState::Failed(message));
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

    /// Crée (ou retire) l'icône selon le réglage, sans réessayer en boucle
    /// quand le bureau n'a pas de zone de notification.
    fn sync_tray(&mut self, ctx: &egui::Context) {
        if !self.lib.settings.tray_icon {
            if self.tray.is_some() || self.tray_error.is_some() || self.tray_attempted {
                self.tray = None;
                self.tray_error = None;
                self.tray_attempted = false;
            }
            return;
        }
        if self.tray_attempted {
            return;
        }
        self.tray_attempted = true;
        match TrayHandle::spawn(ctx) {
            Ok(handle) => {
                self.tray = Some(handle);
                self.tray_error = None;
            }
            Err(err) => {
                self.tray = None;
                self.tray_error = Some(err);
            }
        }
    }

    fn poll_tray(&mut self, ctx: &egui::Context) {
        let Some(tray) = self.tray.as_ref() else {
            return;
        };
        let show = tray.take_show();
        let quit = tray.take_quit();
        if show {
            // Un clic sur l'icône ramène la fenêtre telle qu'elle était.
            self.allow_close = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        if quit {
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
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
                    painter.circle_stroke(mark_rect.center(), 8.0, Stroke::new(2.0_f32, pal.accent));
                    painter.circle_stroke(mark_rect.center(), 3.0, Stroke::new(2.0_f32, pal.text_dim));
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
                    if widgets::tab(ui, "Bibliothèque", self.tab == Tab::Library, &pal).clicked() {
                        self.tab = Tab::Library;
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(16.0);
                        if widgets::icon_button(ui, 32.0, Icon::Gear, &pal, "Paramètres").clicked() {
                            self.settings_open = true;
                        }
                        ui.add_space(6.0);
                        if self.tab == Tab::Library {
                            ui.add_sized(
                                Vec2::new(260.0, 30.0),
                                egui::TextEdit::singleline(&mut self.filter)
                                    .id(egui::Id::new("quark_filter"))
                                    .hint_text("Rechercher dans la bibliothèque  ·  Ctrl+F")
                                    .font(FontId::proportional(12.5)),
                            );
                        } else {
                            ui.label(
                                RichText::new("Boutique bientôt disponible")
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

    /// Jeux affichés dans la colonne de gauche (tri alphabétique + filtre).
    pub fn display_ids(&self) -> Vec<String> {
        let needle = self.filter.trim().to_lowercase();
        let mut ids = self.lib.sorted_ids();
        if !needle.is_empty() {
            ids.retain(|id| {
                self.lib
                    .get(id)
                    .map(|g| g.name.to_lowercase().contains(&needle))
                    .unwrap_or(false)
            });
        }
        ids
    }

    /// Sélectionne un jeu ; bascule sur la bibliothèque si besoin. Un clic
    /// simple abandonne la sélection multiple (Ctrl+clic la construit).
    pub fn select(&mut self, id: &str) {
        self.selected = Some(id.to_string());
        self.marked.clear();
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

        self.top_bar(ctx);
        match self.tab {
            Tab::Store => self.store_tab(ctx),
            Tab::Library => self.library_tab(ctx),
        }

        self.add_dialog(ctx);
        self.settings_dialog(ctx);
        self.customize_dialog(ctx);
        self.game_settings_dialog(ctx);
        self.confirm_remove_dialog(ctx);
        self.logs_dialog(ctx);
        self.toasts_ui(ctx);
    }

    /// Navigation au clavier, sans souris : flèches pour changer de jeu, Entrée
    /// pour lancer, Ctrl+F (ou « / ») pour la recherche, Échap pour la vider.
    /// Les raccourcis sont ignorés tant qu'une fenêtre ou un champ de saisie a
    /// le focus, pour ne jamais voler une frappe.
    fn keyboard(&mut self, ctx: &egui::Context) {
        if self.add.open
            || self.settings_open
            || self.editing.is_some()
            || self.customize.is_some()
            || self.logs.is_some()
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
        let now = now_ts();
        let sessions: Vec<(String, u64)> = self
            .running
            .iter()
            .map(|(id, game)| (id.clone(), game.started.elapsed().as_secs()))
            .collect();
        for (id, secs) in sessions {
            if let Some(game) = self.lib.get_mut(&id) {
                game.playtime_secs += secs;
                game.last_played = Some(now);
            }
        }
        let _ = self.lib.save();
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

    /// Rampe de dégradé des bannières, créée une seule fois.
    pub fn gradient_texture(&mut self, ctx: &egui::Context) -> egui::TextureHandle {
        if self.gradient.is_none() {
            self.gradient = Some(theme::gradient_texture(ctx));
        }
        self.gradient
            .clone()
            .expect("la rampe de dégradé vient d'être créée")
    }

    /// Vue par défaut = réglage ; la vue courante est stockée dans les réglages.
    pub fn set_view(&mut self, view: ViewMode) {
        if self.lib.settings.view != view {
            self.lib.settings.view = view;
            self.touch();
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
