//! Overlay en jeu : petit panneau toujours au-dessus, captures d'écran et
//! notifications.
//!
//! Steam affiche un HUD pendant les parties et capture F12 sans prévenir le
//! jeu. Quark fait la même chose pour tous les jeux qu'il lance : un raccourci
//! X11 global (XWayland compris, donc les jeux Proton) prend une capture,
//! l'écrit dans `~/.local/share/quark/screenshots/<jeu>/`, puis l'overlay
//! affiche l'événement. Les jeux Epic et GOG reçoivent en plus un son et une
//! notification système, comme le client de Valve.
//!
//! Rien ici n'est un secret : les captures restent locales, et le thread du
//! raccourci n'écrit jamais dans le journal de session.

use crate::providers::Provider;
use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2, ViewportBuilder, ViewportClass,
    ViewportId,
};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel as mpsc_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Jeu visé par une capture : figé au moment de l'appui, pour que la capture
/// porte le bon nom même si l'utilisateur change de jeu entre-temps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub name: String,
    pub provider: Option<Provider>,
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

pub struct Overlay {
    pub capture_rx: Option<Receiver<CaptureEvent>>,
    hotkey_stop: Option<Sender<()>>,
    hotkey_thread: Option<std::thread::JoinHandle<()>>,
    shared: Arc<Mutex<Option<Target>>>,
    pub target: Option<Target>,
    pub notices: VecDeque<Notice>,
    pub last_capture: Option<PathBuf>,
    hotkey_key: String,
}

impl Default for Overlay {
    fn default() -> Self {
        Self {
            capture_rx: None,
            hotkey_stop: None,
            hotkey_thread: None,
            shared: Arc::new(Mutex::new(None)),
            target: None,
            notices: VecDeque::new(),
            last_capture: None,
            hotkey_key: String::new(),
        }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        self.stop_hotkey();
    }
}

impl Overlay {
    /// Déclare le jeu en cours. Le thread du raccourci lit cette cible au
    /// moment de l'appui ; il est démarré une seule fois.
    pub fn set_game(&mut self, target: Target, screenshot_key: &str) {
        if self.hotkey_key != screenshot_key {
            self.stop_hotkey();
            if let Some((rx, stop, thread)) =
                start_hotkey(screenshot_key, self.shared.clone())
            {
                self.capture_rx = Some(rx);
                self.hotkey_stop = Some(stop);
                self.hotkey_thread = Some(thread);
            }
            self.hotkey_key = screenshot_key.to_string();
        }
        if let Ok(mut shared) = self.shared.lock() {
            *shared = Some(target.clone());
        }
        self.target = Some(target);
    }

    pub fn clear_game(&mut self) {
        // Sans cela, F12 (ou la touche choisie) restait prise globalement
        // après la fin du jeu, et chaque changement de touche fuyait un thread.
        self.stop_hotkey();
        self.hotkey_key.clear();
        if let Ok(mut shared) = self.shared.lock() {
            *shared = None;
        }
        self.target = None;
    }

    fn stop_hotkey(&mut self) {
        if let Some(stop) = self.hotkey_stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.hotkey_thread.take() {
            let _ = thread.join();
        }
        self.capture_rx = None;
    }

    /// Récupère les captures terminées et les transforme en événements pour
    /// l'overlay. Le son et la notification sont décidés par l'appelant, qui
    /// connaît la boutique du jeu.
    pub fn poll_captures(&mut self) -> Vec<CaptureEvent> {
        let Some(rx) = self.capture_rx.as_ref() else {
            return Vec::new();
        };
        let mut events = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(event) => events.push(event),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.capture_rx = None;
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
        self.notices.truncate(6);
    }

    /// Affiche l'overlay dans une fenêtre séparée, transparente, toujours
    /// au-dessus et qui laisse passer la souris. La fenêtre se ferme dès que
    /// l'overlay n'est plus demandé.
    pub fn ui(&mut self, ctx: &egui::Context, pal: &crate::theme::Palette, key: &str) {
        let Some(target) = self.target.clone() else {
            return;
        };
        self.notices
            .retain(|notice| notice.created.elapsed() < Duration::from_secs(14));
        let monitor = ctx.input(|input| input.viewport().monitor_size);
        let position = monitor
            .map(|size| Pos2::new((size.x - 396.0).max(16.0), 24.0))
            .unwrap_or(Pos2::new(24.0, 24.0));
        let builder = ViewportBuilder::default()
            .with_title("Quark Overlay")
            .with_inner_size(Vec2::new(380.0, 196.0))
            .with_position(position)
            .with_decorations(false)
            .with_resizable(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_mouse_passthrough(true)
            .with_active(false)
            .with_taskbar(false);
        let elapsed = target.started.elapsed();
        let notices: Vec<Notice> = self.notices.iter().take(3).cloned().collect();
        let game = target.name.clone();
        let provider = target.provider;
        let key = key.to_string();
        let pal = *pal;
        ctx.show_viewport_immediate(
            ViewportId::from_hash_of("quark-game-overlay"),
            builder,
            move |ctx, class| {
                if class == ViewportClass::Embedded {
                    return;
                }
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(Color32::TRANSPARENT))
                    .show(ctx, |ui| {
                        draw_overlay(ui, &pal, &game, provider, elapsed, &notices, &key);
                    });
            },
        );
    }
}

fn draw_overlay(
    ui: &mut egui::Ui,
    pal: &crate::theme::Palette,
    game: &str,
    provider: Option<Provider>,
    elapsed: Duration,
    notices: &[Notice],
    key: &str,
) {
    let size = ui.available_size();
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    let card = Rect::from_min_max(
        rect.min + Vec2::new(8.0, 8.0),
        rect.max - Vec2::new(8.0, 8.0),
    );
    painter.rect_filled(card, 14.0, Color32::from_black_alpha(226));
    painter.rect_stroke(
        card,
        14.0,
        Stroke::new(1.0_f32, Color32::from_white_alpha(28)),
        egui::StrokeKind::Inside,
    );
    painter.rect_filled(
        Rect::from_min_size(card.min, Vec2::new(3.0, card.height())),
        0.0,
        pal.accent,
    );

    let left = card.left() + 16.0;
    painter.text(
        Pos2::new(left, card.top() + 16.0),
        Align2::LEFT_TOP,
        "EN JEU",
        FontId::proportional(9.5),
        pal.accent,
    );
    painter.text(
        Pos2::new(card.right() - 14.0, card.top() + 14.0),
        Align2::RIGHT_TOP,
        clock(elapsed.as_secs()),
        FontId::monospace(15.0),
        Color32::WHITE,
    );
    painter.text(
        Pos2::new(left, card.top() + 34.0),
        Align2::LEFT_TOP,
        game,
        FontId::proportional(15.0),
        Color32::WHITE,
    );
    if let Some(provider) = provider {
        let label = match provider {
            Provider::Steam => "STEAM",
            Provider::Epic => "EPIC",
            Provider::Gog => "GOG",
        };
        let color = match provider {
            Provider::Steam => Color32::from_rgb(0x8F, 0xB8, 0xE8),
            Provider::Epic => Color32::from_rgb(0xC8, 0xC8, 0xD2),
            Provider::Gog => Color32::from_rgb(0xB9, 0x8B, 0xE8),
        };
        painter.text(
            Pos2::new(card.right() - 14.0, card.top() + 34.0),
            Align2::RIGHT_TOP,
            label,
            FontId::proportional(9.5),
            color,
        );
    }

    let mut y = card.top() + 64.0;
    if notices.is_empty() {
        painter.text(
            Pos2::new(left, y),
            Align2::LEFT_TOP,
            "Prêt · les notifications apparaissent ici",
            FontId::proportional(11.0),
            Color32::from_white_alpha(150),
        );
    }
    for notice in notices {
        let age = notice.created.elapsed().as_secs_f32();
        let alpha = (1.0 - (age - 10.0).max(0.0) / 4.0).clamp(0.0, 1.0);
        let color = |value: u8| Color32::from_white_alpha((value as f32 * alpha) as u8);
        painter.circle_filled(
            Pos2::new(left + 3.0, y + 6.0),
            2.5,
            pal.accent.gamma_multiply(alpha),
        );
        painter.text(
            Pos2::new(left + 12.0, y),
            Align2::LEFT_TOP,
            &notice.title,
            FontId::proportional(11.5),
            color(235),
        );
        painter.text(
            Pos2::new(left + 12.0, y + 15.0),
            Align2::LEFT_TOP,
            &notice.detail,
            FontId::proportional(10.0),
            color(155),
        );
        y += 34.0;
    }

    painter.text(
        Pos2::new(left, card.bottom() - 22.0),
        Align2::LEFT_TOP,
        format!("{key} · capture d'écran   ·   succès en direct"),
        FontId::proportional(9.5),
        Color32::from_white_alpha(120),
    );
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

/// Lance le thread d'écoute X11. Une seule instance par processus : le
/// raccourci est repris à chaque appui, et la cible est relue dans la mémoire
/// partagée. Sans serveur X (Wayland pur, session distante), il ne se passe
/// rien et l'overlay reste utilisable pour les succès.
fn start_hotkey(
    key: &str,
    shared: Arc<Mutex<Option<Target>>>,
) -> Option<(Receiver<CaptureEvent>, Sender<()>, std::thread::JoinHandle<()>)> {
    let (tx, rx) = mpsc_channel();
    let (stop_tx, stop_rx) = mpsc_channel();
    let key = key.trim().to_string();
    // Mémorise le thread principal : le gestionnaire d'erreur d'I/O X11 ne
    // doit interrompre que le thread auxiliaire, pas toute l'application.
    MAIN_THREAD.store(unsafe { libc::pthread_self() } as usize, Ordering::SeqCst);
    let thread = std::thread::Builder::new()
        .name("quark-hotkey".into())
        .spawn(move || {
            if let Err(error) = hotkey_loop(&key, shared, tx, stop_rx) {
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
        unsafe { libc::pthread_exit(std::ptr::null_mut()) };
    }
    0
}

fn hotkey_loop(
    key: &str,
    shared: Arc<Mutex<Option<Target>>>,
    tx: Sender<CaptureEvent>,
    stop: Receiver<()>,
) -> Result<(), String> {
    use x11_dl::xlib;
    let xlib = xlib::Xlib::open().map_err(|_| "X11 indisponible".to_string())?;
    let display = unsafe { (xlib.XOpenDisplay)(std::ptr::null()) };
    if display.is_null() {
        return Err("connexion X11 impossible".to_string());
    }
    unsafe {
        (xlib.XSetErrorHandler)(Some(ignore_x_error));
        (xlib.XSetIOErrorHandler)(Some(x_io_error));
    }
    let root = unsafe { (xlib.XDefaultRootWindow)(display) };
    let (keycode, keysym) = resolve_key(&xlib, display, key);
    if keycode == 0 {
        unsafe { (xlib.XCloseDisplay)(display) };
        return Err(format!("touche {key} inconnue"));
    }
    let _ = keysym;
    // Verr. Maj et Verr. Num changent les modificateurs : les quatre
    // combinaisons sont prises pour que F12 marche quel que soit leur état.
    for modifiers in [
        0,
        xlib::LockMask,
        xlib::Mod2Mask,
        xlib::LockMask | xlib::Mod2Mask,
    ] {
        unsafe {
            (xlib.XGrabKey)(
                display,
                keycode as i32,
                modifiers,
                root,
                0,
                xlib::GrabModeAsync,
                xlib::GrabModeAsync,
            );
        }
    }
    unsafe { (xlib.XSync)(display, 0) };

    let mut event: xlib::XEvent = unsafe { std::mem::zeroed() };
    let mut last_capture: Option<Instant> = None;
    // Boucle non bloquante : `XNextEvent` ne permettait ni arrêt ni fermeture
    // propre du Display.
    while stop.try_recv().is_err() {
        while unsafe { (xlib.XPending)(display) } > 0 {
            unsafe { (xlib.XNextEvent)(display, &mut event) };
            let kind = unsafe { event.type_ };
            let pressed = unsafe { event.key.keycode } as i32;
            if kind == xlib::KeyPress && pressed == keycode as i32 {
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
                        let _ = sender.send(CaptureEvent {
                            path,
                            game_name: target.name.clone(),
                            provider: target.provider,
                        });
                    }
                });
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    for modifiers in [
        0,
        xlib::LockMask,
        xlib::Mod2Mask,
        xlib::LockMask | xlib::Mod2Mask,
    ] {
        unsafe {
            (xlib.XUngrabKey)(display, keycode as i32, modifiers, root);
        }
    }
    unsafe {
        (xlib.XSync)(display, 0);
        (xlib.XCloseDisplay)(display);
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
            started: Instant::now(),
        };
        let path = screenshot_path(&target);
        assert!(path.starts_with(screenshots_dir()));
        assert_eq!(path.extension().and_then(|ext| ext.to_str()), Some("png"));
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
