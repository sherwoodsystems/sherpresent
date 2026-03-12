# Platform Adapters Specification

## PresentationAdapter Trait

All presentation software adapters implement this Rust trait:

```rust
pub trait PresentationAdapter: Send + Sync {
    fn get_open_presentations(&self) -> Result<Vec<String>, String>;
    fn get_presentation_state(&self, name: &str) -> Result<PresentationState, String>;
    fn get_slide_info(&self, name: &str) -> Result<SlideInfo, String>;
    fn next_slide(&self, name: &str) -> Result<SlideInfo, String>;
    fn prev_slide(&self, name: &str) -> Result<SlideInfo, String>;
    fn get_notes_zoom(&self) -> Result<Option<i32>, String>;     // default: Ok(None)
    fn set_notes_zoom(&self, level: i32) -> Result<(), String>;  // default: Err(...)
    fn get_live_status(&self, name: &str) -> LiveStatus;         // default implementation
}
```

### Data Types

```rust
struct SlideInfo { current: i32, total: i32 }
struct PresentationState { is_open: bool, is_presenting: bool }
struct LiveStatus {
    is_open: bool,
    is_presenting: bool,
    current_slide: i32,
    total_slides: i32,
    zoom_level: Option<i32>,
}
```

## macOS PowerPoint Adapter

- **File**: `adapters/powerpoint.rs`
- **Method**: AppleScript via `osascript`
- **Platform**: `#[cfg(target_os = "macos")]`

Key AppleScript operations:
- List presentations: `tell application "Microsoft PowerPoint" to get name of every presentation`
- Check slideshow: `tell application "Microsoft PowerPoint" to get slide state of slide show view of slide show window 1`
- Navigate: `tell application "Microsoft PowerPoint" to go to next slide of slide show view of slide show window 1`
- Zoom: Manipulates presenter view notes font size

## macOS Keynote Adapter

- **File**: `adapters/keynote.rs`
- **Method**: AppleScript via `osascript`
- **Platform**: `#[cfg(target_os = "macos")]`

Key AppleScript operations:
- List presentations: `tell application "Keynote" to get name of every document`
- Check slideshow: `tell application "Keynote" to get playing of slideshow 1`
- Navigate: `tell application "Keynote" to show next`

## Windows PowerPoint Adapter

- **File**: `adapters/powerpoint_windows.rs`
- **Method**: COM automation
- **Platform**: `#[cfg(target_os = "windows")]`

Uses the Windows COM interface to control PowerPoint.Application.

## Linux LibreOffice Adapter

- **File**: `adapters/libreoffice.rs`
- **Method**: TCP socket protocol
- **Platform**: All (but currently only listed on Linux)

Connects to LibreOffice Impress on `localhost:2002` using the LibreOffice remote control protocol.

## Adapter Selection

```rust
pub fn get_adapter(name: &str) -> Option<Box<dyn PresentationAdapter>> {
    match name {
        "powerpoint" => /* macOS: AppleScript, Windows: COM, Linux: None */,
        "keynote"    => /* macOS only */,
        "libreoffice" => /* all platforms */,
        _ => None,
    }
}
```

Available adapters vary by platform:
- **macOS**: powerpoint, keynote
- **Windows**: powerpoint
- **Linux**: libreoffice
