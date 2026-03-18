//! Standalone test: NSAppleScript vs osascript for PowerPoint commands.
//!
//! Run with:
//!   cargo run --bin test_nsapplescript
//!
//! Make sure PowerPoint is open with a presentation before running.

use std::process::Command;
use std::time::Instant;

// Objective-C FFI to call NSAppleScript
mod nsapplescript {
    use std::ffi::CStr;
    use std::os::raw::c_char;

    // Opaque Objective-C types
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

    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const c_char) -> Class;
        fn sel_registerName(name: *const c_char) -> SEL;
        fn objc_msgSend(obj: id, sel: SEL, ...) -> id;
    }

    // Helper to create a selector
    fn sel(name: &str) -> SEL {
        let c = std::ffi::CString::new(name).unwrap();
        unsafe { sel_registerName(c.as_ptr()) }
    }

    // Helper to get a class
    fn class(name: &str) -> Class {
        let c = std::ffi::CString::new(name).unwrap();
        unsafe { objc_getClass(c.as_ptr()) }
    }

    // Create NSString from &str
    unsafe fn nsstring(s: &str) -> id {
        let cls = class("NSString");
        let c = std::ffi::CString::new(s).unwrap();
        let alloc: id = objc_msgSend(cls as id, sel("alloc"));
        objc_msgSend(alloc, sel("initWithUTF8String:"), c.as_ptr())
    }

    // Read NSString to String
    unsafe fn from_nsstring(nsstr: id) -> Option<String> {
        if nsstr.is_null() {
            return None;
        }
        let ptr: *const c_char = objc_msgSend(nsstr, sel("UTF8String")) as *const c_char;
        if ptr.is_null() {
            return None;
        }
        Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
    }

    /// Execute an AppleScript using NSAppleScript (in-process, no osascript spawn).
    pub fn run(script: &str) -> Result<String, String> {
        unsafe {
            let source = nsstring(script);

            // [[NSAppleScript alloc] initWithSource:source]
            let cls = class("NSAppleScript");
            let alloc: id = objc_msgSend(cls as id, sel("alloc"));
            let script_obj: id = objc_msgSend(alloc, sel("initWithSource:"), source);

            if script_obj.is_null() {
                // Release source
                let _: id = objc_msgSend(source, sel("release"));
                return Err("Failed to create NSAppleScript".to_string());
            }

            // Create error dictionary pointer (NSAppleEventDescriptor **)
            let mut error_dict: id = NIL;
            let error_ptr: *mut id = &mut error_dict;

            // [script executeAndReturnError:&errorDict]
            let result: id = objc_msgSend(script_obj, sel("executeAndReturnError:"), error_ptr);

            if result.is_null() || !error_dict.is_null() {
                // Get error description
                let err_msg = if !error_dict.is_null() {
                    // Error dict is an NSDictionary, get NSAppleScriptErrorMessage
                    let key = nsstring("NSAppleScriptErrorMessage");
                    let msg: id = objc_msgSend(error_dict, sel("objectForKey:"), key);
                    let _: id = objc_msgSend(key, sel("release"));
                    from_nsstring(msg).unwrap_or_else(|| "Unknown AppleScript error".to_string())
                } else {
                    "AppleScript execution failed".to_string()
                };

                let _: id = objc_msgSend(script_obj, sel("release"));
                let _: id = objc_msgSend(source, sel("release"));
                return Err(err_msg);
            }

            // Get string value from NSAppleEventDescriptor
            let string_val: id = objc_msgSend(result, sel("stringValue"));
            let output = from_nsstring(string_val).unwrap_or_default();

            let _: id = objc_msgSend(script_obj, sel("release"));
            let _: id = objc_msgSend(source, sel("release"));

            Ok(output)
        }
    }
}

fn run_osascript(script: &str) -> Result<String, String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| format!("Failed to execute osascript: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            "osascript failed".to_string()
        } else {
            stderr
        })
    }
}

fn main() {
    println!("=== NSAppleScript vs osascript Test ===\n");

    // Test 1: Simple arithmetic (no app interaction)
    println!("--- Test 1: Simple arithmetic (return 1 + 1) ---");
    test_both("return 1 + 1");

    // Test 2: Get presentation list
    println!("\n--- Test 2: Get open presentations ---");
    let list_script =
        r#"tell application "Microsoft PowerPoint" to get name of every presentation"#;
    let presentations = test_both(list_script);

    // If we got a presentation name, use it for the remaining tests
    let pres_name = match &presentations {
        Some(name) if !name.is_empty() => {
            let first = name.split(", ").next().unwrap_or(name).trim().to_string();
            println!("  Using presentation: {}", first);
            first
        }
        _ => {
            println!("\n  No presentations open. Open a PowerPoint file and re-run.");
            println!("  Skipping slide tests.");
            return;
        }
    };

    // Test 3: Get live status
    println!("\n--- Test 3: Get live status ---");
    let status_script = format!(
        r#"tell application "Microsoft PowerPoint"
            set isOpen to "false"
            set isPresenting to "false"
            set currentSlide to "0"
            set totalSlides to "0"
            try
                set pres to presentation "{}"
                set isOpen to "true"
                try
                    set ssw to slide show window of pres
                    set ssView to slide show view of ssw
                    set isPresenting to "true"
                    set currentSlide to (current show position of ssView) as text
                    set totalSlides to (count slides of pres) as text
                end try
            end try
            return isOpen & "," & isPresenting & "," & currentSlide & "," & totalSlides
        end tell"#,
        pres_name
    );
    let status = test_both(&status_script);

    let is_presenting = status
        .as_ref()
        .map(|s| s.contains("true,true"))
        .unwrap_or(false);

    if !is_presenting {
        println!("\n  Not in slideshow mode. Start a presentation to test next/prev.");
        println!("  Skipping next/prev tests.");
        return;
    }

    // Test 4: Next slide
    println!("\n--- Test 4: Next slide ---");
    let next_script = format!(
        r#"tell application "Microsoft PowerPoint"
            set ssView to slide show view of slide show window of presentation "{}"
            set pres to presentation "{}"
            set oldPos to current show position of ssView
            set totalSlides to count slides of pres
            if oldPos >= totalSlides then
                return "BOUNDARY," & (oldPos as text) & "," & (totalSlides as text)
            end if
            go to next slide ssView
            set newPos to current show position of ssView
            return "OK," & (newPos as text) & "," & (totalSlides as text)
        end tell"#,
        pres_name, pres_name
    );
    test_both(&next_script);

    // Test 5: Previous slide (go back)
    println!("\n--- Test 5: Previous slide ---");
    let prev_script = format!(
        r#"tell application "Microsoft PowerPoint"
            set ssView to slide show view of slide show window of presentation "{}"
            set pres to presentation "{}"
            set oldPos to current show position of ssView
            set totalSlides to count slides of pres
            if oldPos <= 1 then
                return "BOUNDARY," & (oldPos as text) & "," & (totalSlides as text)
            end if
            go to previous slide ssView
            set newPos to current show position of ssView
            return "OK," & (newPos as text) & "," & (totalSlides as text)
        end tell"#,
        pres_name, pres_name
    );
    test_both(&prev_script);

    // Test 6: Rapid-fire NSAppleScript (5 quick status checks)
    println!("\n--- Test 6: Rapid-fire NSAppleScript (5x status) ---");
    let start = Instant::now();
    for i in 1..=5 {
        let t = Instant::now();
        let r = nsapplescript::run(&status_script);
        let elapsed = t.elapsed();
        println!(
            "  #{}: {:>6.1}ms  {}",
            i,
            elapsed.as_secs_f64() * 1000.0,
            match &r {
                Ok(v) => format!("OK: {}", v),
                Err(e) => format!("ERR: {}", e),
            }
        );
    }
    let total = start.elapsed();
    println!(
        "  Total: {:.1}ms  Avg: {:.1}ms",
        total.as_secs_f64() * 1000.0,
        total.as_secs_f64() * 1000.0 / 5.0
    );

    println!("\n=== Done ===");
}

/// Run script with both methods, print timing, return the NSAppleScript result.
fn test_both(script: &str) -> Option<String> {
    // osascript first
    let t1 = Instant::now();
    let r1 = run_osascript(script);
    let e1 = t1.elapsed();

    // NSAppleScript second
    let t2 = Instant::now();
    let r2 = nsapplescript::run(script);
    let e2 = t2.elapsed();

    println!(
        "  osascript:    {:>6.1}ms  {}",
        e1.as_secs_f64() * 1000.0,
        match &r1 {
            Ok(v) => format!("OK: {}", v),
            Err(e) => format!("ERR: {}", e),
        }
    );
    println!(
        "  NSAppleScript:{:>6.1}ms  {}",
        e2.as_secs_f64() * 1000.0,
        match &r2 {
            Ok(v) => format!("OK: {}", v),
            Err(e) => format!("ERR: {}", e),
        }
    );

    if let (Ok(a), Ok(b)) = (&r1, &r2) {
        if a != b {
            println!("  ⚠ MISMATCH! osascript='{}' vs NSAppleScript='{}'", a, b);
        }
    }

    r2.ok()
}
