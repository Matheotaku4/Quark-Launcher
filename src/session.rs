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
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Taille conservée du journal (le reste, plus ancien, est ignoré à l'affichage).
const TAIL_BYTES: u64 = 128 * 1024;
/// Plafond du journal d'une session : au-delà, la sortie continue d'être lue
/// (pour ne pas bloquer le jeu) mais n'est plus écrite.
const MAX_LOG_BYTES: u64 = 4 * 1024 * 1024;

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
    /// Préfixe Wine réel (`<prefix>/pfx` pour Proton, le dossier lui-même
    /// pour umu). C'est lui que `wineserver -k` doit recevoir.
    pub wine_prefix: Option<PathBuf>,
    /// `wineserver` du runtime utilisé (celui de Proton), pour fermer le
    /// préfixe.
    pub wineserver: Option<PathBuf>,
    /// Arrêt demandé : au-delà de [`KILL_AFTER`], le groupe est tué.
    pub stopping: Option<Instant>,
    /// `SIGKILL` déjà envoyé au groupe.
    pub kill_sent: bool,
    /// Sauvegarde locale détectée à partir d'une installation fournisseur liée.
    pub save_identity: Option<crate::saves::SaveIdentity>,
}

impl Session {
    /// Vrai s'il faut envoyer un `SIGKILL` : le jeu a ignoré `SIGTERM`.
    pub fn should_escalate(&self) -> bool {
        !self.kill_sent
            && self
                .stopping
                .is_some_and(|since| since.elapsed() >= KILL_AFTER)
    }

    /// Marque l'escalade comme faite (pour ne pas la répéter à chaque image).
    pub fn escalated(&mut self) {
        self.kill_sent = true;
    }

    /// Après un `SIGKILL`, laisse un court délai au noyau pour récolter le
    /// groupe ; au-delà, on cesse d'attendre.
    pub fn should_abandon(&self) -> bool {
        self.kill_sent
            && self
                .stopping
                .is_some_and(|since| since.elapsed() >= KILL_AFTER + Duration::from_secs(3))
    }

    /// Le groupe de processus existe-t-il encore ? Le chef peut être mort
    /// alors que ses enfants (wrapper, Wine, gamescope) tournent toujours.
    pub fn group_alive(&self) -> bool {
        #[cfg(unix)]
        {
            // `kill(-pgid, 0)` ne signale rien : il teste le groupe. EPERM
            // signifie qu'il existe mais appartient à un autre utilisateur.
            let result = unsafe { libc::kill(-self.pid, 0) };
            result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
        }
        #[cfg(not(unix))]
        {
            false
        }
    }
}

/// Démarre le jeu, journal ouvert et redirigé.
pub fn start(game: &Game, resolved: &Resolved) -> Result<Session, String> {
    let log = config::session_log(&game.id);
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|err| format!("dossier de journaux {} : {err}", dir.display()))?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(&log)
        .map_err(|err| format!("journal {} : {err}", log.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        // Corrige aussi un journal créé par une version précédente.
        let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
    }

    // En-tête : sans lui, un journal ne dit pas quel Proton ni quelles
    // variables ont été utilisées. La ligne de commande complète est omise :
    // un wrapper peut y placer un mot de passe ou un jeton.
    let _ = writeln!(file, "[quark] session du {}", timestamp(config::now_ts()));
    let _ = writeln!(file, "[quark] {}", resolved.label());
    let _ = writeln!(
        file,
        "[quark] programme {} ({} argument(s))",
        resolved.spec.program.display(),
        resolved.spec.args.len()
    );
    if let Some(prefix) = &resolved.prefix {
        let _ = writeln!(file, "[quark] préfixe {}", prefix.display());
    }
    for (name, _value) in &resolved.spec.env {
        // Les valeurs peuvent contenir des identifiants (API_KEY, AUTH,
        // CREDENTIALS…) : seuls les noms sont utiles au diagnostic.
        let _ = writeln!(file, "[quark] env {name}");
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
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Groupe de processus dédié : l'arrêt peut viser le jeu et ses enfants
    // sans toucher au reste de la session.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let mut child = cmd.spawn().map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => format!(
            "« {} » introuvable : le programme n'est pas installé ou pas dans le PATH",
            resolved.spec.program.display()
        ),
        _ => err.to_string(),
    })?;
    let pid = child.id() as i32;
    // La sortie est recopiée par deux threads avec un plafond global : un jeu
    // bavard ne peut plus remplir le disque pendant une longue session.
    let sink = Arc::new(Mutex::new(LogSink {
        file,
        remaining: MAX_LOG_BYTES,
    }));
    if let Some(stdout) = child.stdout.take() {
        let _ = copy_log(stdout, sink.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        let _ = copy_log(stderr, sink);
    }
    Ok(Session {
        child,
        pid,
        started: Instant::now(),
        started_ts: config::now_ts(),
        session: 0,
        log,
        wine_prefix: wine_prefix_for(resolved),
        wineserver: wineserver_for(resolved),
        stopping: None,
        kill_sent: false,
        save_identity: None,
    })
}

struct LogSink {
    file: File,
    remaining: u64,
}

/// Recopie un flux vers le journal sans jamais dépasser [`MAX_LOG_BYTES`]. Le
/// flux est toujours vidé, même après la limite, pour que le jeu ne se bloque
/// pas sur un pipe plein.
fn copy_log(
    mut source: impl Read + Send + 'static,
    sink: Arc<Mutex<LogSink>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        let mut truncated = false;
        loop {
            let Ok(read) = source.read(&mut buffer) else {
                break;
            };
            if read == 0 {
                break;
            }
            let Ok(mut sink) = sink.lock() else {
                break;
            };
            if sink.remaining == 0 {
                if !truncated {
                    let _ = writeln!(
                        sink.file,
                        "[quark] journal tronqué à {} octets",
                        MAX_LOG_BYTES
                    );
                    let _ = sink.file.flush();
                    truncated = true;
                }
                continue;
            }
            let writable = (read as u64).min(sink.remaining) as usize;
            let _ = sink.file.write_all(&buffer[..writable]);
            let _ = sink.file.flush();
            sink.remaining -= writable as u64;
            if sink.remaining == 0 && !truncated {
                let _ = writeln!(
                    sink.file,
                    "[quark] journal tronqué à {} octets",
                    MAX_LOG_BYTES
                );
                let _ = sink.file.flush();
                truncated = true;
            }
        }
    })
}

/// Préfixe Wine réellement utilisé par le runtime : Proton range son
/// `drive_c` dans `<préfixe>/pfx`, umu utilise directement `WINEPREFIX`.
fn wine_prefix_for(resolved: &Resolved) -> Option<PathBuf> {
    let prefix = resolved.prefix.as_ref()?;
    Some(if resolved.kind == crate::proton::LaunchKind::Umu {
        prefix.clone()
    } else {
        prefix.join("pfx")
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
    // Idempotent : recliquer sur « Arrêter » ne doit pas repousser l'échéance
    // du SIGKILL.
    if session.stopping.is_some() {
        return;
    }
    session.stopping = Some(Instant::now());

    if let Some(prefix) = session.wine_prefix.clone() {
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
    if session.group_alive() {
        unsafe {
            libc::kill(-session.pid, libc::SIGTERM);
        }
    }
}

/// Tue le groupe de processus sans ménagement (le jeu ignore SIGTERM).
pub fn force_kill(session: &Session) {
    #[cfg(unix)]
    if session.group_alive() {
        unsafe {
            libc::kill(-session.pid, libc::SIGKILL);
        }
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
        match crate::process::spawn_and_reap(&mut cmd) {
            Ok(()) => return Ok(()),
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
    fn le_wineserver_recoit_le_prefixe_wine_reel() {
        let base = PathBuf::from("/tmp/quark-prefix");
        let spec = crate::proton::LaunchSpec {
            program: PathBuf::from("proton"),
            args: Vec::new(),
            env: Vec::new(),
        };
        let resolved = crate::proton::Resolved {
            kind: crate::proton::LaunchKind::Proton,
            spec: spec.clone(),
            runtime: None,
            prefix: Some(base.clone()),
            anticheat: crate::config::AnticheatMode::None,
            extras: Vec::new(),
            warnings: Vec::new(),
        };
        assert_eq!(wine_prefix_for(&resolved), Some(base.join("pfx")));

        let umu = crate::proton::Resolved {
            kind: crate::proton::LaunchKind::Umu,
            spec,
            runtime: None,
            prefix: Some(base.clone()),
            anticheat: crate::config::AnticheatMode::None,
            extras: Vec::new(),
            warnings: Vec::new(),
        };
        assert_eq!(wine_prefix_for(&umu), Some(base));
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

    #[cfg(unix)]
    #[test]
    fn arreter_deux_fois_ne_repousse_pas_le_sigkill() {
        use std::os::unix::process::CommandExt as _;
        let mut command = Command::new("sleep");
        command.arg("30").stdin(Stdio::null());
        command.process_group(0);
        let child = command.spawn().unwrap();
        let pid = child.id() as i32;
        let mut session = Session {
            child,
            pid,
            started: Instant::now(),
            started_ts: 0,
            session: 0,
            log: PathBuf::new(),
            wine_prefix: None,
            wineserver: None,
            stopping: None,
            kill_sent: false,
            save_identity: None,
        };
        stop(&mut session);
        let deadline = session.stopping.expect("premier arrêt");
        std::thread::sleep(Duration::from_millis(20));
        stop(&mut session);
        assert_eq!(session.stopping, Some(deadline));
        session.stopping = Some(Instant::now() - KILL_AFTER);
        assert!(session.should_escalate());
        session.escalated();
        assert!(!session.should_escalate());
        force_kill(&session);
        let _ = session.child.wait();
    }

    #[test]
    fn le_journal_est_plafonne_et_signale_la_troncature() {
        let path = std::env::temp_dir().join(format!(
            "quark-log-cap-{}-{}",
            std::process::id(),
            config::now_ts()
        ));
        let file = File::create(&path).unwrap();
        let sink = Arc::new(Mutex::new(LogSink {
            file,
            remaining: 10,
        }));
        copy_log(std::io::Cursor::new(vec![b'a'; 100]), sink)
            .join()
            .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("aaaaaaaaaa"));
        assert!(text.contains("journal tronqué"));
        assert!(text.len() < 200);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn un_journal_absent_est_une_erreur_lisible() {
        let path = std::env::temp_dir().join("quark-journal-absent.log");
        let _ = std::fs::remove_file(&path);
        let err = read_log_tail(&path, 10).unwrap_err();
        assert!(err.contains("illisible"), "{err}");
    }
}
