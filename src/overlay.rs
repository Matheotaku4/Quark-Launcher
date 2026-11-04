use crate::config::Settings;
use crate::providers::{AchievementInfo, AchievementRow, Provider, ProviderGameKey};
use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2, ViewportBuilder, ViewportClass,
    ViewportCommand, ViewportId,
};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel as mpsc_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Jeu visé par l'overlay : figé au moment du lancement, pour que les captures
/// et les succès portent le bon nom même si l'utilisateur change de jeu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub name: String,
    pub provider: Option<Provider>,
    /// Fiche de boutique, utilisée pour lire les succès.
    pub key: Option<ProviderGameKey>,
    /// Identifiant de la fiche Quark (pont des succès).
    pub game_id: String,
    pub started: Instant,
}

#[derive(Debug, Clone)]
pub struct CaptureEvent {
    pub path: PathBuf,
    pub game_name: String,
    pub provider: Option<Provider>,
}

#[derive(Debug, Clone)]
pub struct Notice {
    pub title: String,
    pub detail: String,
    pub created: Instant,
}

enum HotkeyEvent {
    Capture(CaptureEvent),
    Toggle,
}

pub struct Overlay {
    event_rx: Option<Receiver<HotkeyEvent>>,
    hotkey_stop: Option<Sender<()>>,
    hotkey_thread: Option<std::thread::JoinHandle<()>>,
    shared: Arc<Mutex<Option<Target>>>,
    pub target: Option<Target>,
    pub notices: VecDeque<Notice>,
    pub last_capture: Option<PathBuf>,
    hotkey_screenshot: String,
    hotkey_overlay: String,
    /// L'overlay plein écran est-il ouvert ?
    pub open: bool,
    focus_requested: bool,
    pub achievements: Option<AchievementInfo>,
    pub achievements_loading: bool,
    pub achievements_error: Option<String>,
}

impl Default for Overlay {
    fn default() -> Self {
        Self {
            event_rx: None,
            hotkey_stop: None,
            hotkey_thread: None,
            shared: Arc::new(Mutex::new(None)),
            target: None,
            notices: VecDeque::new(),
            last_capture: None,
            hotkey_screenshot: String::new(),
            hotkey_overlay: String::new(),
            open: false,
            focus_requested: false,
            achievements: None,
            achievements_loading: false,
            achievements_error: None,
        }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        self.stop_hotkey();
    }
}

impl Overlay {
    /// Déclare le jeu en cours et installe les deux raccourcis globaux :
    /// capture d'écran et ouverture de l'overlay.
    pub fn set_game(&mut self, target: Target, screenshot_key: &str, overlay_key: &str) {
        if self.hotkey_screenshot != screenshot_key || self.hotkey_overlay != overlay_key {
            self.stop_hotkey();
            if let Some((rx, stop, thread)) =
                start_hotkey(screenshot_key, overlay_key, self.shared.clone())
            {
                self.event_rx = Some(rx);
                self.hotkey_stop = Some(stop);
                self.hotkey_thread = Some(thread);
            }
            self.hotkey_screenshot = screenshot_key.to_string();
            self.hotkey_overlay = overlay_key.to_string();
        }
        if let Ok(mut shared) = self.shared.lock() {
            *shared = Some(target.clone());
        }
        self.target = Some(target);
        self.open = false;
        self.focus_requested = false;
        self.achievements = None;
        self.achievements_error = None;
        self.achievements_loading = false;
    }

    pub fn clear_game(&mut self) {
        self.stop_hotkey();
        self.hotkey_screenshot.clear();
        self.hotkey_overlay.clear();
        if let Ok(mut shared) = self.shared.lock() {
            *shared = None;
        }
        self.target = None;
        self.open = false;
        self.focus_requested = false;
        self.achievements = None;
        self.achievements_error = None;
        self.achievements_loading = false;
    }

    fn stop_hotkey(&mut self) {
        if let Some(stop) = self.hotkey_stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.hotkey_thread.take() {
            let _ = thread.join();
        }
        self.event_rx = None;
    }

    /// Récupère les captures et bascule l'overlay quand `Maj+Retour arrière`
    /// est pressé. Le son et la notification restent décidés par l'appelant,
    /// qui connaît la boutique du jeu.
    pub fn poll_captures(&mut self) -> Vec<CaptureEvent> {
        let Some(rx) = self.event_rx.as_ref() else {
            return Vec::new();
        };
        let mut events = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(HotkeyEvent::Capture(event)) => events.push(event),
                Ok(HotkeyEvent::Toggle) => {
                    self.open = !self.open;
                    if self.open {
                        self.focus_requested = true;
                        self.achievements_error = None;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.event_rx = None;
                    break;
                }
            }
        }
        for event in &events {
            self.last_capture = Some(event.path.clone());
            let name = event
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("capture.png")
                .to_string();
            self.notice("Capture", name);
        }
        events
    }

    pub fn notice(&mut self, title: impl Into<String>, detail: impl Into<String>) {
        self.notices.push_front(Notice {
            title: title.into(),
            detail: detail.into(),
            created: Instant::now(),
        });
        self.notices.truncate(8);
    }

    pub fn set_achievements(&mut self, info: AchievementInfo) {
        self.achievements = Some(info);
        self.achievements_loading = false;
        self.achievements_error = None;
    }

    pub fn set_achievements_loading(&mut self, loading: bool) {
        self.achievements_loading = loading;
        if loading {
            self.achievements_error = None;
        }
    }

    pub fn set_achievements_error(&mut self, error: Option<String>) {
        self.achievements_error = error;
        self.achievements_loading = false;
    }

    pub fn target_key(&self) -> Option<ProviderGameKey> {
        self.target.as_ref().and_then(|target| target.key.clone())
    }

    /// Affiche soit l'overlay complet (ouvert), soit un petit toast de succès
    /// ou de capture (fermé). Le viewport reste cliquable uniquement lorsque
    /// l'overlay est ouvert.
    pub fn ui(&mut self, ctx: &egui::Context, pal: &crate::theme::Palette, settings: &Settings) {
        let Some(target) = self.target.clone() else {
            return;
        };
        self.notices
            .retain(|notice| notice.created.elapsed() < Duration::from_secs(14));
        let open = self.open;
        let has_toast = !open
            && self
                .notices
                .front()
                .is_some_and(|notice| notice.created.elapsed() < Duration::from_secs(6));
        if !open && !has_toast {
            return;
        }
        let monitor = ctx.input(|input| input.viewport().monitor_size);
        let (size, position) = if open {
            let size = monitor.unwrap_or(Vec2::new(1280.0, 800.0));
            (size, Pos2::ZERO)
        } else {
            let size = Vec2::new(380.0, 96.0);
            let position = monitor
                .map(|monitor| Pos2::new((monitor.x - size.x - 20.0).max(16.0), 20.0))
                .unwrap_or(Pos2::new(24.0, 24.0));
            (size, position)
        };
        let builder = ViewportBuilder::default()
            .with_title("Quark Overlay")
            .with_inner_size(size)
            .with_position(position)
            .with_decorations(false)
            .with_resizable(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_mouse_passthrough(!open)
            .with_active(open)
            .with_taskbar(false);
        let elapsed = target.started.elapsed();
        let notices: Vec<Notice> = self.notices.iter().take(3).cloned().collect();
        let game = target.name.clone();
        let provider = target.provider;
        let pal = *pal;
        let achievements = self.achievements.clone();
        let error = self.achievements_error.clone();
        let loading = self.achievements_loading;
        let focus = std::mem::take(&mut self.focus_requested);
        let mut close = false;
        ctx.show_viewport_immediate(
            ViewportId::from_hash_of("quark-game-overlay"),
            builder,
            move |ctx, class| {
                if class == ViewportClass::Embedded {
                    return;
                }
                if focus {
                    ctx.send_viewport_cmd(ViewportCommand::Focus);
                }
                if open && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
                    close = true;
                }
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(Color32::TRANSPARENT))
                    .show(ctx, |ui| {
                        if open {
                            draw_overlay(
                                ui,
                                &pal,
                                &game,
                                provider,
                                elapsed,
                                &notices,
                                settings,
                                achievements.as_ref(),
                                error.as_deref(),
                                loading,
                                &mut close,
                            );
                        } else {
                            draw_toast(ui, &pal, &notices);
                        }
                    });
            },
        );
        if close {
            self.open = false;
        }
    }
}

fn draw_toast(ui: &mut egui::Ui, pal: &crate::theme::Palette, notices: &[Notice]) {
    use eframe::egui::RichText;
    let Some(notice) = notices.first() else {
        return;
    };
    egui::Frame::new()
        .fill(Color32::from_black_alpha(232))
        .stroke(Stroke::new(1.0_f32, Color32::from_white_alpha(30)))
        .corner_radius(12.0)
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.set_min_width(340.0);
            ui.horizontal(|ui| {
                ui.label(
                    crate::i18n::rich("QUARK")
                        .size(9.5)
                        .color(pal.accent)
                        .strong(),
                );
                ui.with_layout(
                    eframe::egui::Layout::right_to_left(eframe::egui::Align::Center),
                    |ui| {
                        ui.label(
                            crate::i18n::rich(&notice.title)
                                .size(12.0)
                                .color(Color32::WHITE),
                        );
                    },
                );
            });
            ui.label(
                crate::i18n::rich(&notice.detail)
                    .size(11.0)
                    .color(Color32::from_white_alpha(190)),
            );
        });
}

#[allow(clippy::too_many_arguments)]
fn draw_overlay(
    ui: &mut egui::Ui,
    pal: &crate::theme::Palette,
    game: &str,
    provider: Option<Provider>,
    elapsed: Duration,
    notices: &[Notice],
    settings: &Settings,
    achievements: Option<&AchievementInfo>,
    error: Option<&str>,
    loading: bool,
    close: &mut bool,
) {
    use eframe::egui::RichText;
    let screen = ui.max_rect();
    ui.painter()
        .rect_filled(screen, 0.0, Color32::from_black_alpha(178));
    let width = (screen.width() * 0.68)
        .clamp(420.0, 1040.0)
        .min((screen.width() - 32.0).max(320.0));
    let height = (screen.height() * 0.82)
        .clamp(320.0, 820.0)
        .min((screen.height() - 32.0).max(240.0));
    let panel = Rect::from_center_size(screen.center(), Vec2::new(width, height));
    ui.scope_builder(egui::UiBuilder::new().max_rect(panel), |ui| {
        egui::Frame::new()
            .fill(pal.bg)
            .stroke(Stroke::new(1.0_f32, pal.border))
            .corner_radius(16.0)
            .inner_margin(24.0)
            .show(ui, |ui| {
                ui.set_min_size(panel.size() - Vec2::splat(48.0));
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            crate::i18n::rich("EN JEU")
                                .size(10.0)
                                .color(pal.accent)
                                .strong(),
                        );
                        ui.label(crate::i18n::rich(game).size(24.0).color(pal.text));
                    });
                    ui.with_layout(
                        eframe::egui::Layout::right_to_left(eframe::egui::Align::Center),
                        |ui| {
                            if ui
                                .add(
                                    egui::Button::new(crate::i18n::rich("✕").size(16.0))
                                        .fill(Color32::TRANSPARENT),
                                )
                                .clicked()
                            {
                                *close = true;
                            }
                            ui.add_space(8.0);
                            ui.label(
                                crate::i18n::rich(clock(elapsed.as_secs()))
                                    .size(16.0)
                                    .monospace()
                                    .color(pal.text),
                            );
                            if let Some(provider) = provider {
                                ui.label(
                                    crate::i18n::rich(provider_label(provider))
                                        .size(10.0)
                                        .color(provider_color(provider)),
                                );
                            }
                        },
                    );
                });
                ui.add_space(4.0);
                ui.label(
                    crate::i18n::rich(format!(
                        "{} pour fermer · {} pour une capture",
                        settings.overlay_hotkey, settings.screenshot_key
                    ))
                    .size(10.5)
                    .color(pal.text_faint),
                );
                ui.add_space(16.0);
                ui.separator();
                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    ui.label(
                        crate::i18n::rich("Succès")
                            .size(13.0)
                            .color(pal.text)
                            .strong(),
                    );
                    if let Some(info) = achievements {
                        ui.label(
                            crate::i18n::rich(format!("{} / {}", info.unlocked, info.total))
                                .size(11.0)
                                .color(pal.text_dim),
                        );
                    }
                    ui.with_layout(
                        eframe::egui::Layout::right_to_left(eframe::egui::Align::Center),
                        |ui| {
                            if let Some(info) = achievements {
                                let progress = if info.total == 0 {
                                    0.0
                                } else {
                                    info.unlocked as f32 / info.total as f32
                                };
                                ui.add(
                                    egui::ProgressBar::new(progress)
                                        .desired_width(180.0)
                                        .fill(pal.accent),
                                );
                            }
                        },
                    );
                });
                ui.add_space(10.0);

                if loading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            crate::i18n::rich("Lecture des succès…")
                                .size(12.0)
                                .color(pal.text_dim),
                        );
                    });
                } else if let Some(error) = error {
                    ui.label(crate::i18n::rich(error).size(12.0).color(pal.danger));
                    ui.label(
                        crate::i18n::rich(
                            "Vérifie la connexion à la boutique dans Paramètres → Boutiques & outils.",
                        )
                        .size(11.0)
                        .color(pal.text_faint),
                    );
                } else if let Some(info) = achievements {
                    let rows: Vec<&AchievementRow> = if settings.overlay_show_locked {
                        crate::providers::achievements::sorted_rows(&info.rows)
                    } else {
                        crate::providers::achievements::sorted_rows(&info.rows)
                            .into_iter()
                            .filter(|row| row.unlocked)
                            .collect()
                    };
                    if rows.is_empty() {
                        ui.label(
                            crate::i18n::rich("Aucun succès à afficher.")
                                .size(12.0)
                                .color(pal.text_dim),
                        );
                    }
                    let list_height = (ui.available_height() - 70.0).max(80.0);
                    egui::ScrollArea::vertical()
                        .id_salt("quark_overlay_achievements")
                        .auto_shrink([false, false])
                        .max_height(list_height)
                        .show(ui, |ui| {
                            for row in rows {
                                achievement_row(ui, pal, row, settings);
                            }
                        });
                } else {
                    ui.label(
                        crate::i18n::rich("Aucun succès chargé pour ce jeu.")
                            .size(12.0)
                            .color(pal.text_dim),
                    );
                }

                ui.with_layout(
                    eframe::egui::Layout::bottom_up(eframe::egui::Align::LEFT),
                    |ui| {
                        for notice in notices.iter().rev() {
                            ui.label(
                                crate::i18n::rich(format!("{} · {}", notice.title, notice.detail))
                                    .size(10.0)
                                    .color(pal.text_faint),
                            );
                        }
                    },
                );
            });
    });
}

fn achievement_row(
    ui: &mut egui::Ui,
    pal: &crate::theme::Palette,
    row: &AchievementRow,
    settings: &Settings,
) {
    use eframe::egui::RichText;
    let compact = settings.overlay_compact;
    egui::Frame::new()
        .fill(if row.unlocked {
            pal.surface
        } else {
            Color32::from_white_alpha(8)
        })
        .corner_radius(9.0)
        .inner_margin(if compact { 8.0 } else { 11.0 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let icon_size = if compact { 30.0 } else { 38.0 };
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(icon_size), Sense::hover());
                if let Some(path) = row.icon_path.as_ref().filter(|path| path.exists()) {
                    ui.put(
                        rect,
                        egui::Image::new(format!("file://{}", path.display()))
                            .fit_to_exact_size(rect.size()),
                    );
                } else {
                    ui.painter().circle_filled(
                        rect.center(),
                        rect.width() / 2.0,
                        pal.surface_hover,
                    );
                    let letter = row
                        .name
                        .chars()
                        .next()
                        .map(|c| c.to_uppercase().to_string())
                        .unwrap_or_else(|| "?".to_string());
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        letter,
                        FontId::proportional(15.0),
                        pal.text_dim,
                    );
                }
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.set_max_width((ui.available_width() - 150.0).max(160.0));
                    ui.horizontal(|ui| {
                        ui.label(
                            crate::i18n::rich(&row.name)
                                .size(12.5)
                                .color(if row.unlocked { pal.text } else { pal.text_dim })
                                .strong(),
                        );
                        if row.session {
                            ui.label(crate::i18n::rich("SESSION").size(8.5).color(pal.accent));
                        }
                    });
                    if !compact && !row.description.is_empty() {
                        ui.label(
                            crate::i18n::rich(&row.description)
                                .size(10.5)
                                .color(pal.text_faint),
                        );
                    }
                });
                ui.with_layout(
                    eframe::egui::Layout::right_to_left(eframe::egui::Align::Center),
                    |ui| {
                        if row.unlocked {
                            ui.label(crate::i18n::rich("✓").size(16.0).color(pal.accent));
                        } else if settings.overlay_show_progress && row.progress > 0.0 {
                            ui.label(
                                crate::i18n::rich(format!("{:.0} %", row.progress * 100.0))
                                    .size(10.5)
                                    .color(pal.text_faint),
                            );
                        }
                        if settings.overlay_show_progress
                            && let Some(rarity) = row.rarity
                        {
                            ui.label(
                                crate::i18n::rich(format!("{rarity:.1} % des joueurs"))
                                    .size(9.5)
                                    .color(pal.text_faint),
                            );
                        }
                    },
                );
            });
        });
    ui.add_space(if compact { 4.0 } else { 7.0 });
}

fn provider_label(provider: Provider) -> &'static str {
    match provider {
        Provider::Steam => "STEAM",
        Provider::Epic => "EPIC GAMES",
        Provider::Gog => "GOG",
    }
}

fn provider_color(provider: Provider) -> Color32 {
    match provider {
        Provider::Steam => Color32::from_rgb(0x8F, 0xB8, 0xE8),
        Provider::Epic => Color32::from_rgb(0xC8, 0xC8, 0xD2),
        Provider::Gog => Color32::from_rgb(0xB9, 0x8B, 0xE8),
    }
}

fn clock(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

/// Nom de dossier sûr pour les captures : le titre du jeu peut contenir `/`,
/// `:` ou des caractères Unicode qu'un système de fichiers refuse.
pub fn safe_name(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
        if out.len() >= 64 {
            break;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "jeu".to_string()
    } else {
        trimmed
    }
}

pub fn screenshots_dir() -> PathBuf {
    crate::config::data_dir().join("screenshots")
}

fn screenshot_path(target: &Target) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    screenshots_dir()
        .join(safe_name(&target.name))
        .join(format!("{stamp}.png"))
}

/// Lance le thread d'écoute X11. Deux raccourcis sont enregistrés : la
/// capture d'écran et l'ouverture de l'overlay. Sans serveur X (Wayland pur,
/// session distante), il ne se passe rien et l'overlay reste utilisable pour
/// les succès.
fn start_hotkey(
    screenshot_key: &str,
    overlay_key: &str,
    shared: Arc<Mutex<Option<Target>>>,
) -> Option<(
    Receiver<HotkeyEvent>,
    Sender<()>,
    std::thread::JoinHandle<()>,
)> {
    let (tx, rx) = mpsc_channel();
    let (stop_tx, stop_rx) = mpsc_channel();
    let screenshot_key = screenshot_key.trim().to_string();
    let overlay_key = overlay_key.trim().to_string();
    MAIN_THREAD.store(unsafe { libc::pthread_self() } as usize, Ordering::SeqCst);
    let thread = std::thread::Builder::new()
        .name("quark-hotkey".into())
        .spawn(move || {
            if let Err(error) = hotkey_loop(&screenshot_key, &overlay_key, shared, tx, stop_rx) {
                if std::env::var_os("QUARK_DEBUG_HOTKEY").is_some() {
                    eprintln!("[quark] overlay : {error}");
                }
            }
        })
        .ok()?;
    Some((rx, stop_tx, thread))
}

static MAIN_THREAD: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn x_io_error(_display: *mut x11_dl::xlib::Display) -> i32 {
    // Xlib termine normalement tout le processus si le gestionnaire retourne.
    // Depuis le thread du raccourci, on termine seulement ce thread : la
    // fenêtre et l'autosauvegarde continuent.
    if unsafe { libc::pthread_self() } as usize != MAIN_THREAD.load(Ordering::SeqCst) {
        unsafe { libc::pthread_exit(std::ptr::null_mut()) }
    }
    0
}

/// Sépare « Maj+Retour arrière » en modificateurs X11 et nom de touche.
/// Les alias français sont acceptés pour que la valeur par défaut reste
/// lisible même si l'utilisateur la modifie à la main.
fn parse_combo(text: &str) -> (u32, String) {
    use x11_dl::xlib;
    let mut modifiers = 0u32;
    let mut key = String::new();
    for part in text
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        match part.to_ascii_lowercase().as_str() {
            "shift" | "maj" => modifiers |= xlib::ShiftMask,
            "ctrl" | "control" | "contrôle" => modifiers |= xlib::ControlMask,
            "alt" | "option" => modifiers |= xlib::Mod1Mask,
            "super" | "meta" | "win" => modifiers |= xlib::Mod4Mask,
            _ => key = part.to_string(),
        }
    }
    let key = match key.to_ascii_lowercase().as_str() {
        "backspace" | "retour arrière" | "retour arriere" => "BackSpace".to_string(),
        "return" | "entrée" | "entree" => "Return".to_string(),
        "escape" | "échappe" => "Escape".to_string(),
        "space" | "espace" => "space".to_string(),
        "tab" => "Tab".to_string(),
        _ => key,
    };
    (modifiers, key)
}

fn hotkey_loop(
    screenshot_key: &str,
    overlay_key: &str,
    shared: Arc<Mutex<Option<Target>>>,
    tx: Sender<HotkeyEvent>,
    stop: Receiver<()>,
) -> Result<(), String> {
    use x11_dl::xlib;
    let api = xlib::Xlib::open().map_err(|_| "X11 indisponible".to_string())?;
    let display = unsafe { (api.XOpenDisplay)(std::ptr::null()) };
    if display.is_null() {
        return Err("connexion X11 impossible".to_string());
    }
    unsafe {
        (api.XSetErrorHandler)(Some(ignore_x_error));
        (api.XSetIOErrorHandler)(Some(x_io_error));
    }
    let root = unsafe { (api.XDefaultRootWindow)(display) };
    let (shot_mods, shot_name) = parse_combo(screenshot_key);
    let (overlay_mods, overlay_name) = parse_combo(overlay_key);
    let (shot_code, _) = resolve_key(&api, display, &shot_name);
    let (overlay_code, _) = resolve_key(&api, display, &overlay_name);
    if shot_code == 0 && overlay_code == 0 {
        unsafe { (api.XCloseDisplay)(display) };
        return Err("aucune touche de raccourci valide".to_string());
    }
    let mut grabbed: Vec<(u8, u32)> = Vec::new();
    // Verr. Maj et Verr. Num changent les modificateurs : les quatre
    // combinaisons sont prises pour que les raccourcis marchent quel que soit
    // leur état.
    let lock_variants = [
        0,
        xlib::LockMask,
        xlib::Mod2Mask,
        xlib::LockMask | xlib::Mod2Mask,
    ];
    for (code, base) in [(shot_code, shot_mods), (overlay_code, overlay_mods)] {
        if code == 0 {
            continue;
        }
        for extra in lock_variants {
            let mods = base | extra;
            unsafe {
                (api.XGrabKey)(
                    display,
                    code as i32,
                    mods,
                    root,
                    0,
                    xlib::GrabModeAsync,
                    xlib::GrabModeAsync,
                );
            }
            grabbed.push((code, mods));
        }
    }
    unsafe { (api.XSync)(display, 0) };

    let mut event: xlib::XEvent = unsafe { std::mem::zeroed() };
    let mut last_capture: Option<Instant> = None;
    let mut last_toggle: Option<Instant> = None;
    // Boucle non bloquante : `XNextEvent` ne permettait ni arrêt ni fermeture
    // propre du Display.
    while stop.try_recv().is_err() {
        while unsafe { (api.XPending)(display) } > 0 {
            unsafe { (api.XNextEvent)(display, &mut event) };
            let kind = unsafe { event.type_ };
            if kind != xlib::KeyPress {
                continue;
            }
            let pressed = unsafe { event.key.keycode } as u8;
            let state = unsafe { event.key.state } & !(xlib::LockMask | xlib::Mod2Mask);
            if pressed == shot_code && state == shot_mods {
                // La répétition automatique du clavier envoie une rafale de
                // KeyPress : une capture par seconde suffit.
                if last_capture.is_some_and(|last| last.elapsed() < Duration::from_millis(700)) {
                    continue;
                }
                last_capture = Some(Instant::now());
                let target = shared.lock().ok().and_then(|guard| guard.clone());
                let Some(target) = target else {
                    continue;
                };
                let sender = tx.clone();
                std::thread::spawn(move || {
                    let path = screenshot_path(&target);
                    if capture_screen(&path).is_ok() {
                        let _ = sender.send(HotkeyEvent::Capture(CaptureEvent {
                            path,
                            game_name: target.name.clone(),
                            provider: target.provider,
                        }));
                    }
                });
            } else if overlay_code != 0 && pressed == overlay_code && state == overlay_mods {
                // Une répétition de la touche ne doit pas refermer l'overlay
                // aussi vite qu'elle l'a ouvert.
                if last_toggle.is_some_and(|last| last.elapsed() < Duration::from_millis(350)) {
                    continue;
                }
                last_toggle = Some(Instant::now());
                let _ = tx.send(HotkeyEvent::Toggle);
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    for (code, mods) in grabbed {
        unsafe { (api.XUngrabKey)(display, code as i32, mods, root) };
    }
    unsafe {
        (api.XSync)(display, 0);
        (api.XCloseDisplay)(display);
    }
    Ok(())
}

fn resolve_key(
    xlib: &x11_dl::xlib::Xlib,
    display: *mut x11_dl::xlib::Display,
    key: &str,
) -> (u8, u64) {
    use std::ffi::CString;
    let Ok(name) = CString::new(key) else {
        return (0, 0);
    };
    let keysym = unsafe { (xlib.XStringToKeysym)(name.as_ptr()) };
    if keysym == 0 {
        return (0, 0);
    }
    let keycode = unsafe { (xlib.XKeysymToKeycode)(display, keysym) };
    (keycode, keysym)
}

unsafe extern "C" fn ignore_x_error(
    _display: *mut x11_dl::xlib::Display,
    _event: *mut x11_dl::xlib::XErrorEvent,
) -> i32 {
    // Une touche déjà prise par le compositeur ou un autre client ne doit pas
    // faire tomber Quark : l'overlay continue sans raccourci.
    0
}

/// Capture l'écran. X11 d'abord : sous Proton, le jeu est une fenêtre X11 et
/// cette voie ne dépend d'aucun outil externe. Wayland natif passe par les
/// outils du bureau quand ils existent.
fn capture_screen(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|_| "dossier de captures inaccessible".to_string())?;
    }
    if std::env::var_os("DISPLAY").is_some() && capture_x11(path).is_ok() {
        return Ok(());
    }
    capture_external(path)
}

fn capture_x11(path: &Path) -> Result<(), String> {
    use x11_dl::xlib;
    let xlib = xlib::Xlib::open().map_err(|_| "X11 indisponible".to_string())?;
    let display = unsafe { (xlib.XOpenDisplay)(std::ptr::null()) };
    if display.is_null() {
        return Err("connexion X11 impossible".to_string());
    }
    let root = unsafe { (xlib.XDefaultRootWindow)(display) };
    let width = unsafe { (xlib.XDisplayWidth)(display, 0) };
    let height = unsafe { (xlib.XDisplayHeight)(display, 0) };
    if width <= 0 || height <= 0 {
        unsafe { (xlib.XCloseDisplay)(display) };
        return Err("taille d'écran inconnue".to_string());
    }
    let image = unsafe {
        (xlib.XGetImage)(
            display,
            root,
            0,
            0,
            width as u32,
            height as u32,
            !0,
            xlib::ZPixmap,
        )
    };
    if image.is_null() {
        unsafe { (xlib.XCloseDisplay)(display) };
        return Err("capture X11 refusée".to_string());
    }
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    let (red_mask, green_mask, blue_mask) =
        unsafe { ((*image).red_mask, (*image).green_mask, (*image).blue_mask) };
    for y in 0..height {
        for x in 0..width {
            let pixel = unsafe { (xlib.XGetPixel)(image, x, y) };
            pixels.push(color_channel(pixel, red_mask));
            pixels.push(color_channel(pixel, green_mask));
            pixels.push(color_channel(pixel, blue_mask));
            pixels.push(255);
        }
    }
    unsafe {
        (xlib.XDestroyImage)(image);
        (xlib.XCloseDisplay)(display);
    }
    image::save_buffer(
        path,
        &pixels,
        width as u32,
        height as u32,
        image::ColorType::Rgba8,
    )
    .map_err(|_| "écriture de la capture impossible".to_string())?;
    Ok(())
}

fn color_channel(pixel: u64, mask: u64) -> u8 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let max = mask >> shift;
    if max == 0 {
        return 0;
    }
    (((pixel & mask) >> shift) * 255 / max) as u8
}

fn capture_external(path: &Path) -> Result<(), String> {
    let mut attempts: Vec<(&str, Vec<String>)> = Vec::new();
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        attempts.push(("grim", vec![path.display().to_string()]));
    }
    attempts.push((
        "gnome-screenshot",
        vec!["-f".into(), path.display().to_string()],
    ));
    attempts.push((
        "spectacle",
        vec![
            "-b".into(),
            "-n".into(),
            "-o".into(),
            path.display().to_string(),
        ],
    ));
    attempts.push(("scrot", vec![path.display().to_string()]));
    attempts.push((
        "import",
        vec!["-window".into(), "root".into(), path.display().to_string()],
    ));
    for (program, args) in attempts {
        let status = std::process::Command::new(program)
            .args(&args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if status.is_ok_and(|status| status.success())
            && std::fs::metadata(path).is_ok_and(|meta| meta.len() > 512)
        {
            return Ok(());
        }
    }
    Err("aucun outil de capture disponible".to_string())
}

/// Petit carillon de réussite, généré une seule fois. Aucune dépendance à un
/// fichier audio du dépôt : le WAV est écrit dans le dossier de données.
pub fn play_chime() {
    let Some(path) = ensure_chime() else {
        return;
    };
    for (program, args) in [
        ("paplay", vec![path.display().to_string()]),
        ("pw-play", vec![path.display().to_string()]),
        ("aplay", vec!["-q".to_string(), path.display().to_string()]),
        (
            "canberra-gtk-play",
            vec!["-f".to_string(), path.display().to_string()],
        ),
    ] {
        let mut command = std::process::Command::new(program);
        command
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if crate::process::spawn_and_reap(&mut command).is_ok() {
            return;
        }
    }
}

fn ensure_chime() -> Option<PathBuf> {
    let path = crate::config::data_dir()
        .join("sounds")
        .join("quark-achievement.wav");
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > 1000) {
        return Some(path);
    }
    let parent = path.parent()?;
    std::fs::create_dir_all(parent).ok()?;
    let samples = chime_samples();
    std::fs::write(&path, wav_bytes(&samples)).ok()?;
    Some(path)
}

pub fn wav_bytes(samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&44_100u32.to_le_bytes());
    out.extend_from_slice(&88_200u32.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

fn chime_samples() -> Vec<i16> {
    let rate = 44_100.0;
    let seconds = 0.42;
    let count = (rate * seconds) as usize;
    let mut samples = Vec::with_capacity(count);
    for index in 0..count {
        let time = index as f64 / rate;
        let envelope = (-time * 7.0).exp();
        let first = (time * 2.0 * std::f64::consts::PI * 880.0).sin();
        let second = (time * 2.0 * std::f64::consts::PI * 1320.0).sin() * 0.6;
        let value = ((first + second) * envelope * 0.18 * i16::MAX as f64) as i16;
        samples.push(value);
    }
    samples
}

/// Notification système discrète, en complément de l'overlay.
pub fn desktop_notification(summary: &str, body: &str) {
    let mut command = std::process::Command::new("notify-send");
    command
        .args(["-a", "Quark", summary, body])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let _ = crate::process::spawn_and_reap(&mut command);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_noms_de_dossier_de_capture_sont_surs() {
        assert_eq!(
            safe_name("The Witcher 3: Wild Hunt"),
            "The-Witcher-3-Wild-Hunt"
        );
        assert_eq!(safe_name("../../etc/passwd"), "etc-passwd");
        assert_eq!(safe_name("ééé"), "jeu");
        assert!(safe_name(&"a".repeat(200)).len() <= 64);
    }

    #[test]
    fn le_chemin_d_une_capture_reste_dans_le_dossier_des_captures() {
        let target = Target {
            name: "Portal 2".into(),
            provider: Some(Provider::Epic),
            key: Some(ProviderGameKey {
                provider: Provider::Epic,
                product_id: "app_123".into(),
            }),
            game_id: "quark-game".into(),
            started: Instant::now(),
        };
        let path = screenshot_path(&target);
        assert!(path.starts_with(screenshots_dir()));
        assert_eq!(path.extension().and_then(|ext| ext.to_str()), Some("png"));
    }

    #[test]
    fn le_raccourci_maj_retour_arriere_est_analyse() {
        use x11_dl::xlib;
        let (modifiers, key) = parse_combo("Shift+BackSpace");
        assert_eq!(modifiers, xlib::ShiftMask);
        assert_eq!(key, "BackSpace");
        let (modifiers, key) = parse_combo("maj+retour arrière");
        assert_eq!(modifiers, xlib::ShiftMask);
        assert_eq!(key, "BackSpace");
        assert_eq!(parse_combo("F12"), (0, "F12".to_string()));
    }

    #[test]
    fn le_carillon_est_un_wav_pcm_lisible() {
        let samples = chime_samples();
        let wav = wav_bytes(&samples);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(
            u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize,
            samples.len() * 2
        );
        assert!(samples.iter().any(|sample| *sample != 0));
    }

    #[test]
    fn l_horloge_reste_lisible_au_dela_d_une_heure() {
        assert_eq!(clock(0), "00:00");
        assert_eq!(clock(65), "01:05");
        assert_eq!(clock(3665), "1:01:05");
    }
}
