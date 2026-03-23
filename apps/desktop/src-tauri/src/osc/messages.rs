//! # OSC Message Definitions
//!
//! This module defines all the OSC messages used for remote presentation control.
//!
//! ## Quick OSC Primer
//!
//! OSC messages have two parts:
//! 1. **Address** - A URL-like path (e.g., `/clicker/next`)
//! 2. **Arguments** - Typed values (integers, floats, strings, etc.)
//!
//! We use the `/clicker/` prefix for all our messages to avoid conflicts
//! with other OSC software that might be running.
//!
//! ## Message Types
//!
//! ### Incoming Commands (what we receive on port 9000)
//! - `/clicker/next` - Advance to next slide
//! - `/clicker/prev` or `/clicker/previous` - Go back one slide
//! - `/clicker/zoom` - Query current notes zoom level
//! - `/clicker/zoomIn` - Increase notes zoom
//! - `/clicker/zoomOut` - Decrease notes zoom
//! - `/clicker/status` - Request full state update
//! - `/clicker/refresh` - Force state re-sync from presentation app
//!
//! ### Outgoing Feedback (what we send on port 9001)
//! - `/clicker/state/presenting` - 0 or 1: is slideshow active?
//! - `/clicker/state/open` - 0 or 1: is presentation file open?
//! - `/clicker/slide/current` - Current slide number
//! - `/clicker/slide/total` - Total slide count
//! - `/clicker/zoom/level` - Current zoom percentage (100, 150, 200, etc.)

use rosc::{OscMessage, OscType};

use super::state_manager::CachedState;

// =============================================================================
// INCOMING COMMANDS
// =============================================================================

/// Represents an incoming OSC command that we handle.
#[derive(Debug, Clone, PartialEq)]
pub enum OscCommand {
    /// Advance to next slide
    Next,
    /// Go to previous slide
    Previous,
    /// Query current notes zoom level (returns feedback)
    Zoom,
    /// Increase notes zoom level
    ZoomIn,
    /// Decrease notes zoom level
    ZoomOut,
    /// Request full state update (returns feedback)
    Status,
    /// Force refresh state from presentation software
    Refresh,
    /// Jump to a specific slide number
    Goto { slide: i32 },

    /// Scroll the stage view notes up
    ScrollUp,
    /// Scroll the stage view notes down
    ScrollDown,

    /// An OSC address we don't recognize
    /// The String contains the original address for logging
    Unknown(String),
}

/// Direction for scroll commands (sent via broadcast channel to web server)
#[derive(Debug, Clone, PartialEq)]
pub enum ScrollDirection {
    Up,
    Down,
}

impl OscCommand {
    /// Parse an OSC message (address + arguments) into a command variant.
    pub fn from_message(address: &str, args: &[OscType]) -> Self {
        match address {
            "/clicker/next" => Self::Next,
            "/clicker/prev" | "/clicker/previous" => Self::Previous,
            "/clicker/zoom" => Self::Zoom,
            "/clicker/zoomIn" => Self::ZoomIn,
            "/clicker/zoomOut" => Self::ZoomOut,
            "/clicker/status" => Self::Status,
            "/clicker/refresh" => Self::Refresh,
            "/clicker/goto" => {
                let slide = Self::get_int_arg(args, 0).unwrap_or(1);
                Self::Goto { slide }
            }
            "/clicker/scrollUp" => Self::ScrollUp,
            "/clicker/scrollDown" => Self::ScrollDown,

            other => Self::Unknown(other.to_string()),
        }
    }

    /// Helper to extract integer argument
    fn get_int_arg(args: &[OscType], index: usize) -> Option<i32> {
        args.get(index).and_then(|arg| match arg {
            OscType::Int(i) => Some(*i),
            _ => None,
        })
    }

    /// Returns true if this command triggers a state feedback response.
    #[allow(dead_code)]
    pub fn is_query(&self) -> bool {
        matches!(self, Self::Zoom | Self::Status)
    }
}

// =============================================================================
// OUTGOING FEEDBACK
// =============================================================================

/// Builds outgoing OSC feedback messages from presentation state.
///
/// ## Design Note
///
/// This is a "zero-sized type" (ZST) - it has no fields, so it takes up
/// no memory. It's essentially a namespace for related functions.
/// We could use free functions instead, but grouping them under a type
/// makes the API cleaner: `OscFeedback::from_state(...)`.
pub struct OscFeedback;

impl OscFeedback {
    /// Create all feedback messages from the current presentation state.
    ///
    /// ## Returns
    ///
    /// A Vec of OscMessage structs ready to be encoded and sent.
    /// Each message contains one piece of state information.
    ///
    /// ## Why send separate messages?
    ///
    /// OSC clients (like Bitfocus Companion) typically bind UI elements
    /// to individual OSC addresses. By sending separate messages, clients
    /// can subscribe to just the data they need.
    pub fn from_state(state: &CachedState) -> Vec<OscMessage> {
        let mut messages = vec![
            OscMessage {
                addr: "/clicker/state/presenting".to_string(),
                args: vec![OscType::Int(if state.is_presenting { 1 } else { 0 })],
            },
            OscMessage {
                addr: "/clicker/state/open".to_string(),
                args: vec![OscType::Int(if state.is_open { 1 } else { 0 })],
            },
            OscMessage {
                addr: "/clicker/slide/current".to_string(),
                args: vec![OscType::Int(state.current_slide)],
            },
            OscMessage {
                addr: "/clicker/slide/total".to_string(),
                args: vec![OscType::Int(state.total_slides)],
            },
            OscMessage {
                addr: "/clicker/zoom/level".to_string(),
                args: vec![OscType::Int(state.zoom_level.unwrap_or(0))],
            },
        ];

        // Build/animation step info (only sent when builds exist on this slide)
        if let (Some(current_build), Some(total_builds)) = (state.current_build, state.total_builds) {
            messages.push(OscMessage {
                addr: "/clicker/slide/build".to_string(),
                args: vec![OscType::Int(current_build)],
            });
            messages.push(OscMessage {
                addr: "/clicker/slide/builds".to_string(),
                args: vec![OscType::Int(total_builds)],
            });
        }

        messages
    }

    /// Create a single feedback message for just the zoom level.
    ///
    /// Used when responding to `/clicker/zoom` query.
    pub fn zoom_only(zoom_level: Option<i32>) -> OscMessage {
        OscMessage {
            addr: "/clicker/zoom/level".to_string(),
            args: vec![OscType::Int(zoom_level.unwrap_or(0))],
        }
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_parsing() {
        assert_eq!(OscCommand::from_message("/clicker/next", &[]), OscCommand::Next);
        assert_eq!(
            OscCommand::from_message("/clicker/prev", &[]),
            OscCommand::Previous
        );
        assert_eq!(
            OscCommand::from_message("/clicker/previous", &[]),
            OscCommand::Previous
        );
        assert_eq!(OscCommand::from_message("/clicker/zoom", &[]), OscCommand::Zoom);
        assert_eq!(
            OscCommand::from_message("/clicker/zoomIn", &[]),
            OscCommand::ZoomIn
        );
        assert_eq!(
            OscCommand::from_message("/clicker/zoomOut", &[]),
            OscCommand::ZoomOut
        );
        assert_eq!(
            OscCommand::from_message("/clicker/status", &[]),
            OscCommand::Status
        );
        assert_eq!(
            OscCommand::from_message("/clicker/refresh", &[]),
            OscCommand::Refresh
        );
    }

    #[test]
    fn test_scroll_command_parsing() {
        assert_eq!(
            OscCommand::from_message("/clicker/scrollUp", &[]),
            OscCommand::ScrollUp
        );
        assert_eq!(
            OscCommand::from_message("/clicker/scrollDown", &[]),
            OscCommand::ScrollDown
        );
    }

    #[test]
    fn test_goto_command() {
        let cmd = OscCommand::from_message("/clicker/goto", &[OscType::Int(5)]);
        assert_eq!(cmd, OscCommand::Goto { slide: 5 });

        let cmd = OscCommand::from_message("/clicker/goto", &[]);
        assert_eq!(cmd, OscCommand::Goto { slide: 1 });
    }

    #[test]
    fn test_unknown_command() {
        let cmd = OscCommand::from_message("/some/random/path", &[]);
        match cmd {
            OscCommand::Unknown(addr) => assert_eq!(addr, "/some/random/path"),
            _ => panic!("Expected Unknown variant"),
        }
    }

    #[test]
    fn test_is_query() {
        assert!(OscCommand::Zoom.is_query());
        assert!(OscCommand::Status.is_query());
        assert!(!OscCommand::Next.is_query());
        assert!(!OscCommand::Previous.is_query());
        assert!(!OscCommand::ZoomIn.is_query());
        assert!(!OscCommand::ZoomOut.is_query());
        assert!(!OscCommand::Refresh.is_query());
    }
}
