/// Execute AppleScript in-process via NSAppleScript FFI on macOS,
/// falling back to `osascript -e` on other platforms.

#[cfg(target_os = "macos")]
#[allow(non_camel_case_types)]
mod nsapplescript {
    use std::ffi::CStr;
    use std::os::raw::c_char;

    #[repr(C)]
    struct objc_object {
        _private: [u8; 0],
    }
    type id = *mut objc_object;
    type SEL = *const objc_object;
    type Class = *const objc_object;

    const NIL: id = std::ptr::null_mut();

    #[link(name = "Foundation", kind = "framework")]
    extern "C" {}

    // `objc_msgSend` is declared per call-arity below rather than as a single
    // C-variadic `...` binding. On Apple Silicon, Apple's arm64 ABI passes
    // variadic arguments differently from fixed-arity ones, so a variadic Rust
    // FFI declaration hands the callee garbage instead of the real argument —
    // it happens to work on x86_64, where the two ABIs coincide, which is how
    // this went unnoticed until run on real Apple Silicon hardware.
    #[allow(clashing_extern_declarations)]
    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const c_char) -> Class;
        fn sel_registerName(name: *const c_char) -> SEL;

        #[link_name = "objc_msgSend"]
        fn objc_msgSend0(obj: id, sel: SEL) -> id;
        #[link_name = "objc_msgSend"]
        fn objc_msgSend1(obj: id, sel: SEL, arg1: id) -> id;
        #[link_name = "objc_msgSend"]
        fn objc_msgSend1_cstr(obj: id, sel: SEL, arg1: *const c_char) -> id;
        #[link_name = "objc_msgSend"]
        fn objc_msgSend1_errptr(obj: id, sel: SEL, arg1: *mut id) -> id;
    }

    fn sel(name: &str) -> SEL {
        let c = std::ffi::CString::new(name).unwrap();
        unsafe { sel_registerName(c.as_ptr()) }
    }

    fn class(name: &str) -> Class {
        let c = std::ffi::CString::new(name).unwrap();
        unsafe { objc_getClass(c.as_ptr()) }
    }

    unsafe fn nsstring(s: &str) -> id {
        let cls = class("NSString");
        let c = std::ffi::CString::new(s).unwrap();
        let alloc: id = objc_msgSend0(cls as id, sel("alloc"));
        objc_msgSend1_cstr(alloc, sel("initWithUTF8String:"), c.as_ptr())
    }

    unsafe fn from_nsstring(nsstr: id) -> Option<String> {
        if nsstr.is_null() {
            return None;
        }
        let ptr: *const c_char = objc_msgSend0(nsstr, sel("UTF8String")) as *const c_char;
        if ptr.is_null() {
            return None;
        }
        Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
    }

    /// Execute an AppleScript using NSAppleScript (in-process, no osascript spawn).
    pub fn run(script: &str) -> Result<String, String> {
        unsafe {
            let source = nsstring(script);

            let cls = class("NSAppleScript");
            let alloc: id = objc_msgSend0(cls as id, sel("alloc"));
            let script_obj: id = objc_msgSend1(alloc, sel("initWithSource:"), source);

            if script_obj.is_null() {
                let _: id = objc_msgSend0(source, sel("release"));
                return Err("Failed to create NSAppleScript".to_string());
            }

            let mut error_dict: id = NIL;
            let error_ptr: *mut id = &mut error_dict;

            let result: id = objc_msgSend1_errptr(script_obj, sel("executeAndReturnError:"), error_ptr);

            if result.is_null() || !error_dict.is_null() {
                let err_msg = if !error_dict.is_null() {
                    let key = nsstring("NSAppleScriptErrorMessage");
                    let msg: id = objc_msgSend1(error_dict, sel("objectForKey:"), key);
                    let _: id = objc_msgSend0(key, sel("release"));
                    from_nsstring(msg).unwrap_or_else(|| "Unknown AppleScript error".to_string())
                } else {
                    "AppleScript execution failed".to_string()
                };

                let _: id = objc_msgSend0(script_obj, sel("release"));
                let _: id = objc_msgSend0(source, sel("release"));
                return Err(err_msg);
            }

            let string_val: id = objc_msgSend0(result, sel("stringValue"));
            let output = from_nsstring(string_val).unwrap_or_default();

            let _: id = objc_msgSend0(script_obj, sel("release"));
            let _: id = objc_msgSend0(source, sel("release"));

            Ok(output)
        }
    }
}

/// Execute an AppleScript and return the result.
///
/// On macOS: uses in-process NSAppleScript FFI (~5ms status, ~60ms commands).
/// On other platforms: falls back to spawning `osascript -e`.
#[cfg(target_os = "macos")]
pub fn run_applescript(script: &str) -> Result<String, String> {
    nsapplescript::run(script)
}

#[cfg(not(target_os = "macos"))]
pub fn run_applescript(script: &str) -> Result<String, String> {
    use std::process::Command;

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

/// No-op — kept as a public function so callers don't need to change.
pub fn clear_compiled_cache() {
    // No-op
}

/// Whether a GUI app is currently running, by its process name, without
/// launching it. `tell application "X" to ...` launches X via Launch
/// Services if it isn't already running — fine for sending a real command,
/// but wrong for a presence check, which would otherwise force-open
/// PowerPoint/Keynote just by polling their status. System Events is always
/// resident and never launches the process it's asked about.
pub fn is_app_running(process_name: &str) -> bool {
    let script = format!(
        r#"tell application "System Events" to (exists process "{}")"#,
        process_name
    );
    matches!(run_applescript(&script), Ok(result) if result.trim() == "true")
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
