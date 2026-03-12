pub mod keynote;
pub mod libreoffice;
pub mod powerpoint;
#[cfg(target_os = "windows")]
pub mod powerpoint_windows;

use serde::{Deserialize, Serialize};

/// Information about the current slide position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlideInfo {
    pub current: i32,
    pub total: i32,
}

/// State of a presentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresentationState {
    pub is_open: bool,
    pub is_presenting: bool,
}

/// Combined status for live display
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiveStatus {
    pub is_open: bool,
    pub is_presenting: bool,
    pub current_slide: i32,
    pub total_slides: i32,
    pub zoom_level: Option<i32>,
}


/// Trait for presentation application adapters
pub trait PresentationAdapter: Send + Sync {
    /// Get list of open presentation names
    fn get_open_presentations(&self) -> Result<Vec<String>, String>;

    /// Get the state of a specific presentation
    fn get_presentation_state(&self, name: &str) -> Result<PresentationState, String>;

    /// Get current slide info for a presentation
    fn get_slide_info(&self, name: &str) -> Result<SlideInfo, String>;

    /// Navigate to next slide
    fn next_slide(&self, name: &str) -> Result<SlideInfo, String>;

    /// Navigate to previous slide
    fn prev_slide(&self, name: &str) -> Result<SlideInfo, String>;

    /// Get notes zoom level (if supported)
    fn get_notes_zoom(&self) -> Result<Option<i32>, String> {
        Ok(None)
    }

    /// Set notes zoom level (if supported)
    fn set_notes_zoom(&self, _level: i32) -> Result<(), String> {
        Err("Notes zoom not supported for this adapter".to_string())
    }

    /// Get full live status
    fn get_live_status(&self, name: &str) -> LiveStatus {
        let state = self.get_presentation_state(name).unwrap_or(PresentationState {
            is_open: false,
            is_presenting: false,
        });

        if !state.is_presenting {
            return LiveStatus {
                is_open: state.is_open,
                is_presenting: false,
                current_slide: 0,
                total_slides: 0,
                zoom_level: None,
            };
        }

        let slide_info = self.get_slide_info(name).unwrap_or(SlideInfo {
            current: 0,
            total: 0,
        });

        let zoom_level = self.get_notes_zoom().ok().flatten();

        LiveStatus {
            is_open: state.is_open,
            is_presenting: state.is_presenting,
            current_slide: slide_info.current,
            total_slides: slide_info.total,
            zoom_level,
        }
    }
}

/// Get an adapter by name
///
/// Platform-aware adapter selection:
/// - PowerPoint: macOS (AppleScript), Windows (COM)
/// - Keynote: macOS only (AppleScript)
/// - LibreOffice: Cross-platform (TCP socket protocol)
pub fn get_adapter(adapter_name: &str) -> Option<Box<dyn PresentationAdapter>> {
    match adapter_name {
        "powerpoint" => {
            #[cfg(target_os = "macos")]
            return Some(Box::new(powerpoint::PowerPointAdapter));
            #[cfg(target_os = "windows")]
            return Some(Box::new(powerpoint_windows::PowerPointWindowsAdapter));
            #[cfg(target_os = "linux")]
            return None; // PowerPoint not available on Linux
        }
        "keynote" => {
            #[cfg(target_os = "macos")]
            return Some(Box::new(keynote::KeynoteAdapter));
            #[cfg(not(target_os = "macos"))]
            return None; // Keynote is macOS only
        }
        "libreoffice" => Some(Box::new(libreoffice::LibreOfficeAdapter::new())),
        _ => None,
    }
}

/// Get list of available adapters for the current platform
pub fn get_available_adapters() -> Vec<(&'static str, &'static str)> {
    let mut adapters = Vec::new();

    #[cfg(target_os = "macos")]
    {
        adapters.push(("powerpoint", "Microsoft PowerPoint"));
        adapters.push(("keynote", "Keynote"));
    }

    #[cfg(target_os = "windows")]
    {
        adapters.push(("powerpoint", "Microsoft PowerPoint"));
    }

    // LibreOffice remote control is not finished - only show on Linux for now
    #[cfg(target_os = "linux")]
    adapters.push(("libreoffice", "LibreOffice Impress"));

    adapters
}
