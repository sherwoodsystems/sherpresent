//! Apple on-device caption provider (macOS 26+).
//!
//! Speech recognition and translation both happen locally: no API key, no
//! network, no per-minute cost. Both frameworks are Swift-only with no C ABI,
//! so this drives a helper process (`sherpresent-speech`) rather than calling
//! them in-process the way `applescript.rs` does.
//!
//! ## Wire protocol
//!
//! - **stdin**: raw little-endian i16 PCM at 16 kHz mono, unframed. The sidecar
//!   never opens the microphone itself — cpal capture stays in this process, so
//!   there is one device claim, one TCC prompt, and the existing device picker
//!   keeps working.
//! - **stdout**: newline-delimited JSON, one message per line.
//! - **stderr**: plain-text logs, forwarded to `log::` and tailed into failure
//!   messages.
//!
//! Closing stdin is the graceful stop: the sidecar finalizes the utterance in
//! flight, flushes it, and exits 0.
//!
//! ## Replace, not Delta
//!
//! `SpeechTranscriber` emits *revisable* hypotheses — "hello word" becomes
//! "hello world" — so this provider reports [`ProviderEvent::Replace`] with the
//! full current line. The sidecar owns the turn text; nothing is accumulated on
//! this side.
//!
//! The module compiles everywhere even though only macOS can select it: the
//! protocol and process handling are platform-independent, and keeping them in
//! the build is what lets the tests below run on a Linux dev machine that
//! cannot compile Swift. Only `build()`'s match arm is gated.

// Off macOS the entry points below are unreachable by design — `build()` returns
// an error instead of constructing the provider — but the tests still exercise
// `run_loop` and the parsers.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::watch;
use tokio::time::timeout;

use super::{drain_backlog, CaptionProvider, ProviderConfig, ProviderEvent};
use crate::captions::audio::{samples_to_le_bytes, TARGET_SAMPLE_RATE};

/// Name of the helper binary, as placed next to the app executable.
const SIDECAR_NAME: &str = "sherpresent-speech";

/// Overrides sidecar discovery. Set by `bun run macos:dev` when the bundled
/// path does not apply, and by the tests to point at a scripted fake.
const SIDECAR_ENV: &str = "SHERPRESENT_SPEECH_BIN";

/// Wire protocol version. Bumped whenever a message shape changes
/// incompatibly; the sidecar rejects a mismatch rather than guessing.
const PROTOCOL_VERSION: u32 = 1;

/// Minimum macOS major version. `SpeechAnalyzer` and the non-SwiftUI
/// `TranslationSession` initializer both landed in 26 (Tahoe).
const MIN_MACOS_MAJOR: u32 = 26;

/// Respawn backoff bounds.
const BACKOFF_START: Duration = Duration::from_millis(250);
const BACKOFF_MAX: Duration = Duration::from_secs(10);

/// How long to wait for the sidecar to exit after stdin closes.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

/// How long a single audio write may block before the sidecar is presumed
/// wedged.
///
/// Without this a hung helper leaves `write_all` pending forever while the
/// unbounded audio channel grows for the rest of the show. Respawning is
/// strictly better than leaking.
const STDIN_WRITE_TIMEOUT: Duration = Duration::from_secs(1);

/// Lines of sidecar stderr kept for diagnostics.
const STDERR_TAIL_LINES: usize = 20;

/// Lines of stderr appended to a failure message.
const STDERR_TAIL_REPORTED: usize = 5;

pub struct AppleProvider {
    config: ProviderConfig,
}

impl AppleProvider {
    pub fn new(config: ProviderConfig) -> Self {
        Self { config }
    }
}

impl CaptionProvider for AppleProvider {
    fn name(&self) -> &'static str {
        "apple"
    }

    fn run(
        self: Box<Self>,
        audio_rx: UnboundedReceiver<Vec<i16>>,
        events: UnboundedSender<ProviderEvent>,
        shutdown: watch::Receiver<bool>,
    ) -> super::BoxFuture {
        Box::pin(run_loop(self.config, audio_rx, events, shutdown))
    }
}

/// What ended a sidecar session.
///
/// There is no `Reconnect` variant: unlike Gemini's scheduled 10-minute
/// handoff, every unplanned exit here is a failure to back off from.
#[derive(Debug)]
enum SessionOutcome {
    Shutdown,
    Failed { reason: String, was_ready: bool },
    Fatal { message: String },
}

async fn run_loop(
    config: ProviderConfig,
    mut audio_rx: UnboundedReceiver<Vec<i16>>,
    events: UnboundedSender<ProviderEvent>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut backoff = BACKOFF_START;

    loop {
        if *shutdown.borrow() {
            return;
        }

        match run_session(&config, &mut audio_rx, &events, &mut shutdown).await {
            SessionOutcome::Shutdown => return,

            SessionOutcome::Fatal { message } => {
                let _ = events.send(ProviderEvent::Fatal { message });
                return;
            }

            SessionOutcome::Failed { reason, was_ready } => {
                // Commit whatever the dead session produced. With Replace
                // semantics the next session's first event would otherwise
                // overwrite a half-finished line mid-word. `consume_events`
                // drops empty turns, so this is free when nothing is pending.
                let _ = events.send(ProviderEvent::TurnComplete);
                let _ = events.send(ProviderEvent::Reconnecting {
                    reason: reason.clone(),
                });

                // A session that ran normally before dying gets a fresh budget;
                // only repeated fast failures should back off.
                if was_ready {
                    backoff = BACKOFF_START;
                }

                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = shutdown.changed() => return,
                }
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
        }
    }
}

async fn run_session(
    config: &ProviderConfig,
    audio_rx: &mut UnboundedReceiver<Vec<i16>>,
    events: &UnboundedSender<ProviderEvent>,
    shutdown: &mut watch::Receiver<bool>,
) -> SessionOutcome {
    let path = match preflight() {
        Ok(p) => p,
        Err(message) => return SessionOutcome::Fatal { message },
    };

    let args = build_args(config);
    log::info!("Starting speech sidecar: {} {}", path.display(), args.join(" "));

    let mut child = match tokio::process::Command::new(&path)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return SessionOutcome::Fatal {
                message: format!(
                    "Could not start the Apple speech helper at {}: {}",
                    path.display(),
                    e
                ),
            }
        }
    };

    let mut stdin = match child.stdin.take() {
        Some(s) => s,
        None => {
            return SessionOutcome::Failed {
                reason: "speech helper has no stdin pipe".to_string(),
                was_ready: false,
            }
        }
    };
    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            return SessionOutcome::Failed {
                reason: "speech helper has no stdout pipe".to_string(),
                was_ready: false,
            }
        }
    };

    let tail: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
    if let Some(stderr) = child.stderr.take() {
        spawn_stderr_pump(stderr, Arc::clone(&tail));
    }

    let mut lines = BufReader::new(stdout).lines();
    let mut was_ready = false;

    // Anything queued while the previous session was down is stale beyond the
    // last second; replaying it would push the live feed permanently behind.
    let (carryover, dropped) = drain_backlog(audio_rx);
    if dropped > 0 {
        log::debug!("Dropped {} stale audio chunks before sidecar start", dropped);
    }
    for chunk in carryover {
        if stdin.write_all(&samples_to_le_bytes(&chunk)).await.is_err() {
            return SessionOutcome::Failed {
                reason: "speech helper closed stdin during startup".to_string(),
                was_ready,
            };
        }
    }

    loop {
        tokio::select! {
            biased;

            _ = shutdown.changed() => {
                // EOF is the graceful stop: the sidecar flushes its last line.
                drop(stdin);
                if timeout(SHUTDOWN_GRACE, child.wait()).await.is_err() {
                    let _ = child.start_kill();
                }
                return SessionOutcome::Shutdown;
            }

            line = lines.next_line() => {
                match line {
                    Ok(Some(text)) => {
                        match parse_line(&text) {
                            LineOutcome::Ready => {
                                was_ready = true;
                                let _ = events.send(ProviderEvent::Connected);
                            }
                            LineOutcome::Events(list) => {
                                for event in list {
                                    let _ = events.send(event);
                                }
                            }
                            LineOutcome::Progress { stage, fraction } => {
                                log::info!(
                                    "Speech sidecar downloading {}: {:.0}%",
                                    stage,
                                    fraction * 100.0
                                );
                            }
                            LineOutcome::Fatal(message) => {
                                return SessionOutcome::Fatal {
                                    message: with_stderr_tail(message, &snapshot_tail(&tail)),
                                };
                            }
                            LineOutcome::Failed(reason) => {
                                return SessionOutcome::Failed {
                                    reason: with_stderr_tail(reason, &snapshot_tail(&tail)),
                                    was_ready,
                                };
                            }
                            LineOutcome::Ignore => {
                                log::debug!("Ignoring speech sidecar line: {}", text);
                            }
                        }
                    }
                    Ok(None) => {
                        // stdout closed: the sidecar exited.
                        let status = child.wait().await.ok();
                        return classify_exit(status, was_ready, &snapshot_tail(&tail));
                    }
                    Err(e) => {
                        return SessionOutcome::Failed {
                            reason: format!("speech helper stdout read error: {}", e),
                            was_ready,
                        };
                    }
                }
            }

            chunk = audio_rx.recv() => {
                let Some(chunk) = chunk else {
                    // Capture stopped. Same clean stop as an explicit shutdown.
                    drop(stdin);
                    let _ = timeout(SHUTDOWN_GRACE, child.wait()).await;
                    return SessionOutcome::Shutdown;
                };

                let bytes = samples_to_le_bytes(&chunk);
                match timeout(STDIN_WRITE_TIMEOUT, stdin.write_all(&bytes)).await {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        return SessionOutcome::Failed {
                            reason: format!("speech helper closed stdin: {}", e),
                            was_ready,
                        };
                    }
                    Err(_) => {
                        return SessionOutcome::Failed {
                            reason: format!(
                                "speech helper stopped reading audio (stalled {:?})",
                                STDIN_WRITE_TIMEOUT
                            ),
                            was_ready,
                        };
                    }
                }
            }
        }
    }
}

/// Forward sidecar stderr to the log and keep a rolling tail for diagnostics.
fn spawn_stderr_pump(stderr: tokio::process::ChildStderr, tail: Arc<Mutex<VecDeque<String>>>) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            if line.contains("[error]") {
                log::error!("[speech-sidecar] {}", line);
            } else if line.contains("[warn]") {
                log::warn!("[speech-sidecar] {}", line);
            } else {
                log::info!("[speech-sidecar] {}", line);
            }

            if let Ok(mut buf) = tail.lock() {
                buf.push_back(line);
                while buf.len() > STDERR_TAIL_LINES {
                    buf.pop_front();
                }
            }
        }
    });
}

fn snapshot_tail(tail: &Arc<Mutex<VecDeque<String>>>) -> Vec<String> {
    tail.lock()
        .map(|buf| buf.iter().cloned().collect())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Pure helpers (unit-tested on every platform)
// ---------------------------------------------------------------------------

/// What a single stdout line means to the session loop.
#[derive(Debug, PartialEq)]
enum LineOutcome {
    Ready,
    Events(Vec<ProviderEvent>),
    Progress { stage: String, fraction: f64 },
    Fatal(String),
    Failed(String),
    Ignore,
}

/// Parse one NDJSON line from the sidecar.
///
/// Unknown message types and malformed lines are ignored rather than fatal: a
/// version skew or a stray write must not end a live caption feed.
fn parse_line(line: &str) -> LineOutcome {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return LineOutcome::Ignore;
    }

    let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
        return LineOutcome::Ignore;
    };

    let Some(kind) = value.get("type").and_then(Value::as_str) else {
        return LineOutcome::Ignore;
    };

    match kind {
        "ready" => LineOutcome::Ready,

        "partial" | "final" => {
            let source = value
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let translated = value
                .get("translated")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            LineOutcome::Events(vec![ProviderEvent::Replace { source, translated }])
        }

        "turnComplete" => LineOutcome::Events(vec![ProviderEvent::TurnComplete]),

        "assetProgress" => LineOutcome::Progress {
            stage: value
                .get("stage")
                .and_then(Value::as_str)
                .unwrap_or("model")
                .to_string(),
            fraction: value
                .get("fraction")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                .clamp(0.0, 1.0),
        },

        "error" => {
            let message = value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("the Apple speech helper reported an error")
                .to_string();
            // Absent `fatal` is treated as fatal: retrying an error we do not
            // recognize risks a respawn loop.
            if value.get("fatal").and_then(Value::as_bool).unwrap_or(true) {
                LineOutcome::Fatal(message)
            } else {
                LineOutcome::Failed(message)
            }
        }

        _ => LineOutcome::Ignore,
    }
}

/// Decide what a sidecar exit means.
///
/// An exit before `ready` is always fatal, regardless of code: it means the
/// session never got off the ground (missing language pack, unsupported
/// locale, bad arguments), and respawning would loop forever.
fn classify_exit(status: Option<ExitStatus>, was_ready: bool, tail: &[String]) -> SessionOutcome {
    let code = status
        .and_then(|s| s.code())
        .map(|c| c.to_string())
        .unwrap_or_else(|| "signal".to_string());

    if !was_ready {
        return SessionOutcome::Fatal {
            message: with_stderr_tail(
                format!(
                    "The Apple speech helper exited ({}) before it was ready. \
                     Check that macOS 26 or later is installed and the translation \
                     language pack is present, or select Gemini in Settings.",
                    code
                ),
                tail,
            ),
        };
    }

    if status.map(|s| s.success()).unwrap_or(false) {
        return SessionOutcome::Failed {
            reason: "speech helper exited unexpectedly".to_string(),
            was_ready,
        };
    }

    SessionOutcome::Failed {
        reason: with_stderr_tail(format!("speech helper exited ({})", code), tail),
        was_ready,
    }
}

/// Append the last few stderr lines so a failure is diagnosable from a log.
fn with_stderr_tail(message: String, tail: &[String]) -> String {
    let start = tail.len().saturating_sub(STDERR_TAIL_REPORTED);
    let recent = &tail[start..];
    if recent.is_empty() {
        return message;
    }
    format!("{} — {}", message, recent.join(" | "))
}

/// Normalize an operator-typed BCP-47 tag.
///
/// The Settings field is free text, so "EN", " fr ", and "en-us" all arrive.
/// Apple's `Locale.Language` wants the conventional casing.
fn normalize_lang(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut parts = trimmed.split(['-', '_']).filter(|p| !p.is_empty());
    let Some(primary) = parts.next() else {
        return String::new();
    };

    let mut out = primary.to_lowercase();
    for part in parts {
        out.push('-');
        // A two-letter subtag is a region ("US"); a four-letter one is a script
        // ("Hans"). Anything else passes through untouched.
        if part.len() == 2 {
            out.push_str(&part.to_uppercase());
        } else if part.len() == 4 {
            let mut chars = part.chars();
            let first: String = chars.next().map(|c| c.to_uppercase().to_string()).unwrap();
            out.push_str(&first);
            out.push_str(&chars.as_str().to_lowercase());
        } else {
            out.push_str(part);
        }
    }
    out
}

/// Whether two tags name the same language, ignoring region.
///
/// "en-US" and "en" are the same language, so translating between them is a
/// no-op and the sidecar should skip building a `TranslationSession`.
fn same_language(a: &str, b: &str) -> bool {
    fn primary(tag: &str) -> String {
        tag.trim()
            .split(['-', '_'])
            .next()
            .unwrap_or("")
            .to_lowercase()
    }
    let (a, b) = (primary(a), primary(b));
    !a.is_empty() && a == b
}

/// Build the sidecar's argument vector.
fn build_args(config: &ProviderConfig) -> Vec<String> {
    let source = normalize_lang(config.source_language.as_deref().unwrap_or(""));
    let target = normalize_lang(&config.target_language);

    let mut args = vec![
        "--protocol".to_string(),
        PROTOCOL_VERSION.to_string(),
        "--source".to_string(),
        source.clone(),
        "--target".to_string(),
        target.clone(),
        "--sample-rate".to_string(),
        TARGET_SAMPLE_RATE.to_string(),
        "--channels".to_string(),
        "1".to_string(),
        "--format".to_string(),
        "s16le".to_string(),
    ];

    if same_language(&source, &target) {
        args.push("--no-translate".to_string());
    }

    args
}

/// Arguments for a one-shot capability probe.
pub(crate) fn probe_args(source: &str, target: &str) -> Vec<String> {
    vec![
        "--probe".to_string(),
        "--protocol".to_string(),
        PROTOCOL_VERSION.to_string(),
        "--source".to_string(),
        normalize_lang(source),
        "--target".to_string(),
        normalize_lang(target),
    ]
}

// ---------------------------------------------------------------------------
// Preflight and discovery
// ---------------------------------------------------------------------------

fn preflight() -> Result<PathBuf, String> {
    if let Some(path) = sidecar_override()? {
        // An explicit override is a developer or test decision; skip the OS and
        // architecture gates so a scripted fake can stand in on any platform.
        return Ok(path);
    }
    check_platform()?;
    resolve_bundled_sidecar()
}

fn sidecar_override() -> Result<Option<PathBuf>, String> {
    let Ok(raw) = std::env::var(SIDECAR_ENV) else {
        return Ok(None);
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let path = PathBuf::from(raw);
    if !path.is_file() {
        return Err(format!(
            "{} points at a missing file: {}",
            SIDECAR_ENV,
            path.display()
        ));
    }
    Ok(Some(path))
}

#[cfg(target_os = "macos")]
fn check_platform() -> Result<(), String> {
    if std::env::consts::ARCH != "aarch64" {
        return Err("The Apple on-device caption provider requires an Apple Silicon Mac. \
                    Select Gemini in Settings."
            .to_string());
    }

    match macos_major_version() {
        Some(major) if major >= MIN_MACOS_MAJOR => Ok(()),
        Some(major) => Err(format!(
            "The Apple on-device caption provider requires macOS {} (Tahoe) or later; \
             this Mac runs macOS {}. Select Gemini in Settings.",
            MIN_MACOS_MAJOR, major
        )),
        None => Err("Could not determine the macOS version. \
                     The Apple provider requires macOS 26 (Tahoe) or later."
            .to_string()),
    }
}

#[cfg(not(target_os = "macos"))]
fn check_platform() -> Result<(), String> {
    Err(
        "The Apple on-device caption provider requires macOS 26 (Tahoe) or later. \
         Select Gemini in Settings."
            .to_string(),
    )
}

/// Major version from `sw_vers -productVersion`, cached for the process.
#[cfg(target_os = "macos")]
pub(crate) fn macos_major_version() -> Option<u32> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Option<u32>> = OnceLock::new();

    *CACHE.get_or_init(|| {
        let out = std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()?;
        parse_macos_major(&String::from_utf8_lossy(&out.stdout))
    })
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn macos_major_version() -> Option<u32> {
    None
}

fn parse_macos_major(product_version: &str) -> Option<u32> {
    product_version
        .trim()
        .split('.')
        .next()?
        .parse::<u32>()
        .ok()
}

/// Locate the bundled helper.
///
/// Tauri copies `externalBin` entries — with the target-triple suffix stripped
/// — next to the app executable: `target/debug/` under `tauri dev`,
/// `SherPresent.app/Contents/MacOS/` when bundled. `current_exe` covers both,
/// which is what `tauri-plugin-shell` does internally; resolving it here avoids
/// taking on the plugin and its capability wiring for one spawn.
pub(crate) fn resolve_bundled_sidecar() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe failed: {}", e))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "app executable has no parent directory".to_string())?;

    let sibling = dir.join(SIDECAR_NAME);
    if sibling.is_file() {
        return Ok(sibling);
    }

    // Fallback in case the bundling strategy moves to `bundle.resources`.
    let resources = dir.join("../Resources").join(SIDECAR_NAME);
    if resources.is_file() {
        return Ok(resources);
    }

    Err(format!(
        "The Apple speech helper '{}' is missing from this build (looked in {}). \
         Rebuild with `bun run macos:sidecar`, or select Gemini in Settings.",
        SIDECAR_NAME,
        dir.display()
    ))
}

/// Result of `--probe`, surfaced to Settings so the operator can see why the
/// Apple provider is or is not usable before starting a show.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppleCaptionSupport {
    pub os_supported: bool,
    pub os_version: Option<String>,
    pub arch_supported: bool,
    pub speech_locale_supported: bool,
    pub speech_model_installed: bool,
    pub supported_locales: Vec<String>,
    /// "installed" | "notInstalled" | "unsupported"
    pub translation_status: String,
    pub message: Option<String>,
}

impl AppleCaptionSupport {
    /// The answer when the helper cannot even be asked.
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            os_supported: false,
            os_version: None,
            arch_supported: false,
            speech_locale_supported: false,
            speech_model_installed: false,
            supported_locales: Vec::new(),
            translation_status: "unsupported".to_string(),
            message: Some(message.into()),
        }
    }
}

/// How long to wait for the one-shot `--probe` to answer.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Run the helper's capability probe.
///
/// Never returns `Err` for an unsupported machine — "this Mac cannot do it, and
/// here is why" is a valid answer that Settings needs to render, not an error.
pub async fn probe_support(source: &str, target: &str) -> AppleCaptionSupport {
    let path = match preflight() {
        Ok(p) => p,
        Err(message) => return AppleCaptionSupport::unavailable(message),
    };

    let output = tokio::process::Command::new(&path)
        .args(probe_args(source, target))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output();

    let output = match timeout(PROBE_TIMEOUT, output).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            return AppleCaptionSupport::unavailable(format!(
                "Could not run the Apple speech helper: {}",
                e
            ))
        }
        Err(_) => {
            return AppleCaptionSupport::unavailable(
                "The Apple speech helper did not respond within 10 seconds.",
            )
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    // The probe prints exactly one line, but tolerate leading log noise.
    let line = stdout
        .lines()
        .find(|l| l.contains("\"availability\""))
        .unwrap_or("");

    match parse_availability(line) {
        Ok(support) => support,
        Err(e) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = stderr.trim().lines().last().unwrap_or_default();
            AppleCaptionSupport::unavailable(if detail.is_empty() {
                e
            } else {
                format!("{} — {}", e, detail)
            })
        }
    }
}

/// Parse the single `availability` line emitted by `--probe`.
pub(crate) fn parse_availability(line: &str) -> Result<AppleCaptionSupport, String> {
    let value: Value = serde_json::from_str(line.trim())
        .map_err(|e| format!("could not parse speech helper probe output: {}", e))?;

    if value.get("type").and_then(Value::as_str) != Some("availability") {
        return Err(format!(
            "unexpected speech helper probe output: {}",
            line.trim()
        ));
    }

    let speech = value.get("speech");
    let translation = value.get("translation");

    Ok(AppleCaptionSupport {
        os_supported: value
            .get("osSupported")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        os_version: value
            .get("osVersion")
            .and_then(Value::as_str)
            .map(str::to_string),
        // The helper only runs at all on a supported architecture, so reaching
        // this point implies the arch is fine unless it says otherwise.
        arch_supported: value
            .get("archSupported")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        speech_locale_supported: speech
            .and_then(|s| s.get("localeSupported"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        speech_model_installed: speech
            .and_then(|s| s.get("modelInstalled"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        supported_locales: speech
            .and_then(|s| s.get("supportedLocales"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        translation_status: translation
            .and_then(|t| t.get("status"))
            .and_then(Value::as_str)
            .unwrap_or("unsupported")
            .to_string(),
        message: value
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(source: Option<&str>, target: &str) -> ProviderConfig {
        ProviderConfig {
            api_key: String::new(),
            target_language: target.to_string(),
            source_language: source.map(str::to_string),
        }
    }

    // -- parse_line ---------------------------------------------------------

    #[test]
    fn test_parse_ready() {
        let line = r#"{"type":"ready","protocol":1,"sourceLocale":"en-US","targetLocale":"fr"}"#;
        assert_eq!(parse_line(line), LineOutcome::Ready);
    }

    #[test]
    fn test_parse_partial_and_final_both_replace() {
        let partial = parse_line(r#"{"type":"partial","source":"hel","translated":"bon"}"#);
        assert_eq!(
            partial,
            LineOutcome::Events(vec![ProviderEvent::Replace {
                source: "hel".into(),
                translated: "bon".into(),
            }])
        );

        let final_line =
            parse_line(r#"{"type":"final","source":"Hello.","translated":"Bonjour."}"#);
        assert_eq!(
            final_line,
            LineOutcome::Events(vec![ProviderEvent::Replace {
                source: "Hello.".into(),
                translated: "Bonjour.".into(),
            }])
        );
    }

    #[test]
    fn test_parse_partial_tolerates_missing_translation() {
        // A transcription-only session (--no-translate) omits the field.
        assert_eq!(
            parse_line(r#"{"type":"partial","source":"hi"}"#),
            LineOutcome::Events(vec![ProviderEvent::Replace {
                source: "hi".into(),
                translated: String::new(),
            }])
        );
    }

    #[test]
    fn test_parse_turn_complete() {
        assert_eq!(
            parse_line(r#"{"type":"turnComplete"}"#),
            LineOutcome::Events(vec![ProviderEvent::TurnComplete])
        );
    }

    #[test]
    fn test_parse_asset_progress_clamps() {
        assert_eq!(
            parse_line(r#"{"type":"assetProgress","stage":"speechModel","fraction":0.42}"#),
            LineOutcome::Progress {
                stage: "speechModel".into(),
                fraction: 0.42
            }
        );
        assert_eq!(
            parse_line(r#"{"type":"assetProgress","fraction":9.0}"#),
            LineOutcome::Progress {
                stage: "model".into(),
                fraction: 1.0
            }
        );
    }

    #[test]
    fn test_parse_error_fatal_flag_selects_outcome() {
        let fatal = parse_line(
            r#"{"type":"error","code":"translationNotInstalled","fatal":true,"message":"French is not installed"}"#,
        );
        assert_eq!(fatal, LineOutcome::Fatal("French is not installed".into()));

        let retryable = parse_line(
            r#"{"type":"error","code":"assetDownloadFailed","fatal":false,"message":"download failed"}"#,
        );
        assert_eq!(retryable, LineOutcome::Failed("download failed".into()));
    }

    #[test]
    fn test_parse_error_without_fatal_flag_is_fatal() {
        // Retrying an unrecognized error risks an endless respawn loop.
        assert!(matches!(
            parse_line(r#"{"type":"error","message":"boom"}"#),
            LineOutcome::Fatal(_)
        ));
    }

    #[test]
    fn test_parse_garbage_is_ignored_not_fatal() {
        // A malformed line must never end a live caption feed.
        for line in [
            "",
            "   ",
            "not json at all",
            "{}",
            r#"{"nope":1}"#,
            r#"{"type":"futureThing","data":5}"#,
            r#"{"type":42}"#,
        ] {
            assert_eq!(parse_line(line), LineOutcome::Ignore, "line: {:?}", line);
        }
    }

    // -- language handling --------------------------------------------------

    #[test]
    fn test_normalize_lang_handles_operator_typing() {
        assert_eq!(normalize_lang("  EN  "), "en");
        assert_eq!(normalize_lang("en-us"), "en-US");
        assert_eq!(normalize_lang("EN-US"), "en-US");
        assert_eq!(normalize_lang("en_US"), "en-US");
        assert_eq!(normalize_lang("fr"), "fr");
        assert_eq!(normalize_lang("zh-hans-cn"), "zh-Hans-CN");
        assert_eq!(normalize_lang(""), "");
        assert_eq!(normalize_lang("   "), "");
    }

    #[test]
    fn test_same_language_ignores_region_and_case() {
        assert!(same_language("en-US", "en"));
        assert!(same_language("EN", "en-GB"));
        assert!(same_language("fr", "fr-CA"));
        assert!(!same_language("en", "fr"));
        assert!(!same_language("", ""));
    }

    // -- build_args ---------------------------------------------------------

    #[test]
    fn test_build_args_normalizes_and_carries_format() {
        let args = build_args(&cfg(Some(" en_us "), "FR"));
        assert_eq!(args.iter().position(|a| a == "--source").map(|i| &args[i + 1]), Some(&"en-US".to_string()));
        assert_eq!(args.iter().position(|a| a == "--target").map(|i| &args[i + 1]), Some(&"fr".to_string()));
        assert!(args.contains(&"s16le".to_string()));
        assert!(args.contains(&TARGET_SAMPLE_RATE.to_string()));
        assert!(!args.contains(&"--no-translate".to_string()));
    }

    #[test]
    fn test_build_args_skips_translation_for_same_language() {
        // Region differences alone are not a translation.
        assert!(build_args(&cfg(Some("en-US"), "en")).contains(&"--no-translate".to_string()));
        assert!(build_args(&cfg(Some("en"), "en-GB")).contains(&"--no-translate".to_string()));
    }

    #[test]
    fn test_probe_args_normalize() {
        let args = probe_args(" en_us ", "FR");
        assert!(args.contains(&"--probe".to_string()));
        assert!(args.contains(&"en-US".to_string()));
        assert!(args.contains(&"fr".to_string()));
    }

    // -- exit classification ------------------------------------------------

    fn exit_status(code: i32) -> ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(code << 8)
    }

    #[test]
    fn test_exit_before_ready_is_fatal() {
        let outcome = classify_exit(Some(exit_status(3)), false, &[]);
        match outcome {
            SessionOutcome::Fatal { message } => {
                assert!(message.contains("before it was ready"), "{}", message);
                assert!(message.contains("macOS 26"), "{}", message);
            }
            other => panic!("expected Fatal, got {:?}", other),
        }
    }

    #[test]
    fn test_same_exit_after_ready_is_retryable() {
        let outcome = classify_exit(Some(exit_status(3)), true, &[]);
        assert!(matches!(outcome, SessionOutcome::Failed { .. }));
    }

    #[test]
    fn test_exit_classification_includes_stderr_tail() {
        let tail: Vec<String> = (0..10).map(|i| format!("line {}", i)).collect();
        let outcome = classify_exit(Some(exit_status(1)), false, &tail);
        match outcome {
            SessionOutcome::Fatal { message } => {
                // Only the last few lines, and the oldest must be gone.
                assert!(message.contains("line 9"), "{}", message);
                assert!(message.contains("line 5"), "{}", message);
                assert!(!message.contains("line 4"), "{}", message);
            }
            other => panic!("expected Fatal, got {:?}", other),
        }
    }

    #[test]
    fn test_with_stderr_tail_no_op_when_empty() {
        assert_eq!(with_stderr_tail("boom".into(), &[]), "boom");
    }

    // -- discovery ----------------------------------------------------------

    #[test]
    fn test_missing_sidecar_error_names_the_fix() {
        // `current_exe` is the test binary, which has no sidecar beside it.
        let err = resolve_bundled_sidecar().unwrap_err();
        assert!(err.contains("macos:sidecar"), "{}", err);
        assert!(err.contains("Gemini"), "{}", err);
    }

    #[test]
    fn test_parse_macos_major() {
        assert_eq!(parse_macos_major("26.1\n"), Some(26));
        assert_eq!(parse_macos_major("15.4.1"), Some(15));
        assert_eq!(parse_macos_major("26"), Some(26));
        assert_eq!(parse_macos_major(""), None);
        assert_eq!(parse_macos_major("garbage"), None);
    }

    // -- probe parsing ------------------------------------------------------

    #[test]
    fn test_parse_availability_full_line() {
        let line = r#"{"type":"availability","protocol":1,"osSupported":true,"osVersion":"26.1",
            "speech":{"localeSupported":true,"modelInstalled":false,"locale":"en-US",
                      "supportedLocales":["en-US","fr-FR"]},
            "translation":{"status":"notInstalled","source":"en","target":"fr"},
            "message":null}"#;
        let support = parse_availability(line).unwrap();

        assert!(support.os_supported);
        assert_eq!(support.os_version.as_deref(), Some("26.1"));
        assert!(support.speech_locale_supported);
        assert!(!support.speech_model_installed);
        assert_eq!(support.supported_locales, vec!["en-US", "fr-FR"]);
        assert_eq!(support.translation_status, "notInstalled");
    }

    #[test]
    fn test_parse_availability_rejects_other_messages() {
        assert!(parse_availability(r#"{"type":"ready"}"#).is_err());
        assert!(parse_availability("truncated {").is_err());
    }

    #[test]
    fn test_parse_availability_defaults_are_conservative() {
        let support = parse_availability(r#"{"type":"availability"}"#).unwrap();
        assert!(!support.os_supported);
        assert_eq!(support.translation_status, "unsupported");
        assert!(support.supported_locales.is_empty());
    }

    #[test]
    fn test_support_serializes_camel_case_for_the_frontend() {
        let json = serde_json::to_string(&AppleCaptionSupport::unavailable("nope")).unwrap();
        assert!(json.contains("\"osSupported\":false"));
        assert!(json.contains("\"translationStatus\""));
        assert!(!json.contains("os_supported"));
    }
}

/// End-to-end tests against a scripted stand-in for the Swift helper.
///
/// These cover the spawn / select / parse / classify pipeline — the layer most
/// likely to hold logic bugs and least dependent on Apple's frameworks — so it
/// stays verifiable on a machine that cannot compile Swift.
#[cfg(all(test, unix))]
mod fake_sidecar_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio::sync::mpsc;

    /// `SHERPRESENT_SPEECH_BIN` is process-global, so sessions must not overlap.
    static ENV_LOCK: Mutex<()> = Mutex::new(());
    static SCRIPT_SEQ: AtomicU32 = AtomicU32::new(0);

    fn write_script(body: &str) -> PathBuf {
        let n = SCRIPT_SEQ.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "sherpresent-fake-speech-{}-{}.sh",
            std::process::id(),
            n
        ));
        std::fs::write(&path, format!("#!/bin/sh\n{}\n", body)).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    struct Harness {
        events: mpsc::UnboundedReceiver<ProviderEvent>,
        audio_tx: mpsc::UnboundedSender<Vec<i16>>,
        shutdown_tx: watch::Sender<bool>,
        task: tokio::task::JoinHandle<()>,
        _script: PathBuf,
    }

    impl Harness {
        /// Wait for the next event, failing rather than hanging the suite.
        async fn next(&mut self) -> ProviderEvent {
            timeout(Duration::from_secs(10), self.events.recv())
                .await
                .expect("timed out waiting for a provider event")
                .expect("provider event channel closed early")
            }

        /// Drain events until the provider finishes on its own.
        async fn drain(mut self) -> Vec<ProviderEvent> {
            let mut out = Vec::new();
            while let Ok(Some(event)) = timeout(Duration::from_secs(10), self.events.recv()).await {
                out.push(event);
            }
            let _ = timeout(Duration::from_secs(10), self.task).await;
            out
        }
    }

    fn start(body: &str, guard: &std::sync::MutexGuard<'_, ()>) -> Harness {
        let _ = guard;
        let script = write_script(body);
        std::env::set_var(SIDECAR_ENV, &script);

        let (audio_tx, audio_rx) = mpsc::unbounded_channel::<Vec<i16>>();
        let (event_tx, events) = mpsc::unbounded_channel::<ProviderEvent>();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let config = ProviderConfig {
            api_key: String::new(),
            target_language: "fr".to_string(),
            source_language: Some("en-US".to_string()),
        };

        let task = tokio::spawn(run_loop(config, audio_rx, event_tx, shutdown_rx));

        Harness {
            events,
            audio_tx,
            shutdown_tx,
            task,
            _script: script,
        }
    }

    #[tokio::test]
    async fn test_happy_path_emits_replace_then_turn_complete() {
        let guard = ENV_LOCK.lock().unwrap();
        // `cat` at the end keeps the helper alive until stdin closes, the way a
        // real streaming session behaves.
        let mut h = start(
            r#"
echo '{"type":"ready","protocol":1,"sourceLocale":"en-US","targetLocale":"fr"}'
echo '{"type":"partial","source":"hel","translated":"bon"}'
echo '{"type":"partial","source":"hello word","translated":"bonjour mot"}'
echo '{"type":"final","source":"Hello world.","translated":"Bonjour le monde."}'
echo '{"type":"turnComplete"}'
cat > /dev/null
"#,
            &guard,
        );

        assert_eq!(h.next().await, ProviderEvent::Connected);
        assert_eq!(
            h.next().await,
            ProviderEvent::Replace {
                source: "hel".into(),
                translated: "bon".into()
            }
        );
        // The revision replaces rather than appends — the whole point of the
        // Replace variant.
        assert_eq!(
            h.next().await,
            ProviderEvent::Replace {
                source: "hello word".into(),
                translated: "bonjour mot".into()
            }
        );
        assert_eq!(
            h.next().await,
            ProviderEvent::Replace {
                source: "Hello world.".into(),
                translated: "Bonjour le monde.".into()
            }
        );
        assert_eq!(h.next().await, ProviderEvent::TurnComplete);

        // Audio keeps flowing into the still-running helper.
        h.audio_tx.send(vec![0i16; 1600]).unwrap();

        h.shutdown_tx.send(true).unwrap();
        timeout(Duration::from_secs(10), h.task)
            .await
            .expect("run_loop did not stop after shutdown")
            .unwrap();
    }

    #[tokio::test]
    async fn test_exit_before_ready_is_fatal_and_stops() {
        let guard = ENV_LOCK.lock().unwrap();
        let h = start(
            r#"
echo '[error] could not load the speech model' >&2
exit 1
"#,
            &guard,
        );

        let events = h.drain().await;
        // Exactly one Fatal and no respawn attempt.
        assert_eq!(events.len(), 1, "unexpected events: {:?}", events);
        match &events[0] {
            ProviderEvent::Fatal { message } => {
                assert!(message.contains("before it was ready"), "{}", message);
                // The stderr tail is what makes this diagnosable from a log.
                assert!(message.contains("could not load the speech model"), "{}", message);
            }
            other => panic!("expected Fatal, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_missing_language_pack_is_fatal_with_the_operator_message() {
        let guard = ENV_LOCK.lock().unwrap();
        let h = start(
            r#"
echo '{"type":"error","code":"translationNotInstalled","fatal":true,"message":"French is not installed. Open System Settings > General > Language & Region > Translation Languages."}'
exit 3
"#,
            &guard,
        );

        let events = h.drain().await;
        assert_eq!(events.len(), 1, "unexpected events: {:?}", events);
        match &events[0] {
            ProviderEvent::Fatal { message } => {
                assert!(message.contains("System Settings"), "{}", message);
            }
            other => panic!("expected Fatal, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_crash_after_ready_commits_the_line_and_respawns() {
        let guard = ENV_LOCK.lock().unwrap();
        let mut h = start(
            r#"
echo '{"type":"ready","protocol":1}'
echo '{"type":"partial","source":"half a sen","translated":"une demi"}'
exit 1
"#,
            &guard,
        );

        assert_eq!(h.next().await, ProviderEvent::Connected);
        assert_eq!(
            h.next().await,
            ProviderEvent::Replace {
                source: "half a sen".into(),
                translated: "une demi".into()
            }
        );

        // The half-finished line is committed before the next session starts,
        // so its first Replace cannot overwrite it mid-word.
        assert_eq!(h.next().await, ProviderEvent::TurnComplete);
        match h.next().await {
            ProviderEvent::Reconnecting { reason } => {
                assert!(reason.contains("exited"), "{}", reason);
            }
            other => panic!("expected Reconnecting, got {:?}", other),
        }

        // It really does respawn rather than giving up.
        assert_eq!(h.next().await, ProviderEvent::Connected);

        h.shutdown_tx.send(true).unwrap();
        let _ = timeout(Duration::from_secs(10), h.task).await;
    }

    #[tokio::test]
    async fn test_malformed_lines_do_not_end_the_session() {
        let guard = ENV_LOCK.lock().unwrap();
        let mut h = start(
            r#"
echo '{"type":"ready","protocol":1}'
echo 'not json at all'
echo '{"type":"somethingFromAFutureVersion"}'
echo ''
echo '{"type":"final","source":"still here","translated":"toujours la"}'
cat > /dev/null
"#,
            &guard,
        );

        assert_eq!(h.next().await, ProviderEvent::Connected);
        // Garbage is skipped, and the feed survives it.
        assert_eq!(
            h.next().await,
            ProviderEvent::Replace {
                source: "still here".into(),
                translated: "toujours la".into()
            }
        );

        h.shutdown_tx.send(true).unwrap();
        let _ = timeout(Duration::from_secs(10), h.task).await;
    }

    #[tokio::test]
    async fn test_capture_stopping_is_a_clean_shutdown() {
        let guard = ENV_LOCK.lock().unwrap();
        let mut h = start(
            r#"
echo '{"type":"ready","protocol":1}'
cat > /dev/null
"#,
            &guard,
        );

        assert_eq!(h.next().await, ProviderEvent::Connected);

        // Dropping the audio sender is what `CaptionEngine::stop` does first.
        drop(h.audio_tx);

        // No Fatal, no Reconnecting — the run just ends.
        let rest = timeout(Duration::from_secs(10), async {
            let mut out = Vec::new();
            while let Some(e) = h.events.recv().await {
                out.push(e);
            }
            out
        })
        .await
        .expect("run_loop did not stop when capture ended");

        assert!(rest.is_empty(), "unexpected events after capture stop: {:?}", rest);
    }

    #[tokio::test]
    async fn test_bad_override_path_is_fatal() {
        let guard = ENV_LOCK.lock().unwrap();
        let _ = &guard;
        std::env::set_var(SIDECAR_ENV, "/nonexistent/sherpresent-speech");

        let (_audio_tx, audio_rx) = mpsc::unbounded_channel::<Vec<i16>>();
        let (event_tx, mut events) = mpsc::unbounded_channel::<ProviderEvent>();
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);

        run_loop(
            ProviderConfig {
                api_key: String::new(),
                target_language: "fr".to_string(),
                source_language: Some("en-US".to_string()),
            },
            audio_rx,
            event_tx,
            shutdown_rx,
        )
        .await;

        match events.recv().await {
            Some(ProviderEvent::Fatal { message }) => {
                assert!(message.contains("missing file"), "{}", message);
            }
            other => panic!("expected Fatal, got {:?}", other),
        }
    }
}
