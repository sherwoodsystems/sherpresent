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
    fn get_notes_zoom(&self) -> Result<Option<i32>, String>;           // default: Ok(None)
    fn set_notes_zoom(&self, level: i32) -> Result<(), String>;        // default: Err(...)
    fn get_presenter_notes(&self, name: &str) -> Result<Option<String>, String>; // default: Ok(None)
    fn get_live_status(&self, name: &str) -> LiveStatus;               // default implementation
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
    presenter_notes: Option<String>,
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
- Notes: `content of text range of text frame of shape 2 of notes page of slide N`

## macOS Keynote Adapter

- **File**: `adapters/keynote.rs`
- **Method**: AppleScript via `osascript`
- **Platform**: `#[cfg(target_os = "macos")]`

Key AppleScript operations:
- List presentations: `tell application "Keynote" to get name of every document`
- Check slideshow: `tell application "Keynote" to get playing of slideshow 1`
- Navigate: `tell application "Keynote" to show next`
- Notes: `presenter notes of current slide`

## Windows PowerPoint Adapter

- **File**: `adapters/powerpoint_windows.rs`
- **Method**: COM automation
- **Platform**: `#[cfg(target_os = "windows")]`

Uses the Windows COM interface to control PowerPoint.Application.

Key COM paths:
- Notes: `Presentation → Slides → Item(N) → NotesPage → Shapes → Placeholders → Item(2) → TextFrame → TextRange → Text`

## Linux LibreOffice Adapter

- **File**: `adapters/libreoffice.rs`
- **Method**: TCP socket protocol
- **Platform**: All (but currently only listed on Linux)

Connects to LibreOffice Impress on `localhost:1599` using the Impress Remote Protocol. Notes are delivered as `slide_notes` messages containing HTML, which is stripped to plain text.

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
- **macOS**: powerpoint, keynote, canva
- **Windows**: powerpoint, canva
- **Linux**: libreoffice, canva

## Presenter Notes Support

All adapters support retrieving presenter notes for the current slide. Notes are returned in `LiveStatus.presenter_notes` and displayed in the StatusDisplay component when available.

| Adapter | Method | Format |
|---------|--------|--------|
| macOS PowerPoint | AppleScript (`notes page` text range) | Plain text |
| macOS Keynote | AppleScript (`presenter notes of current slide`) | Plain text |
| Windows PowerPoint | COM (`NotesPage.Shapes.Placeholders(2).TextFrame.TextRange.Text`) | Plain text |
| LibreOffice | TCP `slide_notes` message (auto-received on slide change) | HTML → stripped to plain text |
| Canva | WebSocket interception (binary `cwsp` protocol) | Plain text |
