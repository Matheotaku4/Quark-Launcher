//! État de l'application et assemblage de l'interface.

use crate::artwork::{self, SearchHit, SearchOutcome};
use crate::config::{
    Artwork, AssetKind, Game, Library, ProtonMode, ThemeMode, ViewMode, human_playtime, now_ts,
};
use crate::proton::{self, ProtonRuntime};
use crate::theme::{self, Palette};
use crate::widgets::{self, Icon};
use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Pos2, RichText, Sense, Stroke, Vec2};
use std::collections::HashMap;
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
    Exe,
}

/// État de la fenêtre « Ajouter un jeu » (nom -> artworks -> exécutable).
pub struct AddGame {
    pub open: bool,
    pub step: AddStep,
    pub name: String,
    pub state: SearchState,
    pub rx: Option<Receiver<SearchOutcome>>,
    pub last_typed: Option<Instant>,
    pub hit: Option<SearchHit>,
    pub exe: String,
    pub args: String,
    pub proton: ProtonMode,
    pub custom_prefix: bool,
    pub prefix: String,
    pub error: Option<String>,
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
            hit: None,
            exe: String::new(),
            args: String::new(),
            proton: ProtonMode::Default,
            custom_prefix: false,
            prefix: String::new(),
            error: None,
        }
    }
}

/// Téléchargement d'artworks en cours.
pub struct AssetJob {
    pub rx: Receiver<(Artwork, Vec<String>)>,
    pub game_id: String,
    pub label: String,
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
    pub browse: Option<crate::ui_dialogs::FileBrowser>,
    pub confirm_remove: Option<String>,
    pub runtimes: Vec<ProtonRuntime>,
    pub running: HashMap<String, RunningGame>,
    pub job: Option<AssetJob>,
    pub toasts: Vec<Toast>,
    pub dirty: bool,
    pub last_change: Instant,
    pub applied_theme: Option<ThemeMode>,
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
            browse: None,
            confirm_remove: None,
            running: HashMap::new(),
            job: None,
            toasts: Vec::new(),
            dirty: false,
            last_change: Instant::now(),
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
        self.poll_asset_job();
        self.poll_search();
        self.auto_search(ctx);

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

        if self.job.is_some() || !self.running.is_empty() {
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

    fn poll_asset_job(&mut self) {
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
        if warnings.is_empty() {
            self.toast_info(format!("Artworks de {label} importés"));
        } else {
            self.toast_error(format!("{label} : {}", warnings.join(" · ")));
        }
    }

    fn poll_search(&mut self) {
        let received = self.add.rx.as_ref().map(|rx| rx.try_recv());
        match received {
            Some(Ok(outcome)) => {
                self.add.state = SearchState::Done(outcome.hits, outcome.warnings);
                self.add.rx = None;
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.add.state = SearchState::Done(Vec::new(), vec!["Recherche interrompue".to_string()]);
                self.add.rx = None;
            }
            _ => {}
        }
    }

    fn auto_search(&mut self, ctx: &egui::Context) {
        if !self.add.open || self.add.step != AddStep::Name || self.add.rx.is_some() {
            return;
        }
        if self.add.name.trim().len() < 2 {
            return;
        }
        if let Some(typed) = self.add.last_typed {
            if typed.elapsed() > Duration::from_millis(500) {
                self.start_search(ctx);
            }
        }
    }

    pub fn start_search(&mut self, ctx: &egui::Context) {
        let query = self.add.name.trim().to_string();
        if query.len() < 2 {
            return;
        }
        let (tx, rx) = channel();
        let settings = self.lib.settings.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let client = artwork::http();
            let outcome = artwork::search(&client, &query, &settings);
            let _ = tx.send(outcome);
            ctx.request_repaint();
        });
        self.add.rx = Some(rx);
        self.add.state = SearchState::Loading;
    }

    pub fn start_asset_job(&mut self, ctx: &egui::Context, game_id: &str, hit: SearchHit) {
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

    fn backdrop(&mut self, ctx: &egui::Context) {
        egui::Area::new(egui::Id::new("quark_backdrop"))
            .order(egui::Order::Middle)
            .fixed_pos(Pos2::ZERO)
            .show(ctx, |ui| {
                let rect = ctx.screen_rect();
                ui.allocate_response(rect.size(), Sense::click());
                ui.painter()
                    .rect_filled(rect, 0.0, Color32::from_black_alpha(175));
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

        let modal = self.settings_open
            || self.add.open
            || self.editing.is_some()
            || self.browse.is_some()
            || self.confirm_remove.is_some();
        if modal {
            self.backdrop(ctx);
        }

        self.add_dialog(ctx);
        self.settings_dialog(ctx);
        self.game_settings_dialog(ctx);
        self.file_browser_window(ctx);
        self.confirm_remove_dialog(ctx);
        self.toasts_ui(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
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
