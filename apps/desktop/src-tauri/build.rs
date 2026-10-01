fn main() {
    tauri_build::build();

    // tauri_build copies the staged sidecars (`bun run macos:sidecar`) next
    // to the binary, but only when this script runs. Without this, a rebuilt
    // helper never reaches target/ and the app keeps launching the old one.
    println!("cargo:rerun-if-changed=binaries");

    // Link macOS frameworks needed for NSAppleScript FFI
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=dylib=objc");
    }
}
