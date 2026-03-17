fn main() {
    tauri_build::build();

    // Link macOS frameworks needed for NSAppleScript FFI
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=dylib=objc");
    }
}
