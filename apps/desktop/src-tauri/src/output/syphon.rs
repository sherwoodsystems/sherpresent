//! Syphon output: drives the `sherpresent-output` Swift helper, which renders
//! caption frames with alpha and publishes them as a Syphon server.
//!
//! ## Protocol (version 1)
//!
//! - **stdin**: NDJSON, the overlay socket's message set (`settings`,
//!   `status`, `replay`, `segment`). EOF = graceful stop.
//! - **stdout**: NDJSON — `ready`, `sinks` (receiver connect/disconnect),
//!   `error`.
//! - **stderr**: plain-text logs, forwarded to ours.
//!
//! Like `provider/apple.rs`, the process handling compiles everywhere so the
//! tests can drive a scripted fake on any platform; only `is_supported`
//! gates real use.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::ChildStdin;
use tokio::sync::{broadcast, watch};
use tokio::time::timeout;

use super::{OutputState, OutputStatus, StatusReporter};
use crate::captions::{CaptionSinks, OverlaySettings};

/// Helper binary name, as placed next to the app executable.
pub const HELPER_NAME: &str = "sherpresent-output";

/// Overrides helper discovery (development, and tests with a scripted fake).
const HELPER_ENV: &str = "SHERPRESENT_OUTPUT_BIN";

const PROTOCOL_VERSION: u32 = 1;

/// The helper's deployment target. Syphon itself goes back much further; 13 is
/// just the floor for the Swift concurrency the helper uses.
const MIN_MACOS_MAJOR: u32 = 13;

const BACKOFF_START: Duration = Duration::from_millis(250);
const BACKOFF_MAX: Duration = Duration::from_secs(10);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
/// A write that blocks this long means the helper is wedged; respawn.
const STDIN_WRITE_TIMEOUT: Duration = Duration::from_secs(1);
const STDERR_TAIL_LINES: usize = 5;

/// Whether this Mac can run the helper. Apple Silicon only, because that's the
/// only architecture the build script produces.
pub fn is_supported() -> bool {
    cfg!(target_os = "macos")
        && std::env::consts::ARCH == "aarch64"
        && crate::captions::provider::apple::macos_major_version()
            .is_some_and(|v| v >= MIN_MACOS_MAJOR)
}

pub fn preflight() -> Result<PathBuf, String> {
    if let Some(path) = crate::sidecar::env_override(HELPER_ENV)? {
        return Ok(path);
    }
    if !is_supported() {
        return Err(format!(
            "Syphon output needs an Apple Silicon Mac on macOS {} or later.",
            MIN_MACOS_MAJOR
        ));
    }
    crate::sidecar::resolve_bundled(HELPER_NAME)
        .map_err(|e| format!("Syphon output: {} Rebuild with `bun run macos:sidecar`.", e))
}

enum SessionEnd {
    /// Asked to stop, or the caption feed is gone for good.
    Shutdown,
    /// Crashed or wedged; worth a respawn.
    Failed { reason: String, was_ready: bool },
    /// Won't get better by retrying (bad arguments, no Metal device...).
    Fatal(String),
}

/// Run the output until `shutdown` flips, respawning the helper with backoff.
pub async fn run(
    path: PathBuf,
    name: String,
    sinks: CaptionSinks,
    mut shutdown: watch::Receiver<bool>,
    report: StatusReporter,
) {
    let base = OutputStatus {
        supported: true,
        state: OutputState::Starting,
        name: name.clone(),
        has_clients: false,
        message: None,
    };
    let mut backoff = BACKOFF_START;

    loop {
        report(base.clone());
        match run_session(&path, &name, &sinks, &mut shutdown, &report, &base).await {
            SessionEnd::Shutdown => break,
            SessionEnd::Fatal(message) => {
                log::error!("Syphon output stopped: {}", message);
                report(OutputStatus {
                    state: OutputState::Error,
                    message: Some(message),
                    ..base.clone()
                });
                return;
            }
            SessionEnd::Failed { reason, was_ready } => {
                log::warn!("Syphon output helper failed, restarting: {}", reason);
                if was_ready {
                    backoff = BACKOFF_START;
                }
                report(OutputStatus {
                    state: OutputState::Error,
                    message: Some(format!("Restarting: {}", reason)),
                    ..base.clone()
                });
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = shutdown.changed() => {}
                }
                if *shutdown.borrow() {
                    break;
                }
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
        }
    }

    report(OutputStatus {
        state: OutputState::Stopped,
        ..base
    });
}

async fn run_session(
    path: &Path,
    name: &str,
    sinks: &CaptionSinks,
    shutdown: &mut watch::Receiver<bool>,
    report: &StatusReporter,
    base: &OutputStatus,
) -> SessionEnd {
    if *shutdown.borrow() {
        return SessionEnd::Shutdown;
    }

    // Subscribe before snapshotting, so nothing published in between is lost.
    let mut updates = sinks.broadcast.subscribe();
    let mut overlay = sinks.overlay.subscribe();

    let args = build_args(name);
    log::info!("Starting caption output helper: {} {}", path.display(), args.join(" "));
    let mut child = match tokio::process::Command::new(path)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return SessionEnd::Fatal(format!(
                "Could not start the caption output helper at {}: {}",
                path.display(),
                e
            ))
        }
    };

    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        return SessionEnd::Failed {
            reason: "helper has no stdio pipes".to_string(),
            was_ready: false,
        };
    };
    let tail = Arc::new(Mutex::new(VecDeque::new()));
    if let Some(stderr) = child.stderr.take() {
        spawn_stderr_pump(stderr, Arc::clone(&tail));
    }

    // Initial state: styling, engine status, and whatever is on screen now.
    // Snapshot first: a watch borrow is a lock guard and must not live
    // across the awaits below.
    let settings = overlay.borrow_and_update().clone();
    for line in initial_lines(sinks, &settings) {
        if let Err(reason) = write_line(&mut stdin, &line).await {
            return SessionEnd::Failed { reason, was_ready: false };
        }
    }

    let mut lines = BufReader::new(stdout).lines();
    let mut was_ready = false;
    let mut current = base.clone();

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    drop(stdin);
                    if timeout(SHUTDOWN_GRACE, child.wait()).await.is_err() {
                        let _ = child.kill().await;
                    }
                    return SessionEnd::Shutdown;
                }
            }
            update = updates.recv() => {
                let line = match update {
                    Ok(u) => serde_json::to_string(&u).ok(),
                    // Behind: only the newest lines matter, so resync from the
                    // replay buffer rather than replaying the backlog.
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        log::warn!("Caption output lagged, skipped {} updates", n);
                        Some(replay_line(sinks))
                    }
                    Err(broadcast::error::RecvError::Closed) => return SessionEnd::Shutdown,
                };
                if let Some(line) = line {
                    if let Err(reason) = write_line(&mut stdin, &line).await {
                        return SessionEnd::Failed { reason, was_ready };
                    }
                }
            }
            Ok(()) = overlay.changed() => {
                let line = settings_line(&overlay.borrow_and_update());
                if let Err(reason) = write_line(&mut stdin, &line).await {
                    return SessionEnd::Failed { reason, was_ready };
                }
            }
            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => match parse_line(&line) {
                        HelperLine::Ready { has_clients } | HelperLine::Sinks { has_clients } => {
                            was_ready = true;
                            current = OutputStatus {
                                state: OutputState::Running,
                                has_clients,
                                message: None,
                                ..current
                            };
                            report(current.clone());
                        }
                        HelperLine::Error { fatal: true, message } => {
                            return SessionEnd::Fatal(with_tail(message, &tail));
                        }
                        HelperLine::Error { fatal: false, message } => {
                            log::warn!("Caption output helper: {}", message);
                        }
                        HelperLine::Ignore => {}
                    },
                    Ok(None) | Err(_) => {
                        let status = child.wait().await.ok();
                        return SessionEnd::Failed {
                            reason: with_tail(
                                format!("helper exited ({})", status.map_or("unknown".into(), |s| s.to_string())),
                                &tail,
                            ),
                            was_ready,
                        };
                    }
                }
            }
        }
    }
}

async fn write_line(stdin: &mut ChildStdin, line: &str) -> Result<(), String> {
    let mut bytes = Vec::with_capacity(line.len() + 1);
    bytes.extend_from_slice(line.as_bytes());
    bytes.push(b'\n');
    match timeout(STDIN_WRITE_TIMEOUT, stdin.write_all(&bytes)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(format!("helper closed stdin: {}", e)),
        Err(_) => Err("helper stopped reading (write timed out)".to_string()),
    }
}

fn spawn_stderr_pump(stderr: tokio::process::ChildStderr, tail: Arc<Mutex<VecDeque<String>>>) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            log::info!("[output-sidecar] {}", line);
            let mut buf = tail.lock().unwrap();
            buf.push_back(line);
            while buf.len() > STDERR_TAIL_LINES {
                buf.pop_front();
            }
        }
    });
}

fn with_tail(message: String, tail: &Arc<Mutex<VecDeque<String>>>) -> String {
    let buf = tail.lock().unwrap();
    if buf.is_empty() {
        message
    } else {
        format!("{} — {}", message, buf.iter().cloned().collect::<Vec<_>>().join(" | "))
    }
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

fn build_args(name: &str) -> Vec<String> {
    vec![
        "--protocol".into(),
        PROTOCOL_VERSION.to_string(),
        "--sink".into(),
        "syphon".into(),
        "--name".into(),
        name.to_string(),
    ]
}

fn settings_line(s: &OverlaySettings) -> String {
    serde_json::json!({ "type": "settings", "settings": s }).to_string()
}

fn replay_line(sinks: &CaptionSinks) -> String {
    let segments: Vec<_> = sinks.buffer.lock().unwrap().iter().cloned().collect();
    serde_json::json!({ "type": "replay", "segments": segments }).to_string()
}

fn initial_lines(sinks: &CaptionSinks, settings: &OverlaySettings) -> Vec<String> {
    let status = sinks.status.lock().unwrap().clone();
    vec![
        settings_line(settings),
        serde_json::json!({ "type": "status", "status": status }).to_string(),
        replay_line(sinks),
    ]
}

#[derive(Debug, PartialEq)]
enum HelperLine {
    Ready { has_clients: bool },
    Sinks { has_clients: bool },
    Error { fatal: bool, message: String },
    Ignore,
}

/// Unknown types and malformed lines are ignored: a stray write or a newer
/// helper must not take the output down mid-show.
fn parse_line(line: &str) -> HelperLine {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return HelperLine::Ignore;
    };
    let any_clients = |v: &Value| {
        v.get("sinks")
            .and_then(Value::as_array)
            .is_some_and(|s| s.iter().any(|s| s.get("hasClients") == Some(&Value::Bool(true))))
    };
    match v.get("type").and_then(Value::as_str) {
        Some("ready") => HelperLine::Ready { has_clients: any_clients(&v) },
        Some("sinks") => HelperLine::Sinks { has_clients: any_clients(&v) },
        Some("error") => HelperLine::Error {
            fatal: v.get("fatal").and_then(Value::as_bool).unwrap_or(false),
            message: v
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error")
                .to_string(),
        },
        _ => HelperLine::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::captions::{CaptionSegment, CaptionStatus, CaptionUpdate};
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn test_parse_line() {
        assert_eq!(
            parse_line(r#"{"type":"ready","sinks":[{"kind":"syphon","hasClients":false}]}"#),
            HelperLine::Ready { has_clients: false }
        );
        assert_eq!(
            parse_line(r#"{"type":"sinks","sinks":[{"kind":"syphon","hasClients":true}]}"#),
            HelperLine::Sinks { has_clients: true }
        );
        assert_eq!(
            parse_line(r#"{"type":"error","fatal":true,"message":"no Metal"}"#),
            HelperLine::Error { fatal: true, message: "no Metal".into() }
        );
        assert_eq!(parse_line("garbage"), HelperLine::Ignore);
        assert_eq!(parse_line(r#"{"type":"future"}"#), HelperLine::Ignore);
    }

    #[test]
    fn test_build_args() {
        assert_eq!(
            build_args("Stage Left"),
            ["--protocol", "1", "--sink", "syphon", "--name", "Stage Left"]
        );
    }

    fn sinks() -> CaptionSinks {
        CaptionSinks {
            broadcast: broadcast::channel(16).0,
            buffer: Arc::new(Mutex::new(VecDeque::new())),
            status: Arc::new(Mutex::new(CaptionStatus::default())),
            overlay: watch::channel(OverlaySettings::default()).0,
        }
    }

    /// A fake helper: announces ready, then copies stdin to `log`.
    fn fake_helper(log: &Path) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sherpresent-output-test-{}-{}",
            std::process::id(),
            log.file_name().unwrap().to_string_lossy()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("fake-output");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\necho '{{\"type\":\"ready\",\"sinks\":[{{\"kind\":\"syphon\",\"hasClients\":true}}]}}'\ncat > '{}'\n",
                log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    async fn wait_for(log: &Path, needle: &str) -> String {
        for _ in 0..100 {
            let text = std::fs::read_to_string(log).unwrap_or_default();
            if text.contains(needle) {
                return text;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("{} never appeared in {}", needle, std::fs::read_to_string(log).unwrap_or_default());
    }

    #[tokio::test]
    async fn test_forwards_state_segments_and_settings() {
        let log = std::env::temp_dir().join(format!("sp-output-forward-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&log);
        let script = fake_helper(&log);

        let sinks = sinks();
        sinks.buffer.lock().unwrap().push_back(CaptionSegment {
            id: 1,
            source: "earlier".into(),
            translated: String::new(),
            is_final: true,
            timestamp: 0,
        });

        let statuses: Arc<Mutex<Vec<OutputStatus>>> = Arc::default();
        let seen = Arc::clone(&statuses);
        let report: StatusReporter = Arc::new(move |s| seen.lock().unwrap().push(s));
        let (stop_tx, stop_rx) = watch::channel(false);
        let task = tokio::spawn(run(script, "Test".into(), sinks.clone(), stop_rx, report));

        // Initial state arrives first, including the replay buffer.
        let text = wait_for(&log, "\"earlier\"").await;
        assert!(text.contains(r#""type":"settings""#), "{}", text);
        assert!(text.contains(r#""type":"status""#), "{}", text);

        // Wait for ready before publishing, so the subscription is live.
        for _ in 0..100 {
            if statuses.lock().unwrap().iter().any(|s| s.state == OutputState::Running) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let running = statuses.lock().unwrap().last().cloned().unwrap();
        assert_eq!(running.state, OutputState::Running);
        assert!(running.has_clients);

        let _ = sinks.broadcast.send(CaptionUpdate::Segment {
            segment: CaptionSegment {
                id: 2,
                source: "live line".into(),
                translated: String::new(),
                is_final: false,
                timestamp: 0,
            },
        });
        wait_for(&log, "\"live line\"").await;

        sinks.overlay.send_modify(|s| s.font_size = 99);
        wait_for(&log, "\"fontSize\":99").await;

        stop_tx.send(true).unwrap();
        timeout(Duration::from_secs(5), task).await.unwrap().unwrap();
        assert_eq!(statuses.lock().unwrap().last().unwrap().state, OutputState::Stopped);
    }

    #[tokio::test]
    async fn test_fatal_error_stops_without_respawn() {
        let dir = std::env::temp_dir().join(format!("sp-output-fatal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("fake-output");
        std::fs::write(
            &script,
            "#!/bin/sh\necho '{\"type\":\"error\",\"fatal\":true,\"message\":\"No Metal device\"}'\nexit 3\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let statuses: Arc<Mutex<Vec<OutputStatus>>> = Arc::default();
        let seen = Arc::clone(&statuses);
        let report: StatusReporter = Arc::new(move |s| seen.lock().unwrap().push(s));
        let (_stop_tx, stop_rx) = watch::channel(false);
        timeout(Duration::from_secs(5), run(script, "Test".into(), sinks(), stop_rx, report))
            .await
            .expect("fatal error must end run() without retrying");

        let last = statuses.lock().unwrap().last().cloned().unwrap();
        assert_eq!(last.state, OutputState::Error);
        assert!(last.message.unwrap().contains("No Metal device"));
    }
}
