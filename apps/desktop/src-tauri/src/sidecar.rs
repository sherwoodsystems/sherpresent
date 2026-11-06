//! Locating the Swift helper binaries (`externalBin` in `tauri.macos.conf.json`).
//!
//! Shared by the Apple speech provider and the caption video output, which
//! each drive their own helper process: discovery, plus the stderr handling
//! both supervisors use.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, BufReader};

/// Lines of helper stderr kept for diagnostics.
const STDERR_TAIL_LINES: usize = 20;

/// Lines of stderr appended to a failure message.
const STDERR_TAIL_REPORTED: usize = 5;

/// Rolling tail of a helper's stderr.
pub type StderrTail = Arc<Mutex<VecDeque<String>>>;

/// Locate a bundled helper by name.
///
/// Tauri copies `externalBin` entries — with the target-triple suffix stripped
/// — next to the app executable: `target/debug/` under `tauri dev`,
/// `SherPresent.app/Contents/MacOS/` when bundled. `current_exe` covers both,
/// which is what `tauri-plugin-shell` does internally; resolving it here avoids
/// taking on the plugin and its capability wiring for one spawn.
///
/// The error names the directory searched; callers append their own fix.
pub fn resolve_bundled(name: &str) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe failed: {}", e))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "app executable has no parent directory".to_string())?;

    let sibling = dir.join(name);
    if sibling.is_file() {
        return Ok(sibling);
    }

    // Fallback in case the bundling strategy moves to `bundle.resources`.
    let resources = dir.join("../Resources").join(name);
    if resources.is_file() {
        return Ok(resources);
    }

    Err(format!(
        "The helper '{}' is missing from this build (looked in {}).",
        name,
        dir.display()
    ))
}

/// An explicit helper path from the environment, for development and for
/// tests that stand in a scripted fake. Unset or blank means "not overridden".
pub fn env_override(var: &str) -> Result<Option<PathBuf>, String> {
    let Ok(raw) = std::env::var(var) else {
        return Ok(None);
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let path = PathBuf::from(raw);
    if !path.is_file() {
        return Err(format!(
            "{} points at a missing file: {}",
            var,
            path.display()
        ));
    }
    Ok(Some(path))
}

/// Forward helper stderr to the log under `[tag]`, honouring the `[error]` /
/// `[warn]` prefixes the Swift helpers write, and keep a rolling tail for
/// failure messages.
pub fn spawn_stderr_pump(stderr: tokio::process::ChildStderr, tail: StderrTail, tag: &'static str) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            if line.contains("[error]") {
                log::error!("[{}] {}", tag, line);
            } else if line.contains("[warn]") {
                log::warn!("[{}] {}", tag, line);
            } else {
                log::info!("[{}] {}", tag, line);
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

pub fn snapshot_tail(tail: &StderrTail) -> Vec<String> {
    tail.lock()
        .map(|buf| buf.iter().cloned().collect())
        .unwrap_or_default()
}

/// Append the last few stderr lines to a failure message.
pub fn with_stderr_tail(message: String, tail: &[String]) -> String {
    let start = tail.len().saturating_sub(STDERR_TAIL_REPORTED);
    let recent = &tail[start..];
    if recent.is_empty() {
        return message;
    }
    format!("{} — {}", message, recent.join(" | "))
}
