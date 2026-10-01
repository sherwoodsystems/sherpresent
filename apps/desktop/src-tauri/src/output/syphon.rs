//! Syphon output: drives the `sherpresent-output` Swift helper, which renders
//! frames (captions with alpha, or notes) and publishes them as a Syphon
//! server. One helper process per output.
//!
//! ## Protocol (version 2)
//!
//! - **args**: `--content captions|notes` picks what the helper draws.
//! - **stdin**: NDJSON from the output's [`Feed`]. EOF = graceful stop.
//! - **stdout**: NDJSON — `ready`, `sinks` (receiver connect/disconnect),
//!   `error`.
//! - **stderr**: plain-text logs, forwarded to ours.
//!
//! Like `provider/apple.rs`, the process handling compiles everywhere so the
//! tests can drive a scripted fake on any platform; only `is_supported`
//! gates real use.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::watch;
use tokio::time::timeout;
use tokio_stream::StreamExt;

use super::feed::Feed;
use super::{OutputState, OutputStatus, StatusReporter};
use crate::config::CaptionText;
use crate::sidecar::{snapshot_tail, spawn_stderr_pump, with_stderr_tail, StderrTail};

/// Helper binary name, as placed next to the app executable.
pub const HELPER_NAME: &str = "sherpresent-output";

/// Overrides helper discovery (development, and tests with a scripted fake).
const HELPER_ENV: &str = "SHERPRESENT_OUTPUT_BIN";

const PROTOCOL_VERSION: u32 = 3;

/// The helper's deployment target. Syphon itself goes back much further; 13 is
/// just the floor for the Swift concurrency the helper uses.
const MIN_MACOS_MAJOR: u32 = 13;

const BACKOFF_START: Duration = Duration::from_millis(250);
const BACKOFF_MAX: Duration = Duration::from_secs(10);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
/// A write that blocks this long means the helper is wedged; respawn.
const STDIN_WRITE_TIMEOUT: Duration = Duration::from_secs(1);

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
    /// Asked to stop, or the feed is gone for good.
    Shutdown,
    /// Crashed or wedged; worth a respawn.
    Failed { reason: String, was_ready: bool },
    /// Won't get better by retrying (bad arguments, no Metal device...).
    Fatal(String),
}

/// Run the output until `shutdown` flips, respawning the helper with backoff.
/// `content` is the helper's `--content`, `text` its `--text` (captions
/// only); `name` the Syphon server name.
pub async fn run(
    path: PathBuf,
    content: &'static str,
    text: Option<CaptionText>,
    name: String,
    feed: Feed,
    mut shutdown: watch::Receiver<bool>,
    report: StatusReporter,
) {
    let base = OutputStatus {
        state: OutputState::Starting,
        ..OutputStatus::stopped(true, &name)
    };
    let mut backoff = BACKOFF_START;

    loop {
        report(base.clone());
        let args = build_args(content, &name, text);
        match run_session(&path, &args, &feed, &mut shutdown, &report, &base).await {
            SessionEnd::Shutdown => break,
            SessionEnd::Fatal(message) => {
                log::error!("Syphon {} output stopped: {}", content, message);
                report(OutputStatus {
                    state: OutputState::Error,
                    message: Some(message),
                    ..base.clone()
                });
                return;
            }
            SessionEnd::Failed { reason, was_ready } => {
                log::warn!(
                    "Syphon {} output helper failed, restarting: {}",
                    content,
                    reason
                );
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
    args: &[String],
    feed: &Feed,
    shutdown: &mut watch::Receiver<bool>,
    report: &StatusReporter,
    base: &OutputStatus,
) -> SessionEnd {
    if *shutdown.borrow() {
        return SessionEnd::Shutdown;
    }

    // Opened before the spawn, so the opening state is snapshotted and live
    // updates queue from here on; nothing is lost while the helper starts.
    let mut feed = feed();

    log::info!(
        "Starting output helper: {} {}",
        path.display(),
        args.join(" ")
    );
    let mut child = match tokio::process::Command::new(path)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return SessionEnd::Fatal(format!(
                "Could not start the output helper at {}: {}",
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
    let tail = StderrTail::default();
    if let Some(stderr) = child.stderr.take() {
        spawn_stderr_pump(stderr, Arc::clone(&tail), "output-sidecar");
    }

    let mut lines = BufReader::new(stdout).lines();
    let mut was_ready = false;
    let mut current = base.clone();

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    stop(child, stdin).await;
                    return SessionEnd::Shutdown;
                }
            }
            line = feed.next() => {
                let Some(line) = line else {
                    stop(child, stdin).await;
                    return SessionEnd::Shutdown;
                };
                if let Err(reason) = write_line(&mut stdin, &line).await {
                    return SessionEnd::Failed { reason, was_ready };
                }
            }
            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => match parse_line(&line) {
                        HelperLine::Sinks { has_clients } => {
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
                            return SessionEnd::Fatal(with_stderr_tail(message, &snapshot_tail(&tail)));
                        }
                        HelperLine::Error { fatal: false, message } => {
                            log::warn!("Output helper: {}", message);
                        }
                        HelperLine::Ignore => {}
                    },
                    Ok(None) | Err(_) => {
                        let status = child.wait().await.ok();
                        return SessionEnd::Failed {
                            reason: with_stderr_tail(
                                format!("helper exited ({})", status.map_or("unknown".into(), |s| s.to_string())),
                                &snapshot_tail(&tail),
                            ),
                            was_ready,
                        };
                    }
                }
            }
        }
    }
}

/// Close stdin so the helper stops its Syphon server cleanly, then give it a
/// moment before killing it.
async fn stop(mut child: Child, stdin: ChildStdin) {
    drop(stdin);
    if timeout(SHUTDOWN_GRACE, child.wait()).await.is_err() {
        let _ = child.kill().await;
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

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

fn build_args(content: &str, name: &str, text: Option<CaptionText>) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--protocol".into(),
        PROTOCOL_VERSION.to_string(),
        "--content".into(),
        content.into(),
        "--sink".into(),
        "syphon".into(),
        "--name".into(),
        name.to_string(),
    ];
    if let Some(text) = text {
        args.extend(["--text".into(), text.as_str().into()]);
    }
    args
}

#[derive(Debug, PartialEq)]
enum HelperLine {
    /// `ready` or `sinks`: both report receiver state, which is all we use.
    Sinks {
        has_clients: bool,
    },
    Error {
        fatal: bool,
        message: String,
    },
    Ignore,
}

/// Unknown types and malformed lines are ignored: a stray write or a newer
/// helper must not take the output down mid-show.
fn parse_line(line: &str) -> HelperLine {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return HelperLine::Ignore;
    };
    let any_clients = |v: &Value| {
        v.get("sinks").and_then(Value::as_array).is_some_and(|s| {
            s.iter()
                .any(|s| s.get("hasClients") == Some(&Value::Bool(true)))
        })
    };
    match v.get("type").and_then(Value::as_str) {
        Some("ready" | "sinks") => HelperLine::Sinks {
            has_clients: any_clients(&v),
        },
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
    use crate::captions::{CaptionSegment, CaptionSinks, CaptionUpdate};
    use crate::output::feed;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Mutex;

    #[test]
    fn test_parse_line() {
        assert_eq!(
            parse_line(r#"{"type":"ready","sinks":[{"kind":"syphon","hasClients":false}]}"#),
            HelperLine::Sinks { has_clients: false }
        );
        assert_eq!(
            parse_line(r#"{"type":"sinks","sinks":[{"kind":"syphon","hasClients":true}]}"#),
            HelperLine::Sinks { has_clients: true }
        );
        assert_eq!(
            parse_line(r#"{"type":"error","fatal":true,"message":"no Metal"}"#),
            HelperLine::Error {
                fatal: true,
                message: "no Metal".into()
            }
        );
        assert_eq!(parse_line("garbage"), HelperLine::Ignore);
        assert_eq!(parse_line(r#"{"type":"future"}"#), HelperLine::Ignore);
    }

    #[test]
    fn test_build_args() {
        assert_eq!(
            build_args("captions", "Stage Left", Some(CaptionText::Both)),
            [
                "--protocol",
                "3",
                "--content",
                "captions",
                "--sink",
                "syphon",
                "--name",
                "Stage Left",
                "--text",
                "both"
            ]
        );
    }

    /// An executable shell script standing in for the helper.
    fn fake_script(tag: &str, body: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sp-output-{}-{}", tag, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("fake-output");
        std::fs::write(&script, format!("#!/bin/sh\n{}", body)).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    /// A fake helper: announces ready, then copies stdin to `log`.
    fn fake_helper(log: &Path) -> PathBuf {
        fake_script(
            &log.file_name().unwrap().to_string_lossy(),
            &format!(
                "echo '{{\"type\":\"ready\",\"sinks\":[{{\"kind\":\"syphon\",\"hasClients\":true}}]}}'\ncat > '{}'\n",
                log.display()
            ),
        )
    }

    /// A reporter that records every status it's given.
    fn recorder() -> (Arc<Mutex<Vec<OutputStatus>>>, StatusReporter) {
        let statuses: Arc<Mutex<Vec<OutputStatus>>> = Arc::default();
        let seen = Arc::clone(&statuses);
        (statuses, Arc::new(move |s| seen.lock().unwrap().push(s)))
    }

    async fn wait_for(log: &Path, needle: &str) -> String {
        for _ in 0..100 {
            let text = std::fs::read_to_string(log).unwrap_or_default();
            if text.contains(needle) {
                return text;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!(
            "{} never appeared in {}",
            needle,
            std::fs::read_to_string(log).unwrap_or_default()
        );
    }

    #[tokio::test]
    async fn test_forwards_state_segments_and_settings() {
        let log =
            std::env::temp_dir().join(format!("sp-output-forward-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&log);
        let script = fake_helper(&log);

        let sinks = CaptionSinks::default();
        sinks.buffer.lock().unwrap().push_back(CaptionSegment {
            id: 1,
            source: "earlier".into(),
            translated: String::new(),
            is_final: true,
            timestamp: 0,
        });

        let (statuses, report) = recorder();
        let (stop_tx, stop_rx) = watch::channel(false);
        let task = tokio::spawn(run(
            script,
            "captions",
            None,
            "Test".into(),
            feed::captions(sinks.clone()),
            stop_rx,
            report,
        ));

        // Initial state arrives first, including the replay buffer.
        let text = wait_for(&log, "\"earlier\"").await;
        assert!(text.contains(r#""type":"settings""#), "{}", text);
        assert!(text.contains(r#""type":"status""#), "{}", text);

        // Wait for ready before publishing, so the subscription is live.
        for _ in 0..100 {
            if statuses
                .lock()
                .unwrap()
                .iter()
                .any(|s| s.state == OutputState::Running)
            {
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
        timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            statuses.lock().unwrap().last().unwrap().state,
            OutputState::Stopped
        );
    }

    #[tokio::test]
    async fn test_fatal_error_stops_without_respawn() {
        let script = fake_script(
            "fatal",
            "echo '{\"type\":\"error\",\"fatal\":true,\"message\":\"No Metal device\"}'\nexit 3\n",
        );
        let (statuses, report) = recorder();
        let (_stop_tx, stop_rx) = watch::channel(false);
        timeout(
            Duration::from_secs(5),
            run(
                script,
                "captions",
                None,
                "Test".into(),
                feed::captions(CaptionSinks::default()),
                stop_rx,
                report,
            ),
        )
        .await
        .expect("fatal error must end run() without retrying");

        let last = statuses.lock().unwrap().last().cloned().unwrap();
        assert_eq!(last.state, OutputState::Error);
        assert!(last.message.unwrap().contains("No Metal device"));
    }
}
