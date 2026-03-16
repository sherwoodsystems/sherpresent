use std::process::Command;

/// Execute an AppleScript and return the result.
///
/// On macOS, tries the in-process NSAppleScript path first (faster, no process spawn),
/// falling back to `osascript -e` on failure. On other platforms, uses osascript only.
pub fn run_applescript(script: &str) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        match native::run_applescript_cached(script) {
            Ok(result) => return Ok(result),
            Err(e) => {
                log::debug!("NSAppleScript failed ({}), falling back to osascript", e);
            }
        }
    }

    run_applescript_process(script)
}

/// Execute an AppleScript via `osascript -e` (process-based fallback).
fn run_applescript_process(script: &str) -> Result<String, String> {
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

// =============================================================================
// macOS: In-process NSAppleScript via objc2 FFI
// =============================================================================

#[cfg(target_os = "macos")]
mod native {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use objc2::rc::Retained;
    use objc2::AnyThread;
    use objc2_foundation::{NSAppleScript, NSDictionary, NSString};

    /// Wrapper to allow NSAppleScript in a static (it uses Mach ports, not the run loop).
    struct SendSyncScript(Retained<NSAppleScript>);

    // SAFETY: NSAppleScript dispatches Apple Events via Mach ports, which are thread-safe.
    // The "main thread only" note in Apple docs refers to scripts that interact with the
    // run loop, which ours don't — we only send/receive Apple Events to other processes.
    unsafe impl Send for SendSyncScript {}
    unsafe impl Sync for SendSyncScript {}

    /// Global cache of compiled NSAppleScript instances.
    static COMPILED_CACHE: std::sync::LazyLock<Mutex<HashMap<String, SendSyncScript>>> =
        std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

    /// Serializes all NSAppleScript execution.
    /// NSAppleScript is !Send+!Sync — concurrent execution from multiple threads
    /// causes Apple Event replies to get crossed. This mutex ensures only one
    /// script executes at a time. This is fine in practice because Apple Events
    /// to the same target app are serialized by the OS anyway.
    static EXECUTION_LOCK: std::sync::LazyLock<Mutex<()>> =
        std::sync::LazyLock::new(|| Mutex::new(()));

    /// Execute an AppleScript in-process using NSAppleScript, with compiled caching.
    ///
    /// The cache mutex is only held briefly for lookup/insert — never during execution.
    /// This prevents deadlocks when multiple threads run AppleScripts concurrently.
    pub fn run_applescript_cached(script: &str) -> Result<String, String> {
        // Acquire execution lock — serializes all AppleScript execution.
        let _exec_guard = EXECUTION_LOCK.lock().unwrap();

        // Phase 1: Check cache
        {
            let cache = COMPILED_CACHE.lock().unwrap();
            if let Some(cached) = cache.get(script) {
                let script_ref = cached.0.clone();
                drop(cache);
                return execute_script(&script_ref);
            }
        }

        // Phase 2: Compile (cache lock released, exec lock still held)
        let ns_source = NSString::from_str(script);
        let ns_script = NSAppleScript::initWithSource(NSAppleScript::alloc(), &ns_source)
            .ok_or_else(|| "Failed to create NSAppleScript".to_string())?;

        let mut compile_error: Option<Retained<NSDictionary<NSString>>> = None;
        let compiled = unsafe { ns_script.compileAndReturnError(Some(&mut compile_error)) };

        if !compiled {
            return Err(extract_error_message(compile_error.as_deref()));
        }

        // Phase 3: Execute
        let result = execute_script(&ns_script);

        // Phase 4: Cache on success
        if result.is_ok() {
            let mut cache = COMPILED_CACHE.lock().unwrap();
            cache.insert(script.to_string(), SendSyncScript(ns_script));
        }

        result
    }

    /// Execute an already-compiled NSAppleScript and extract the string result.
    ///
    /// Uses `objc2::msg_send!` directly to handle the nullable return from
    /// `executeAndReturnError:` — the generated binding declares it as non-optional
    /// `Retained`, but Apple's API can return nil on failure.
    fn execute_script(script: &NSAppleScript) -> Result<String, String> {
        let mut exec_error: Option<Retained<NSDictionary<NSString>>> = None;

        // Use catch_unwind to handle the case where executeAndReturnError returns nil.
        // objc2's generated binding declares a non-optional return type, so nil causes a panic.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            unsafe { script.executeAndReturnError(Some(&mut exec_error)) }
        }));

        match result {
            Ok(descriptor) => {
                match descriptor.stringValue() {
                    Some(s) => Ok(s.to_string()),
                    None => {
                        // Script succeeded but returned a non-string descriptor
                        Ok(String::new())
                    }
                }
            }
            Err(_) => {
                // executeAndReturnError returned nil (script failed)
                Err(extract_error_message(exec_error.as_deref()))
            }
        }
    }

    /// Extract error message from an NSAppleScript error dictionary.
    fn extract_error_message(error_dict: Option<&NSDictionary<NSString>>) -> String {
        if let Some(dict) = error_dict {
            let key = NSString::from_str("NSAppleScriptErrorMessage");
            if let Some(value) = dict.objectForKey(&key) {
                // objectForKey returns Retained<AnyObject>; cast to NSString
                let ptr = Retained::as_ptr(&value) as *const NSString;
                let ns_str: &NSString = unsafe { &*ptr };
                return ns_str.to_string();
            }
        }
        "AppleScript execution failed".to_string()
    }

    /// Clear all cached compiled scripts.
    pub fn clear_compiled_cache() {
        let mut cache = COMPILED_CACHE.lock().unwrap();
        let count = cache.len();
        cache.clear();
        if count > 0 {
            log::info!("Cleared {} compiled AppleScript(s) from cache", count);
        }
    }

    /// Run a quick smoke test to verify NSAppleScript works on this system.
    /// Returns true if in-process execution is functional.
    #[allow(dead_code)]
    pub fn is_native_available() -> bool {
        let ns_source = NSString::from_str("return 1");
        let Some(ns_script) = NSAppleScript::initWithSource(NSAppleScript::alloc(), &ns_source) else {
            return false;
        };
        let mut err: Option<Retained<NSDictionary<NSString>>> = None;
        if !unsafe { ns_script.compileAndReturnError(Some(&mut err)) } {
            return false;
        }
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            unsafe { ns_script.executeAndReturnError(Some(&mut err)) }
        }))
        .is_ok()
    }
}

/// Clear the compiled AppleScript cache (macOS only). No-op on other platforms.
pub fn clear_compiled_cache() {
    #[cfg(target_os = "macos")]
    native::clear_compiled_cache();
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
    fn test_cached_execution_reuses_compiled() {
        let r1 = run_applescript("return 42");
        assert!(r1.is_ok());
        assert_eq!(r1.unwrap(), "42");

        let r2 = run_applescript("return 42");
        assert!(r2.is_ok());
        assert_eq!(r2.unwrap(), "42");
    }

    #[test]
    fn test_clear_cache() {
        let _ = run_applescript("return 99");
        clear_compiled_cache();
        let result = run_applescript("return 99");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "99");
    }

    #[test]
    fn test_process_fallback() {
        let result = run_applescript_process("return \"fallback\"");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "fallback");
    }

    #[test]
    fn test_error_script_doesnt_hang() {
        // A script that will fail — should return Err, not hang
        let result = run_applescript("tell application \"NonExistentApp12345\" to quit");
        // We don't care if it's Ok or Err, just that it doesn't hang
        let _ = result;
    }
}
