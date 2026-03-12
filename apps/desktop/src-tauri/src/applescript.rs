use std::process::Command;

/// Execute an AppleScript and return the result.
///
/// Uses `osascript -e` to run the script. Single quotes in the script
/// are automatically escaped.
pub fn run_applescript(script: &str) -> Result<String, String> {
    // Escape single quotes for shell
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
}
