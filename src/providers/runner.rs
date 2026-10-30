//! Isolated subprocess contract for managed provider tools.

use std::ffi::OsString;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
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

pub fn run(
    command: ProviderCommand,
    timeout: Duration,
    cancellation: Option<Arc<AtomicBool>>,
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
    thread::spawn(move || {
        let mut stdout = stdout;
        let _ = stdout_tx.send(read_capped(&mut stdout));
    });
    thread::spawn(move || {
        let mut stderr = stderr;
        let _ = stderr_tx.send(read_capped(&mut stderr));
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

fn read_capped(reader: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(MAX_STREAM_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_STREAM_BYTES {
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
