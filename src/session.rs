//! Lancement d'un jeu : journal de session et arrêt propre.
//!
//! Chaque lancement crée `~/.local/share/quark/logs/<jeu>/session.log`, dans
//! lequel sont écrits la commande exacte, ses variables d'environnement, puis
//! tout ce que le jeu et Wine affichent. C'est ce qui remplace le terminal :
//! sans capture, un jeu qui refuse de démarrer ne laisse aucune trace.
//!
//! L'arrêt tue le groupe de processus du jeu *et* le `wineserver` du préfixe :
//! sans lui, les processus Windows lancés par le jeu (lanceurs, anti-triche)
//! survivent à la fermeture de la fenêtre.

use crate::config::{self, Game};
use crate::proton::Resolved;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Taille conservée du journal (le reste, plus ancien, est ignoré à l'affichage).
const TAIL_BYTES: u64 = 128 * 1024;

/// Délai avant d'abandonner la politesse et de tuer le groupe de processus.
const KILL_AFTER: Duration = Duration::from_secs(4);

pub struct Session {
    pub child: Child,
    /// Identifiant du chef de groupe (voir `process_group(0)` au lancement).
    pub pid: i32,
    pub started: Instant,
    /// Début de la session en epoch : c'est lui qui date l'historique.
    pub started_ts: i64,
    /// Secondes écoulées, mises à jour à chaque tick.
    pub session: u64,
    pub log: PathBuf,
    pub prefix: Option<PathBuf>,
    /// `wineserver` du runtime utilisé (celui de Proton), pour fermer le
    /// préfixe.
    pub wineserver: Option<PathBuf>,
    /// Arrêt demandé : au-delà de [`KILL_AFTER`], le groupe est tué.
    pub stopping: Option<Instant>,
    /// Sauvegarde locale détectée à partir d'une installation fournisseur liée.
    pub save_identity: Option<crate::saves::SaveIdentity>,
}

impl Session {
    /// Vrai s'il faut envoyer un `SIGKILL` : le jeu a ignoré `SIGTERM`.
    pub fn should_escalate(&self) -> bool {
        self.stopping
            .is_some_and(|since| since.elapsed() >= KILL_AFTER)
    }

    /// Marque l'escalade comme faite (pour ne pas la répéter à chaque image).
    pub fn escalated(&mut self) {
        self.stopping = None;
    }
}

/// Démarre le jeu, journal ouvert et redirigé.
pub fn start(game: &Game, resolved: &Resolved) -> Result<Session, String> {
    let log = config::session_log(&game.id);
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|err| format!("dossier de journaux {} : {err}", dir.display()))?;
    }
    let mut file =
        File::create(&log).map_err(|err| format!("journal {} : {err}", log.display()))?;

    // En-tête : sans lui, un journal ne dit pas quel Proton ni quelles
    // variables ont été utilisées.
    let _ = writeln!(file, "[quark] session du {}", timestamp(config::now_ts()));
    let _ = writeln!(file, "[quark] {}", resolved.label());
    let _ = writeln!(file, "[quark] {}", resolved.spec.command_line());
    if let Some(prefix) = &resolved.prefix {
        let _ = writeln!(file, "[quark] préfixe {}", prefix.display());
    }
    for (name, value) in &resolved.spec.env {
        let _ = writeln!(file, "[quark] env {name}={value}");
    }
    for warning in &resolved.warnings {
        let _ = writeln!(file, "[quark] ! {warning}");
    }
    let _ = writeln!(
        file,
        "--------------------------------------------------------------"
    );
    let _ = file.flush();

    let mut cmd = Command::new(&resolved.spec.program);
    cmd.args(&resolved.spec.args);
    for (name, value) in &resolved.spec.env {
        cmd.env(name, value);
    }
    let workdir = game
        .exe_path()
        .parent()
        .map(Path::to_path_buf)
        .or_else(|| resolved.spec.program.parent().map(Path::to_path_buf))
        .filter(|dir| dir.is_dir());
    if let Some(dir) = workdir {
        cmd.current_dir(dir);
    }
    let stdout = file.try_clone().map_err(|err| err.to_string())?;
    let stderr = file.try_clone().map_err(|err| err.to_string())?;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    // Groupe de processus dédié : l'arrêt peut viser le jeu et ses enfants
    // sans toucher au reste de la session.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let child = cmd.spawn().map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => format!(
            "« {} » introuvable : le programme n'est pas installé ou pas dans le PATH",
            resolved.spec.program.display()
        ),
        _ => err.to_string(),
    })?;
    let pid = child.id() as i32;
    Ok(Session {
        child,
        pid,
        started: Instant::now(),
        started_ts: config::now_ts(),
        session: 0,
        log,
        prefix: resolved.prefix.clone(),
        wineserver: wineserver_for(resolved),
        stopping: None,
        save_identity: None,
    })
}

/// `wineserver` du runtime Proton utilisé, pour fermer son préfixe. Un jeu
/// natif n'en a pas besoin.
fn wineserver_for(resolved: &Resolved) -> Option<PathBuf> {
    let runtime = resolved.runtime.as_ref()?;
    let candidate = runtime.dir().join("files/bin/wineserver");
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

/// Arrête le jeu : `wineserver -k` pour les processus Windows du préfixe, puis
/// `SIGTERM` au groupe. L'escalade en `SIGKILL` a lieu au tick suivant.
pub fn stop(session: &mut Session) {
    session.stopping = Some(Instant::now());

    if let Some(prefix) = session.prefix.clone() {
        // Le wineserver de Proton n'est pas dans le PATH : sans le chemin du
        // runtime, on essaie celui du système, et on ignore l'échec.
        let program = session
            .wineserver
            .clone()
            .unwrap_or_else(|| PathBuf::from("wineserver"));
        let mut cmd = Command::new(program);
        cmd.arg("-k")
            .env("WINEPREFIX", &prefix)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        match cmd.spawn() {
            Ok(mut child) => {
                // Court : `wineserver -k` rend la main très vite, et on ne
                // bloque pas l'interface pour l'attendre.
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            Err(_) => {}
        }
    }

    #[cfg(unix)]
    unsafe {
        libc::kill(-session.pid, libc::SIGTERM);
    }
}

/// Tue le groupe de processus sans ménagement (le jeu ignore SIGTERM).
pub fn force_kill(session: &Session) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-session.pid, libc::SIGKILL);
    }
    #[cfg(not(unix))]
    {
        let _ = &session.pid;
    }
}

/// Dernières lignes du journal, et indicateur de troncature.
pub fn read_log_tail(path: &Path, max_lines: usize) -> Result<(Vec<String>, bool), String> {
    let mut file = File::open(path).map_err(|err| format!("journal illisible : {err}"))?;
    let len = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    let truncated = len > TAIL_BYTES;
    if truncated {
        file.seek(SeekFrom::Start(len - TAIL_BYTES))
            .map_err(|err| err.to_string())?;
    }
    let mut bytes: Vec<u8> = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|err| err.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<String> = text.lines().map(|line| line.to_string()).collect();
    if truncated && !lines.is_empty() {
        // Première ligne probablement coupée au milieu d'un mot.
        lines.remove(0);
    }
    if lines.len() > max_lines {
        lines.drain(..lines.len() - max_lines);
    }
    Ok((lines, truncated))
}

/// Ouvre un dossier dans le gestionnaire de fichiers du bureau.
pub fn open_folder(path: &Path) -> Result<(), String> {
    let target = path.display().to_string();
    for program in ["xdg-open", "gio", "kde-open"] {
        let mut cmd = Command::new(program);
        if program == "gio" {
            cmd.arg("open");
        }
        cmd.arg(&target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        match cmd.spawn() {
            Ok(_) => return Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(format!("{program} : {err}")),
        }
    }
    Err("aucun gestionnaire de fichiers trouvé (xdg-open, gio, kde-open)".to_string())
}

/// Horodatage lisible « 2026-10-28 14:05 UTC » sans dépendance de calendrier.
pub fn timestamp(epoch: i64) -> String {
    let days = epoch.div_euclid(86_400);
    let seconds = epoch.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        seconds / 3600,
        (seconds % 3600) / 60
    )
}

/// Date civile d'un nombre de jours depuis l'époque Unix (algorithme de
/// Howard Hinnant, sans table ni bibliothèque).
pub(crate) fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as i64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_horodatage_est_lisible() {
        assert_eq!(timestamp(0), "1970-01-01 00:00 UTC");
        // 2026-10-28 14:05 UTC
        assert_eq!(timestamp(1_793_196_300), "2026-10-28 14:05 UTC");
    }

    #[test]
    fn la_queue_du_journal_ne_lit_que_la_fin() {
        let dir = std::env::temp_dir().join(format!("quark-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.log");
        let mut text = String::new();
        // Bien au-delà des 128 Kio conservés à l'affichage.
        for index in 0..40_000 {
            text.push_str(&format!("ligne {index}\n"));
        }
        std::fs::write(&path, &text).unwrap();

        let (lines, truncated) = read_log_tail(&path, 40).unwrap();
        assert!(truncated, "un journal long doit être tronqué à l'affichage");
        assert_eq!(lines.len(), 40);
        assert_eq!(lines.last().unwrap(), "ligne 39999");

        // Petit journal : tout est conservé, sans marque de troncature.
        std::fs::write(&path, "début\nfin\n").unwrap();
        let (lines, truncated) = read_log_tail(&path, 40).unwrap();
        assert!(!truncated);
        assert_eq!(lines, ["début", "fin"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn un_journal_absent_est_une_erreur_lisible() {
        let path = std::env::temp_dir().join("quark-journal-absent.log");
        let _ = std::fs::remove_file(&path);
        let err = read_log_tail(&path, 10).unwrap_err();
        assert!(err.contains("illisible"), "{err}");
    }
}
