//! État de l'application et assemblage de l'interface.

use crate::artwork::{self, SearchHit, SearchOutcome};
use crate::config::{
    Artwork, AssetKind, Game, Library, ProtonMode, ThemeMode, ViewMode, human_playtime, now_ts,
};
use crate::news::{self, NewsItem};
use crate::proton::{self, ProtonRuntime};
use crate::theme::{self, Palette};
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

pub struct RunningGame {
    pub child: std::process::Child,
    pub started: Instant,
    pub session: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Error,
}

pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
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
}

/// Sélecteur de fichiers du système, exécuté dans un thread dédié pour ne pas
/// bloquer le rendu pendant que le dialogue est ouvert.
pub struct NativePicker {
    pub rx: Receiver<Option<PathBuf>>,
    pub target: PickerTarget,
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
    pub editing: Option<Game>,
    pub picker: Option<NativePicker>,
    pub confirm_remove: Option<String>,
    pub runtimes: Vec<ProtonRuntime>,
    pub running: HashMap<String, RunningGame>,
    pub job: Option<AssetJob>,
    pub toasts: Vec<Toast>,
    pub dirty: bool,
    pub last_change: Instant,
    pub applied_theme: Option<ThemeMode>,
    /// Panneau d'informations du jeu (masqué par défaut, bouton ⓘ).
    pub show_info: bool,
    /// Artworks à télécharger après le job en cours.
    pub job_queue: VecDeque<(String, SearchHit)>,
    /// Textures des artworks (décodées par Quark lui-même).
    pub textures: crate::images::TextureCache,
    /// Actualités par jeu (chargées une fois, mises en cache).
    pub news: HashMap<String, NewsState>,
    /// Chargement d'actualités en cours.
    pub news_job: Option<NewsJob>,
}

impl QuarkApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let lib = Library::load();
        let runtimes = proton::detect_runtimes();
        let selected = lib.games.first().map(|g| g.id.clone());
        theme::apply(&cc.egui_ctx, lib.settings.theme);
        let pal = theme::palette(lib.settings.theme);
        Self {
            applied_theme: Some(lib.settings.theme),
            selected,
            runtimes,
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
            editing: None,
            picker: None,
            confirm_remove: None,
            running: HashMap::new(),
            job: None,
            toasts: Vec::new(),
            dirty: false,
            last_change: Instant::now(),
            show_info: false,
            job_queue: VecDeque::new(),
            textures: crate::images::TextureCache::default(),
            news: HashMap::new(),
            news_job: None,
        }
    }

    pub fn touch(&mut self) {
        self.dirty = true;
        self.last_change = Instant::now();
    }

    pub fn toast_info(&mut self, text: impl Into<String>) {
        self.toasts.push(Toast {
            text: text.into(),
            kind: ToastKind::Info,
            until: Instant::now() + Duration::from_secs(6),
        });
    }

    pub fn toast_error(&mut self, text: impl Into<String>) {
        self.toasts.push(Toast {
            text: text.into(),
            kind: ToastKind::Error,
            until: Instant::now() + Duration::from_secs(10),
        });
    }

    pub fn open_add_dialog(&mut self) {
        self.add = AddGame::reset();
    }

    // -----------------------------------------------------------------------
    // Boucle
    // -----------------------------------------------------------------------

    fn tick(&mut self, ctx: &egui::Context) {
        self.poll_running_games();
        self.poll_asset_job(ctx);
        self.poll_news();
        self.poll_picker(ctx);
        self.poll_search();
        self.poll_scan();
        self.auto_search(ctx);
        self.ensure_news(ctx);

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
        }

        if self.job.is_some() || self.news_job.is_some() || !self.running.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(400));
        }
    }

    fn poll_running_games(&mut self) {
        let mut finished: Vec<(String, u64)> = Vec::new();
        for (id, game) in self.running.iter_mut() {
            match game.child.try_wait() {
                Ok(None) => game.session = game.started.elapsed().as_secs(),
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
        if let Some((id, hit)) = self.job_queue.pop_front() {
            self.start_asset_job(ctx, &id, hit);
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
            if !directory {
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
        // Un seul téléchargement à la fois : les suivants attendent leur tour.
        if self.job.is_some() {
            self.job_queue.push_back((game_id.to_string(), hit));
            return;
        }
        let (tx, rx) = channel();
        let settings = self.lib.settings.clone();
        let id = game_id.to_string();
        let label = hit.title.clone();
        let thread_id = id.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let client = artwork::http();
            let result = artwork::fetch(&client, &thread_id, &hit, &settings);
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        self.job = Some(AssetJob {
            rx,
            game_id: game_id.to_string(),
            label,
        });
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
        let mut added = 0usize;
        for (title, exe, hit) in picked {
            let mut game = Game::new(title, exe.display().to_string());
            game.proton = proton.clone();
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
                if let Some(prefix) = &resolved.prefix {
                    if let Err(err) = std::fs::create_dir_all(prefix) {
                        self.toast_error(format!("Préfixe Proton : {err}"));
                        return;
                    }
                }
                let mut cmd = std::process::Command::new(&resolved.spec.program);
                cmd.args(&resolved.spec.args);
                for (key, value) in &resolved.spec.env {
                    cmd.env(key, value);
                }
                let workdir = game
                    .exe_path()
                    .parent()
                    .map(|p| p.to_path_buf())
                    .or_else(|| resolved.spec.program.parent().map(|p| p.to_path_buf()));
                if let Some(dir) = workdir {
                    cmd.current_dir(dir);
                }
                cmd.stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
                // Groupe de processus dédié : « Arrêter » peut tuer wine et le jeu.
                #[cfg(unix)]
                {
                    use std::os::unix::process::CommandExt;
                    cmd.process_group(0);
                }
                match cmd.spawn() {
                    Ok(child) => {
                        self.running.insert(
                            id.to_string(),
                            RunningGame {
                                child,
                                started: Instant::now(),
                                session: 0,
                            },
                        );
                        self.toast_info(format!("{} lancé · {}", game.name, resolved.label()));
                        ctx.request_repaint();
                    }
                    Err(err) => self.toast_error(format!("Lancement impossible : {err}")),
                }
            }
            Err(err) => self.toast_error(err),
        }
    }

    pub fn stop(&mut self, id: &str) {
        if let Some(game) = self.running.get_mut(id) {
            // Proton lance wine en enfant : on termine tout le groupe de processus
            // (le jeu est lancé dans son propre groupe, voir `launch`).
            let pid = game.child.id() as i32;
            #[cfg(unix)]
            unsafe {
                libc::kill(-pid, libc::SIGTERM);
            }
            let _ = game.child.kill();
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
                                    .hint_text("Rechercher dans la bibliothèque")
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
                ui.with_layout(Layout::bottom_up(Align::RIGHT), |ui| {
                    for toast in self.toasts.iter().rev().take(4) {
                        let color = match toast.kind {
                            ToastKind::Info => pal.border,
                            ToastKind::Error => pal.danger,
                        };
                        egui::Frame::new()
                            .fill(pal.surface)
                            .stroke(Stroke::new(1.0_f32, color))
                            .corner_radius(6.0)
                            .inner_margin(egui::Margin::symmetric(14, 9))
                            .show(ui, |ui| {
                                ui.set_max_width(380.0);
                                ui.label(
                                    RichText::new(&toast.text)
                                        .size(12.5)
                                        .color(if toast.kind == ToastKind::Error {
                                            pal.danger
                                        } else {
                                            pal.text
                                        }),
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

    /// Sélectionne un jeu ; bascule sur la bibliothèque si besoin.
    pub fn select(&mut self, id: &str) {
        self.selected = Some(id.to_string());
    }
}

impl eframe::App for QuarkApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick(ctx);

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
        self.game_settings_dialog(ctx);
        self.confirm_remove_dialog(ctx);
        self.toasts_ui(ctx);
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

    /// Vue par défaut = réglage ; la vue courante est stockée dans les réglages.
    pub fn set_view(&mut self, view: ViewMode) {
        if self.lib.settings.view != view {
            self.lib.settings.view = view;
            self.touch();
        }
    }

}
