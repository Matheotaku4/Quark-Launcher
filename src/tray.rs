//! Icône de la zone de notification (StatusNotifierItem, via `ksni`).
//!
//! La fenêtre n'essaie plus de se cacher : sur Wayland, cacher une fenêtre
//! n'existe pas (`set_visible` y est sans effet), elle restait donc ouverte et
//! dans la barre des tâches. L'icône vit désormais dans un petit processus
//! dédié (`quark --tray`) : fermer Quark ferme vraiment la fenêtre, l'icône
//! reste, et un clic dessus relance Quark.
//!
//! Aucun GTK n'est nécessaire : `ksni` parle D-Bus directement, ce qui
//! fonctionne sur KDE, XFCE, Cinnamon… et sur GNOME avec l'extension
//! AppIndicator.

use crate::config;
use ksni::blocking::TrayMethods;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Argument qui démarre le processus « icône seule ».
pub const HELPER_ARG: &str = "--tray";

/// Demandes de l'icône, lues par la boucle du processus dédié.
#[derive(Default)]
struct TrayShared {
    show: AtomicBool,
    quit: AtomicBool,
}

/// Icône vivante : la détruire retire l'icône de la zone de notification.
pub struct TrayHandle {
    shared: Arc<TrayShared>,
    handle: ksni::blocking::Handle<QuarkTray>,
}

impl TrayHandle {
    fn spawn() -> Result<Self, String> {
        let shared = Arc::new(TrayShared::default());
        let tray = QuarkTray {
            shared: shared.clone(),
            ctx: eframe::egui::Context::default(),
        };
        let handle = tray.spawn().map_err(|err| err.to_string())?;
        Ok(Self { shared, handle })
    }

    /// Vrai tant que le service n'a pas été arrêté (watcher disparu, etc.).
    fn alive(&self) -> bool {
        !self.handle.is_closed()
    }

    fn take_show(&self) -> bool {
        self.shared.show.swap(false, Ordering::Relaxed)
    }

    fn take_quit(&self) -> bool {
        self.shared.quit.swap(false, Ordering::Relaxed)
    }
}

impl Drop for TrayHandle {
    fn drop(&mut self) {
        let _ = self.handle.shutdown();
    }
}

struct QuarkTray {
    shared: Arc<TrayShared>,
    /// Réveille le launcher : sans cela, une fenêtre réduite ne redessine
    /// jamais et un clic sur l'icône resterait sans effet.
    ctx: eframe::egui::Context,
}

impl QuarkTray {
    fn request_show(&self) {
        self.shared.show.store(true, Ordering::Relaxed);
        self.ctx.request_repaint();
    }

    fn request_quit(&self) {
        self.shared.quit.store(true, Ordering::Relaxed);
        self.ctx.request_repaint();
    }
}

impl ksni::Tray for QuarkTray {
    fn id(&self) -> String {
        "quark".to_string()
    }

    fn title(&self) -> String {
        "Quark".to_string()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![mark_icon(32)]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "Quark".to_string(),
            description: "Launcher de jeux".to_string(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.request_show();
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem {
                label: "Ouvrir Quark".to_string(),
                activate: Box::new(|tray: &mut Self| tray.request_show()),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: "Quitter Quark".to_string(),
                activate: Box::new(|tray: &mut Self| tray.request_quit()),
                ..Default::default()
            }
            .into(),
        ]
    }
}

// ---------------------------------------------------------------------------
// Fichiers de coordination entre les deux processus
// ---------------------------------------------------------------------------

fn tray_pid_path() -> PathBuf {
    config::data_dir().join("tray.pid")
}

fn gui_pid_path() -> PathBuf {
    config::data_dir().join("gui.pid")
}

/// Verrou exclusif non bloquant sur un fichier : il disparaît avec le
/// processus, donc il ne peut pas rester « pris » après un plantage.
fn lock(name: &str) -> Option<std::fs::File> {
    let _ = std::fs::create_dir_all(config::data_dir());
    let file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(config::data_dir().join(name))
        .ok()?;
    let taken = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    taken.then_some(file)
}

/// Une seule fenêtre Quark à la fois : une seconde instance se retire, sinon
/// deux processus écriraient `config.json` en même temps.
pub fn lock_gui() -> Option<std::fs::File> {
    lock("gui.lock")
}

/// Démarrage du processus depuis le boot, lu dans `/proc/<pid>/stat`.
fn process_start_time(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Le nom (`comm`) peut contenir des espaces et des parenthèses : on repart
    // après la dernière `)`.
    let rest = stat.rsplit_once(')')?.1;
    rest.split_whitespace().nth(19)?.parse().ok()
}

fn pid_file_contents() -> String {
    let pid = std::process::id();
    match process_start_time(pid) {
        Some(start) => format!("{pid} {start}"),
        None => pid.to_string(),
    }
}

fn pid_belongs_to_quark(pid: u32, helper: bool) -> bool {
    let Ok(exe) = std::fs::read_link(format!("/proc/{pid}/exe")) else {
        return false;
    };
    let ours = std::env::current_exe()
        .ok()
        .and_then(|path| path.canonicalize().ok());
    if ours.as_deref() != Some(exe.as_path()) {
        return false;
    }
    // La fenêtre et l'icône utilisent le même binaire : la ligne de commande
    // distingue les deux.
    let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    let is_helper = cmdline
        .split(|byte| *byte == 0)
        .any(|arg| arg == crate::tray::HELPER_ARG.as_bytes());
    is_helper == helper
}

fn alive_pid(path: &Path, helper: bool) -> Option<u32> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut parts = text.split_whitespace();
    let pid: i32 = parts.next()?.parse().ok()?;
    let recorded_start: Option<u64> = parts.next().and_then(|value| value.parse().ok());
    if pid <= 0 {
        let _ = std::fs::remove_file(path);
        return None;
    }
    let pid = pid as u32;
    // `kill(pid, 0)` n'envoie aucun signal : il demande seulement si le
    // processus existe encore.
    let alive = unsafe { libc::kill(pid as i32, 0) } == 0
        && pid_belongs_to_quark(pid, helper)
        && recorded_start
            .map(|start| process_start_time(pid) == Some(start))
            .unwrap_or(true);
    if !alive {
        // PID réutilisé, binaire remplacé ou fichier périmé : on le retire.
        let _ = std::fs::remove_file(path);
        return None;
    }
    Some(pid)
}

/// PID du processus « icône seule », s'il tourne encore.
pub fn helper_pid() -> Option<u32> {
    alive_pid(&tray_pid_path(), true)
}

/// PID de la fenêtre Quark ouverte, s'il y en a une.
pub fn gui_pid() -> Option<u32> {
    alive_pid(&gui_pid_path(), false)
}

/// Inscrit la fenêtre courante : l'icône saura qu'elle n'a rien à relancer.
pub fn write_gui_pid() {
    let _ = std::fs::create_dir_all(config::data_dir());
    let _ = std::fs::write(gui_pid_path(), pid_file_contents());
}

pub fn clear_gui_pid() {
    let _ = std::fs::remove_file(gui_pid_path());
}

/// Démarre l'icône si elle ne tourne pas déjà. Renvoie son PID dès qu'elle a
/// pris la parole sur le bus ; sinon l'erreur explique que le bureau n'a pas de
/// zone de notification.
pub fn ensure_helper() -> Result<u32, String> {
    if let Some(pid) = helper_pid() {
        return Ok(pid);
    }
    let exe = std::env::current_exe().map_err(|err| format!("chemin du binaire : {err}"))?;
    let _ = std::fs::create_dir_all(config::data_dir());
    // Un pidfile périmé ferait croire à une icône vivante.
    let _ = std::fs::remove_file(tray_pid_path());
    let mut command = std::process::Command::new(exe);
    command
        .arg(HELPER_ARG)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    crate::process::spawn_and_reap(&mut command)
        .map_err(|err| format!("démarrage de l'icône : {err}"))?;
    // Le processus doit s'enregistrer sur D-Bus avant que l'on ferme la
    // fenêtre : sans cela, une icône qui échoue laisserait Quark fermé sans
    // aucun moyen de le rouvrir.
    let deadline = Instant::now() + Duration::from_millis(900);
    while Instant::now() < deadline {
        if let Some(pid) = helper_pid() {
            return Ok(pid);
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    Err(
        "aucune zone de notification : ce bureau n'expose pas de watcher \
         StatusNotifierItem (extension AppIndicator sous GNOME)"
            .to_string(),
    )
}

/// Retire l'icône (réglage décoché).
pub fn stop_helper() {
    if let Some(pid) = helper_pid() {
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }
    let _ = std::fs::remove_file(tray_pid_path());
}

fn spawn_gui() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut command = std::process::Command::new(exe);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let _ = crate::process::spawn_and_reap(&mut command);
}

// ---------------------------------------------------------------------------
// Processus « icône seule » : `quark --tray`
// ---------------------------------------------------------------------------

/// Boucle du processus dédié à l'icône. Renvoie le code de sortie.
pub fn run_helper() -> i32 {
    // Deux icônes dans la zone de notification seraient déroutantes.
    let Some(_lock) = lock("tray.lock") else {
        return 0;
    };
    let tray = match TrayHandle::spawn() {
        Ok(tray) => tray,
        Err(err) => {
            eprintln!("[quark] icône indisponible : {err}");
            return 1;
        }
    };
    let pid_path = tray_pid_path();
    let _ = std::fs::write(&pid_path, pid_file_contents());

    loop {
        if tray.take_show() && gui_pid().is_none() {
            spawn_gui();
        }
        if tray.take_quit() {
            if let Some(pid) = gui_pid() {
                unsafe {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
            }
            let _ = std::fs::remove_file(&pid_path);
            return 0;
        }
        if !tray.alive() {
            // Le bureau a retiré la zone de notification : plus rien à tenir.
            let _ = std::fs::remove_file(&pid_path);
            return 0;
        }
        std::thread::sleep(Duration::from_millis(80));
    }
}

/// La marque de Quark (anneau bleu + point), dessinée en ARGB32 : l'icône ne
/// dépend ainsi d'aucun thème installé.
fn mark_icon(size: i32) -> ksni::Icon {
    let s = size.clamp(8, 256) as usize;
    let mut data = vec![0u8; s * s * 4];
    let center = s as f32 / 2.0;
    let r_out = s as f32 * 0.44;
    let r_in = s as f32 * 0.29;
    let ring_mid = (r_out + r_in) / 2.0;
    let ring_half = (r_out - r_in) / 2.0;
    let dot_radius = s as f32 * 0.13;
    for y in 0..s {
        for x in 0..s {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            let distance = (dx * dx + dy * dy).sqrt();
            let in_ring = (distance - ring_mid).abs() <= ring_half;
            // Le point est décalé comme sur la marque de la barre du haut.
            let ox = dx - s as f32 * 0.16;
            let oy = dy - s as f32 * 0.09;
            let in_dot = (ox * ox + oy * oy).sqrt() <= dot_radius;
            let (r, g, b) = if in_ring {
                (0x1A, 0x9F, 0xFF)
            } else if in_dot {
                (0x3F, 0xB0, 0xFF)
            } else {
                continue;
            };
            let index = (y * s + x) * 4;
            data[index] = 0xFF;
            data[index + 1] = r;
            data[index + 2] = g;
            data[index + 3] = b;
        }
    }
    ksni::Icon {
        width: s as i32,
        height: s as i32,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::mark_icon;

    #[test]
    fn l_icone_a_la_taille_demandee() {
        let icon = mark_icon(32);
        assert_eq!(icon.width, 32);
        assert_eq!(icon.height, 32);
        assert_eq!(icon.data.len(), 32 * 32 * 4);
    }

    #[test]
    fn l_icone_est_transparente_au_centre_sauf_le_point() {
        let icon = mark_icon(32);
        // Coin supérieur gauche : transparent (rien n'est dessiné).
        assert_eq!(icon.data[0], 0);
        // Le point est décalé en bas à droite du centre : il est opaque.
        let index = ((32 * 19 + 21) * 4) as usize;
        assert_eq!(icon.data[index], 0xFF);
    }
}
