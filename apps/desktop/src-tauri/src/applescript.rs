use std::process::Command;

/// Execute an AppleScript via `osascript -e` and return the result.
///
/// Simple blocking approach — no caching, no threads, no timeouts.
/// Keynote responds fast; PowerPoint is the only slow one, and the
/// complexity of NSAppleScript + EXECUTION_LOCK was making it worse.
pub fn run_applescript(script: &str) -> Result<String, String> {
    let escaped = script.replace('\'', "'\\''");

    let output = Command::new("osascript")
        .arg("-e")
        .arg(&escaped)
        .output()
        .map_err(|e| format!("Failed to execute osascript: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err("AppleScript execution failed with no error message".to_string())
        } else {
            Err(stderr)
        }
    }
}

/// No-op — the compiled cache was removed along with the NSAppleScript native module.
/// Kept as a public function so callers don't need to change.
pub fn clear_compiled_cache() {
    // No-op: NSAppleScript caching has been removed.
}

#[cfg(test)]
#[cfg(target_os = "macos")]
mod tests {
    use super::*;

    #[test]
    fn test_simple_applescript() {
        let result = run_applescript("return 1 + 1");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "2");
    }

    #[test]
    fn test_string_result() {
        let result = run_applescript("return \"hello\"");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn test_error_script() {
        let result = run_applescript("tell application \"NonExistentApp12345\" to quit");
        // We don't care if it's Ok or Err, just that it doesn't hang
        let _ = result;
    }
}
