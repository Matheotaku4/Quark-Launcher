//! Isolated subprocess contract for managed provider tools.

use std::ffi::OsString;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread;
use std::time::{Duration, Instant};

const MAX_STREAM_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ProviderCommand {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
    pub cwd: Option<PathBuf>,
    pub secrets: Vec<String>,
    pub discard_output: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

/// Motif lisible d'un échec d'outil : la dernière ligne de sortie non vide,
/// bornée, et sans les lignes qui pourraient citer une adresse de compte. Sans
/// elle, un échec d'installation ne disait rien d'utile.
pub fn failure_hint(output: &CommandOutput) -> String {
    let line = output
        .stderr
        .lines()
        .chain(output.stdout.lines())
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.contains('@'))
        .next_back();
    let Some(line) = line else {
        return match output.exit_code {
            Some(code) => format!("code de sortie {code}"),
            None => "aucune sortie".to_string(),
        };
    };
    let line = line.trim_start_matches(|c: char| c == '[' || c == ' ');
    let mut hint: String = line.chars().take(160).collect();
    if line.chars().count() > 160 {
        hint.push('\u{2026}');
    }
    hint
}

/// Avancement d'un téléchargement de boutique, partagé entre le processus
/// fournisseur et l'interface.
#[derive(Debug, Clone, PartialEq)]
pub struct InstallProgress {
    /// Pourcentage 0–100, quand la boutique en publie un.
    pub percent: Option<f32>,
    /// Étape lisible : « Téléchargement », « Installation »…
    pub stage: String,
    /// Débit publié par l'outil, déjà mis en forme.
    pub speed: Option<String>,
    pub eta: Option<String>,
    pub written: Option<u64>,
    pub total: Option<u64>,
}

impl Default for InstallProgress {
    fn default() -> Self {
        Self {
            percent: None,
            stage: "Préparation…".to_string(),
            speed: None,
            eta: None,
            written: None,
            total: None,
        }
    }
}

impl InstallProgress {
    pub fn label(&self) -> String {
        match self.percent {
            Some(percent) => {
                let percent = format!("{percent:.1}").replace('.', ",");
                format!("{} · {percent} %", self.stage)
            }
            None => self.stage.clone(),
        }
    }

    pub fn sizes(&self) -> Option<String> {
        let total = self.total?;
        let written = self.written.unwrap_or(0);
        if total == 0 {
            return None;
        }
        Some(format!(
            "{} Go / {} Go au total",
            gigabytes(written),
            gigabytes(total)
        ))
    }
}

fn gigabytes(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1_073_741_824.0).replace('.', ",")
}

/// Lit un pourcentage et des octets dans une ligne de progression. Legendary
/// écrit `Progress: 42.41% (5.42/12.78 GiB)` ; gogdl écrit
/// `Progress: 42.41 4472934400/10536185856, … ETA: 00:02:11`. Les deux formes
/// passent par ici.
fn progress_sample(line: &str) -> Option<(Option<f32>, Option<u64>, Option<u64>, Option<String>)> {
    let lower = line.to_ascii_lowercase();
    if !lower.contains("progress") && !lower.contains("%") {
        return None;
    }
    let mut percent = None;
    let mut written = None;
    let mut total = None;
    let mut eta = None;

    if let Some(rest) = line.split("Progress:").nth(1) {
        let rest = rest.trim();
        let token = rest.split_whitespace().next().unwrap_or_default();
        let number = token.trim_end_matches('%').replace(',', ".");
        if let Ok(value) = number.parse::<f32>() {
            if (0.0..=100.0).contains(&value) {
                percent = Some(value);
            }
        }
        // `(5.42/12.78 GiB)` : la taille est en GiB, séparée par des espaces.
        if let Some(start) = rest.find('(') {
            if let Some(end) = rest[start..].find(')') {
                let inside = &rest[start + 1..start + end];
                if let Some((left, right)) = inside.split_once('/') {
                    let unit = if inside.to_ascii_lowercase().contains("gib") {
                        1_073_741_824.0
                    } else {
                        1.0
                    };
                    let left = left
                        .trim()
                        .trim_end_matches(|c: char| c.is_ascii_alphabetic())
                        .trim()
                        .replace(',', ".")
                        .parse::<f64>()
                        .ok();
                    let right = right
                        .trim_end_matches(|c: char| c.is_ascii_alphabetic())
                        .trim()
                        .replace(',', ".")
                        .parse::<f64>()
                        .ok();
                    if let (Some(left), Some(right)) = (left, right) {
                        written = Some((left * unit).round().max(0.0) as u64);
                        total = Some((right * unit).round().max(0.0) as u64);
                    }
                }
            }
        } else if let Some((left, right)) = rest.split_once('/') {
            // gogdl : « 4472934400/10536185856, » en octets bruts.
            let left = left
                .split_whitespace()
                .next_back()
                .and_then(|value| value.parse::<u64>().ok());
            let right = right
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|value| value.parse::<u64>().ok());
            written = left;
            total = right;
        }
    }
    if let Some(index) = line.find("ETA:") {
        let value = line[index + 4..]
            .split([',', ' '])
            .find(|part| !part.trim().is_empty())
            .map(str::trim)
            .filter(|value| value.chars().any(|c| c.is_ascii_digit()));
        eta = value.map(str::to_string);
    }
    if let Some(index) = line.find('%') {
        if percent.is_none() {
            let before = &line[..index];
            let token = before
                .split_whitespace()
                .next_back()
                .unwrap_or_default()
                .trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.');
            if let Ok(value) = token.replace(',', ".").parse::<f32>() {
                if (0.0..=100.0).contains(&value) {
                    percent = Some(value);
                }
            }
        }
    }
    (percent.is_some() || written.is_some() || eta.is_some())
        .then_some((percent, written, total, eta))
}

/// Met à jour l'avancement à partir d'une ligne de sortie de Legendary ou de
/// gogdl. Le pourcentage ne recule jamais : une reprise de fichier ferait
/// clignoter la barre de 40 % à 3 %.
pub fn apply_progress(progress: &mut InstallProgress, line: &str) {
    let lower = line.to_ascii_lowercase();
    let stage = if lower.contains("download") && !lower.contains("downloaded:") {
        Some("Téléchargement")
    } else if lower.contains("install") || lower.contains("extract") || lower.contains("decompress")
    {
        Some("Installation")
    } else if lower.contains("verif") || lower.contains("hash") || lower.contains("validat") {
        Some("Vérification")
    } else if lower.contains("finish") || lower.contains("clean") || lower.contains("prereq") {
        Some("Finalisation")
    } else {
        None
    };
    if let Some(stage) = stage {
        progress.stage = stage.to_string();
    }
    if let Some((percent, written, total, eta)) = progress_sample(line) {
        if stage.is_none() && (percent.is_some() || written.is_some()) {
            progress.stage = "Téléchargement".to_string();
        }
        if let Some(percent) = percent {
            let current = progress.percent.unwrap_or(0.0);
            progress.percent = Some(current.max(percent).min(100.0));
        }
        if written.is_some() {
            progress.written = written;
        }
        if total.is_some() {
            progress.total = total;
        }
        if eta.is_some() {
            progress.eta = eta;
        }
    }
    if lower.contains("mib/s") {
        if let Some(index) = lower.find("mib/s") {
            let before = &line[..index];
            let value = before
                .split_whitespace()
                .next_back()
                .filter(|value| value.chars().any(|c| c.is_ascii_digit()));
            if let Some(value) = value {
                progress.speed = Some(format!("{value} Mo/s"));
            }
        }
    }
}

pub fn run(
    command: ProviderCommand,
    timeout: Duration,
    cancellation: Option<Arc<AtomicBool>>,
) -> Result<CommandOutput, String> {
    run_inner(command, timeout, cancellation, None)
}

/// Comme [`run`], en publiant l'avancement du téléchargement au fil de l'eau.
pub fn run_streaming(
    command: ProviderCommand,
    timeout: Duration,
    cancellation: Option<Arc<AtomicBool>>,
    progress: Arc<Mutex<InstallProgress>>,
) -> Result<CommandOutput, String> {
    run_inner(command, timeout, cancellation, Some(progress))
}

fn run_inner(
    command: ProviderCommand,
    timeout: Duration,
    cancellation: Option<Arc<AtomicBool>>,
    progress: Option<Arc<Mutex<InstallProgress>>>,
) -> Result<CommandOutput, String> {
    if cancellation
        .as_ref()
        .is_some_and(|flag| flag.load(Ordering::SeqCst))
    {
        return Err("commande annulée avant son démarrage".to_string());
    }
    let mut process = Command::new(&command.program);
    process
        .args(&command.args)
        .envs(command.env.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = &command.cwd {
        process.current_dir(cwd);
    }
    let mut child = process
        .spawn()
        .map_err(|_| "impossible de démarrer la commande fournisseur".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "stdout fournisseur indisponible".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "stderr fournisseur indisponible".to_string())?;
    let (stdout_tx, stdout_rx) = mpsc::channel();
    let (stderr_tx, stderr_rx) = mpsc::channel();
    let stdout_progress = progress.clone();
    thread::spawn(move || {
        let mut stdout = stdout;
        let result = read_stream(&mut stdout, stdout_progress.as_ref());
        let _ = stdout_tx.send(result);
    });
    let stderr_progress = progress.clone();
    thread::spawn(move || {
        let mut stderr = stderr;
        let result = read_stream(&mut stderr, stderr_progress.as_ref());
        let _ = stderr_tx.send(result);
    });

    let started = Instant::now();
    let status = loop {
        if cancellation
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::SeqCst))
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err("commande fournisseur annulée".to_string());
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err("commande fournisseur dépassant le délai autorisé".to_string());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(
                Duration::from_millis(10).min(timeout.saturating_sub(started.elapsed())),
            ),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("impossible de vérifier l'état de la commande fournisseur".to_string());
            }
        }
    };
    let remaining = timeout
        .saturating_sub(started.elapsed())
        .max(Duration::from_millis(1));
    let stdout = stdout_rx
        .recv_timeout(remaining)
        .map_err(|_| "lecture de stdout fournisseur dépassant le délai autorisé".to_string())?
        .map_err(|_| "sortie stdout fournisseur trop volumineuse ou illisible".to_string())?;
    let remaining = timeout
        .saturating_sub(started.elapsed())
        .max(Duration::from_millis(1));
    let stderr = stderr_rx
        .recv_timeout(remaining)
        .map_err(|_| "lecture de stderr fournisseur dépassant le délai autorisé".to_string())?
        .map_err(|_| "sortie stderr fournisseur trop volumineuse ou illisible".to_string())?;
    let stdout = String::from_utf8_lossy(&stdout).into_owned();
    let stderr = String::from_utf8_lossy(&stderr).into_owned();
    Ok(CommandOutput {
        stdout: if command.discard_output {
            String::new()
        } else {
            crate::providers::credentials::redact_secrets(&stdout, &command.secrets)
        },
        stderr: if command.discard_output {
            String::new()
        } else {
            crate::providers::credentials::redact_secrets(&stderr, &command.secrets)
        },
        exit_code: status.code(),
    })
}

/// Lit un flux en découpant sur `\n` et `\r` (les barres de progression des
/// outils réécrivent la même ligne), en conservant la sortie brute pour les
/// erreurs. Le dépassement du plafond est signalé comme avant ; l'avancement,
/// lui, continue d'être lu jusqu'à la fin du processus.
fn read_stream(
    reader: &mut impl Read,
    progress: Option<&Arc<Mutex<InstallProgress>>>,
) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut too_large = false;
    let mut line: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        if bytes.len() as u64 >= MAX_STREAM_BYTES {
            too_large = true;
        } else {
            let room = MAX_STREAM_BYTES as usize - bytes.len();
            bytes.extend_from_slice(&chunk[..read.min(room)]);
        }
        for byte in &chunk[..read] {
            if *byte == b'\n' || *byte == b'\r' {
                if !line.is_empty() {
                    if let Some(progress) = progress {
                        let text = String::from_utf8_lossy(&line);
                        if let Ok(mut state) = progress.lock() {
                            apply_progress(&mut state, &text);
                        }
                    }
                    line.clear();
                }
            } else if line.len() < 16 * 1024 {
                line.push(*byte);
            }
        }
    }
    if !line.is_empty() {
        if let Some(progress) = progress {
            let text = String::from_utf8_lossy(&line);
            if let Ok(mut state) = progress.lock() {
                apply_progress(&mut state, &text);
            }
        }
    }
    if too_large {
        return Err(std::io::Error::other("sortie fournisseur trop volumineuse"));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn cmd(args: &[&str]) -> ProviderCommand {
        ProviderCommand {
            program: PathBuf::from("/bin/sh"),
            args: args.iter().map(OsString::from).collect(),
            env: vec![],
            cwd: None,
            secrets: vec![],
            discard_output: false,
        }
    }

    #[test]
    fn runner_preserves_argument_boundaries_and_reports_exit_status() {
        let output = run(
            cmd(&[
                "-c",
                "printf '%s' \"$1\"; exit 7",
                "runner",
                "$(touch nope); value",
            ]),
            Duration::from_secs(1),
            None,
        )
        .unwrap();
        assert_eq!(output.stdout, "$(touch nope); value");
        assert_eq!(output.exit_code, Some(7));
    }

    #[test]
    fn runner_applies_working_directory_and_explicit_environment() {
        let root = std::env::temp_dir().join(format!(
            "quark-provider-runner-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut command = cmd(&["-c", "printf '%s|%s' \"$PWD\" \"$QUARK_TEST_VALUE\""]);
        command.cwd = Some(root.clone());
        command
            .env
            .push(("QUARK_TEST_VALUE".into(), "kept as one value".into()));
        let output = run(command, Duration::from_secs(1), None).unwrap();
        assert_eq!(
            output.stdout,
            format!("{}|kept as one value", root.display())
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runner_redacts_registered_secrets_from_both_streams() {
        let mut command = cmd(&[
            "-c",
            "printf 'token=%s' secret-value; printf 'err=%s' secret-value >&2",
        ]);
        command.secrets.push("secret-value".into());
        let output = run(command, Duration::from_secs(1), None).unwrap();
        assert_eq!(output.stdout, "token=[REDACTED]");
        assert_eq!(output.stderr, "err=[REDACTED]");
    }

    #[test]
    fn runner_stops_a_command_that_exceeds_its_deadline() {
        let mut command = cmd(&["2"]);
        command.program = PathBuf::from("/bin/sleep");
        let error = run(command, Duration::from_millis(30), None).unwrap_err();
        assert!(error.contains("délai"));
    }

    #[test]
    fn runner_cancels_a_running_child() {
        let cancellation = Arc::new(AtomicBool::new(false));
        let signal = cancellation.clone();
        let mut command = cmd(&["3"]);
        command.program = PathBuf::from("/bin/sleep");
        let thread =
            thread::spawn(move || run(command, Duration::from_secs(5), Some(cancellation)));
        thread::sleep(Duration::from_millis(30));
        signal.store(true, Ordering::SeqCst);
        assert!(thread.join().unwrap().unwrap_err().contains("annul"));
    }

    #[test]
    fn runner_can_discard_sensitive_tool_output_entirely() {
        let mut command = cmd(&["-c", "printf '{\\\"access_token\\\":\\\"secret\\\"}'"]);
        command.discard_output = true;
        let output = run(command, Duration::from_secs(1), None).unwrap();
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }

    #[test]
    fn un_echec_cite_la_derniere_ligne_utile_sans_adresse_de_compte() {
        let output = CommandOutput {
            stdout: String::new(),
            stderr: "[Core] INFO: Logging in as player@example.com\n[Core] ERROR: disk full\n\n"
                .to_string(),
            exit_code: Some(1),
        };
        assert_eq!(failure_hint(&output), "Core] ERROR: disk full");
        let silent = CommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(3),
        };
        assert_eq!(failure_hint(&silent), "code de sortie 3");
    }

    #[test]
    fn l_avancement_legendary_lit_pourcentage_octets_et_eta() {
        let mut progress = InstallProgress::default();
        apply_progress(
            &mut progress,
            "[DLManager] INFO: = Progress: 42.41% (5.42/12.78 GiB), Running for 00:01:22, ETA: 00:01:44",
        );
        assert_eq!(progress.percent, Some(42.41));
        assert_eq!(
            progress.total,
            Some((12.78_f64 * 1_073_741_824.0).round() as u64)
        );
        assert_eq!(progress.eta.as_deref(), Some("00:01:44"));
        assert!(
            progress.stage.contains("Téléchargement"),
            "{}",
            progress.stage
        );
        // Un pourcentage plus faible (nouvelle passe) ne fait pas reculer la barre.
        apply_progress(
            &mut progress,
            "[DLManager] INFO: = Progress: 3.00% (0.38/12.78 GiB)",
        );
        assert_eq!(progress.percent, Some(42.41));
        apply_progress(
            &mut progress,
            "[DLManager] INFO: = Progress: 99.50% (12.72/12.78 GiB)",
        );
        assert_eq!(progress.percent, Some(99.50));
    }

    #[test]
    fn l_avancement_gogdl_accepte_les_oectets_bruts_et_le_debit() {
        let mut progress = InstallProgress::default();
        apply_progress(
            &mut progress,
            "= Progress: 12.50 4472934400/35791024640, Running for: 00:00:31, ETA: 00:03:12",
        );
        assert_eq!(progress.percent, Some(12.5));
        assert_eq!(progress.written, Some(4_472_934_400));
        assert_eq!(progress.total, Some(35_791_024_640));
        apply_progress(
            &mut progress,
            " + Download\t- 18.42 MiB/s (raw) / 21.00 MiB/s (decompressed)",
        );
        assert_eq!(progress.speed.as_deref(), Some("18.42 Mo/s"));
    }

    #[test]
    fn le_flux_met_a_jour_l_avancement_pendant_la_commande() {
        let progress = Arc::new(Mutex::new(InstallProgress::default()));
        let output = run_streaming(
            cmd(&[
                "-c",
                "echo '= Progress: 10.00% (0.10/1.00 GiB)'; sleep 0.05; echo '= Progress: 55.00% (0.55/1.00 GiB)'; exit 0",
            ]),
            Duration::from_secs(2),
            None,
            progress.clone(),
        )
        .unwrap();
        assert_eq!(output.exit_code, Some(0));
        assert_eq!(progress.lock().unwrap().percent, Some(55.0));
    }

    #[test]
    fn runner_never_starts_a_pre_cancelled_command() {
        let marker =
            std::env::temp_dir().join(format!("quark-runner-cancel-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let command = cmd(&[
            "-c",
            "printf started > \"$1\"",
            "runner",
            marker.to_str().unwrap(),
        ]);
        let cancellation = Arc::new(AtomicBool::new(true));
        let error = run(command, Duration::from_secs(1), Some(cancellation)).unwrap_err();
        assert!(error.contains("annul"));
        assert!(!marker.exists());
    }
}
