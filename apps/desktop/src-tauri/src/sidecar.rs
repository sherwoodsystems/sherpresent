//! Locating the Swift helper binaries (`externalBin` in `tauri.macos.conf.json`).
//!
//! Shared by the Apple speech provider and the caption video output, which
//! each drive their own helper process.

use std::path::PathBuf;

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
        return Err(format!("{} points at a missing file: {}", var, path.display()));
    }
    Ok(Some(path))
}
