pub mod canva;
pub mod keynote;
pub mod libreoffice;
pub mod powerpoint;
#[cfg(target_os = "windows")]
pub mod powerpoint_windows;

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use crate::config::AdapterConfig;

/// Information about the current slide position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlideInfo {
    pub current: i32,
    pub total: i32,
    /// Transition duration in seconds (Keynote only). None for other adapters.
    pub transition_duration: Option<f64>,
}

/// State of a presentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresentationState {
    pub is_open: bool,
    pub is_presenting: bool,
}

/// Connection status for network-based adapters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

/// Combined status for live display
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LiveStatus {
    pub is_open: bool,
    pub is_presenting: bool,
    pub current_slide: i32,
    pub total_slides: i32,
    pub zoom_level: Option<i32>,
    pub presenter_notes: Option<String>,
    /// Current build/animation step on this slide (0 = no builds fired yet)
    pub current_build: Option<i32>,
    /// Total click-triggered build steps on this slide (0 or None = no builds)
    pub total_builds: Option<i32>,
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

    /// Navigate to a specific slide number (1-indexed)
    fn goto_slide(&self, _name: &str, _slide: i32) -> Result<SlideInfo, String> {
        Err("Go-to-slide not supported for this adapter".to_string())
    }

    /// Get notes zoom level (if supported)
    fn get_notes_zoom(&self) -> Result<Option<i32>, String> {
        Ok(None)
    }

    /// Set notes zoom level (if supported)
    fn set_notes_zoom(&self, _level: i32) -> Result<(), String> {
        Err("Notes zoom not supported for this adapter".to_string())
    }

    /// Get presenter notes for the current slide (if supported)
    fn get_presenter_notes(&self, _name: &str) -> Result<Option<String>, String> {
        Ok(None)
    }

    /// Get presenter notes for all slides (if supported)
    /// Returns a map of slide number (1-indexed) → notes text.
    /// Adapters that don't support bulk fetch return an empty map;
    /// notes accumulate progressively via polling instead.
    fn get_all_presenter_notes(&self, _name: &str) -> Result<HashMap<i32, String>, String> {
        Ok(HashMap::new())
    }

    /// Get connection status (for network-based adapters)
    fn connection_status(&self) -> ConnectionStatus {
        ConnectionStatus::Connected
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
                presenter_notes: None,
                current_build: None,
                total_builds: None,
            };
        }

        let slide_info = self.get_slide_info(name).unwrap_or(SlideInfo {
            current: 0,
            total: 0,
            transition_duration: None,
        });

        let zoom_level = self.get_notes_zoom().ok().flatten();
        let presenter_notes = self.get_presenter_notes(name).ok().flatten();

        LiveStatus {
            is_open: state.is_open,
            is_presenting: state.is_presenting,
            current_slide: slide_info.current,
            total_slides: slide_info.total,
            zoom_level,
            presenter_notes,
            current_build: None,
            total_builds: None,
        }
    }
}

// =============================================================================
// SHARED ADAPTER UTILITIES
// =============================================================================

/// PowerPoint's fixed zoom levels for presenter view notes
pub const ZOOM_LEVELS: [i32; 5] = [100, 150, 200, 300, 400];

/// Get the next zoom level up from current
pub fn get_next_zoom_level(current: i32) -> i32 {
    for &level in &ZOOM_LEVELS {
        if level > current {
            return level;
        }
    }
    ZOOM_LEVELS[ZOOM_LEVELS.len() - 1]
}

/// Get the next zoom level down from current
pub fn get_prev_zoom_level(current: i32) -> i32 {
    for &level in ZOOM_LEVELS.iter().rev() {
        if level < current {
            return level;
        }
    }
    ZOOM_LEVELS[0]
}

/// Parse the "N|||text" notes response format used by AppleScript/COM adapters.
///
/// Each line is expected to be `<slide_number>|||<notes_text>`.
/// Empty notes are skipped.
pub fn parse_notes_response(result: &str) -> HashMap<i32, String> {
    let mut notes = HashMap::new();
    let mut total_lines = 0;
    let mut empty_count = 0;
    for line in result.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        total_lines += 1;
        if let Some((num_str, text)) = line.split_once("|||") {
            if let Ok(slide_num) = num_str.trim().parse::<i32>() {
                let text = text.trim();
                if !text.is_empty() && text != "missing value" {
                    log::debug!("parse_notes: slide {} has notes ({} chars): {:?}", slide_num, text.len(), &text[..text.len().min(60)]);
                    notes.insert(slide_num, text.to_string());
                } else {
                    empty_count += 1;
                }
            }
        }
    }
    log::debug!("parse_notes: {} lines total, {} with notes, {} empty", total_lines, notes.len(), empty_count);
    notes
}

/// Get an adapter by name
///
/// Platform-aware adapter selection:
/// - PowerPoint: macOS (AppleScript), Windows (COM)
/// - Keynote: macOS only (AppleScript)
/// - LibreOffice: Cross-platform (TCP socket protocol, network-configurable)
/// - Canva: Cross-platform (webview-based, requires AppHandle — use CanvaAdapter singleton)
pub fn get_adapter(adapter_name: &str, config: &AdapterConfig) -> Option<Box<dyn PresentationAdapter>> {
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
        "libreoffice" => {
            let (host, port) = match config {
                AdapterConfig::LibreOffice { host, port } => (host.clone(), *port),
                _ => ("127.0.0.1".to_string(), libreoffice::IMPRESS_REMOTE_PORT),
            };
            Some(Box::new(libreoffice::LibreOfficeAdapter::new(host, port)))
        }
        // Canva adapter is a singleton managed via AppState — not created here
        "canva" => None,
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

    // LibreOffice is network-based — available on all platforms
    adapters.push(("libreoffice", "LibreOffice Impress"));

    // Canva is webview-based — available on all platforms
    adapters.push(("canva", "Canva"));

    adapters
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AdapterConfig;

    #[test]
    fn test_get_adapter_libreoffice_with_defaults() {
        let config = AdapterConfig::None;
        let adapter = get_adapter("libreoffice", &config);
        assert!(adapter.is_some());
    }

    #[test]
    fn test_get_adapter_libreoffice_with_custom_config() {
        let config = AdapterConfig::LibreOffice {
            host: "10.0.0.5".to_string(),
            port: 2002,
        };
        let adapter = get_adapter("libreoffice", &config);
        assert!(adapter.is_some());
    }

    #[test]
    fn test_get_adapter_canva_returns_none() {
        // Canva is a singleton managed via AppState, not created by get_adapter
        let config = AdapterConfig::None;
        let adapter = get_adapter("canva", &config);
        assert!(adapter.is_none());
    }

    #[test]
    fn test_get_adapter_unknown_returns_none() {
        let config = AdapterConfig::None;
        let adapter = get_adapter("nonexistent", &config);
        assert!(adapter.is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_get_adapter_powerpoint_unavailable_on_linux() {
        let config = AdapterConfig::None;
        assert!(get_adapter("powerpoint", &config).is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_get_adapter_keynote_unavailable_on_linux() {
        let config = AdapterConfig::None;
        assert!(get_adapter("keynote", &config).is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_adapter_powerpoint_available_on_macos() {
        let config = AdapterConfig::None;
        assert!(get_adapter("powerpoint", &config).is_some());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_adapter_keynote_available_on_macos() {
        let config = AdapterConfig::None;
        assert!(get_adapter("keynote", &config).is_some());
    }

    #[test]
    fn test_available_adapters_includes_libreoffice() {
        let adapters = get_available_adapters();
        assert!(adapters.iter().any(|(id, _)| *id == "libreoffice"));
    }

    #[test]
    fn test_available_adapters_includes_canva() {
        let adapters = get_available_adapters();
        assert!(adapters.iter().any(|(id, _)| *id == "canva"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_available_adapters_includes_powerpoint_on_macos() {
        let adapters = get_available_adapters();
        assert!(adapters.iter().any(|(id, _)| *id == "powerpoint"));
        assert!(adapters.iter().any(|(id, _)| *id == "keynote"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_available_adapters_excludes_powerpoint_on_linux() {
        let adapters = get_available_adapters();
        assert!(!adapters.iter().any(|(id, _)| *id == "powerpoint"));
        assert!(!adapters.iter().any(|(id, _)| *id == "keynote"));
    }

    #[test]
    fn test_parse_notes_response() {
        let input = "1|||Hello world\n2|||\n3|||Some notes here\n";
        let notes = parse_notes_response(input);
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[&1], "Hello world");
        assert_eq!(notes[&3], "Some notes here");
        assert!(!notes.contains_key(&2)); // empty notes skipped
    }

    #[test]
    fn test_zoom_levels() {
        assert_eq!(get_next_zoom_level(100), 150);
        assert_eq!(get_next_zoom_level(150), 200);
        assert_eq!(get_next_zoom_level(400), 400);

        assert_eq!(get_prev_zoom_level(400), 300);
        assert_eq!(get_prev_zoom_level(150), 100);
        assert_eq!(get_prev_zoom_level(100), 100);
    }

    #[test]
    fn test_connection_status_default_is_connected() {
        // The default trait impl returns Connected (for local adapters like PowerPoint)
        struct DummyAdapter;
        impl PresentationAdapter for DummyAdapter {
            fn get_open_presentations(&self) -> Result<Vec<String>, String> { Ok(vec![]) }
            fn get_presentation_state(&self, _: &str) -> Result<PresentationState, String> {
                Ok(PresentationState { is_open: false, is_presenting: false })
            }
            fn get_slide_info(&self, _: &str) -> Result<SlideInfo, String> {
                Ok(SlideInfo { current: 0, total: 0, transition_duration: None })
            }
            fn next_slide(&self, _: &str) -> Result<SlideInfo, String> {
                Ok(SlideInfo { current: 0, total: 0, transition_duration: None })
            }
            fn prev_slide(&self, _: &str) -> Result<SlideInfo, String> {
                Ok(SlideInfo { current: 0, total: 0, transition_duration: None })
            }
        }
        let adapter = DummyAdapter;
        assert!(matches!(adapter.connection_status(), ConnectionStatus::Connected));
    }
}
