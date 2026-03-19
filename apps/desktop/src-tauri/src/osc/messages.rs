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
///
/// ## How Rust Enums Work (for TypeScript devs)
///
/// Unlike TypeScript's string literal types, Rust enums are actual types
/// with variants. The `Unknown(String)` variant holds the original address
/// for commands we don't recognize - useful for logging/debugging.
///
/// Pattern matching (like a switch statement) is exhaustive in Rust,
/// meaning the compiler ensures we handle every variant.
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

    // =========================================================================
    // CHANNEL COMMANDS - for peer-to-peer sync
    // =========================================================================
    /// Peer announcing its presence: instance_id, channel, ip, port
    ChannelAnnounce {
        instance_id: String,
        channel: String,
        ip: String,
        port: i32,
    },
    /// Peer leaving the channel
    ChannelLeave { instance_id: String },
    /// Peer heartbeat (keep-alive)
    ChannelHeartbeat { instance_id: String },
    /// Forwarded next command from peer
    ChannelCmdNext { origin: String },
    /// Forwarded prev command from peer
    ChannelCmdPrev { origin: String },
    /// Forwarded goto command from peer
    ChannelCmdGoto { origin: String, slide: i32 },

    /// An OSC address we don't recognize
    /// The String contains the original address for logging
    Unknown(String),
}

/// Result of parsing a channel-based OSC command
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelCommand {
    /// The channel name from the OSC address
    pub channel: String,
    /// The parsed command
    pub command: OscCommand,
}

impl OscCommand {
    /// Parse a channel-based OSC message (e.g., /clicker/<channel>/next)
    /// Returns Some((channel_name, command)) if the message matches channel format,
    /// None if it doesn't match the expected pattern.
    pub fn from_channel_message(address: &str, args: &[OscType]) -> Option<ChannelCommand> {
        // Expected format: /clicker/<channel>/<command>
        let parts: Vec<&str> = address.split('/').collect();

        // parts[0] is empty (before first /), parts[1] is "clicker", parts[2] is channel, parts[3] is command
        if parts.len() < 4 || parts[1] != "clicker" {
            return None;
        }

        let channel = parts[2].to_string();

        // Validate channel name (must be one of valid channels)
        if !crate::config::ChannelConfig::is_valid_channel_name(&channel) {
            return None;
        }

        let command = match parts[3] {
            "next" => Self::Next,
            "prev" | "previous" => Self::Previous,
            "goto" => {
                let slide = Self::get_int_arg(args, 0).unwrap_or(1);
                Self::ChannelCmdGoto { origin: String::new(), slide }
            }
            "black" => Self::ChannelCmdNext { origin: "black".to_string() }, // Placeholder for black screen
            "white" => Self::ChannelCmdNext { origin: "white".to_string() }, // Placeholder for white screen
            "resume" => Self::ChannelCmdNext { origin: "resume".to_string() }, // Placeholder for resume
            "status" => Self::Status,
            "refresh" => Self::Refresh,
            _ => return None,
        };

        Some(ChannelCommand { channel, command })
    }

    /// Check if an OSC address matches a specific channel
    pub fn matches_channel(address: &str, channel: &str) -> bool {
        let prefix = format!("/clicker/{}/", channel);
        address.starts_with(&prefix)
    }

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

            // Channel commands with arguments
            "/clicker/channel/announce" => {
                let instance_id = Self::get_string_arg(args, 0).unwrap_or_default();
                let channel = Self::get_string_arg(args, 1).unwrap_or_default();
                let ip = Self::get_string_arg(args, 2).unwrap_or_default();
                let port = Self::get_int_arg(args, 3).unwrap_or(9000);
                Self::ChannelAnnounce { instance_id, channel, ip, port }
            }
            "/clicker/channel/leave" => {
                let instance_id = Self::get_string_arg(args, 0).unwrap_or_default();
                Self::ChannelLeave { instance_id }
            }
            "/clicker/channel/heartbeat" => {
                let instance_id = Self::get_string_arg(args, 0).unwrap_or_default();
                Self::ChannelHeartbeat { instance_id }
            }
            "/clicker/channel/cmd/next" => {
                let origin = Self::get_string_arg(args, 0).unwrap_or_default();
                Self::ChannelCmdNext { origin }
            }
            "/clicker/channel/cmd/prev" => {
                let origin = Self::get_string_arg(args, 0).unwrap_or_default();
                Self::ChannelCmdPrev { origin }
            }
            "/clicker/channel/cmd/goto" => {
                let origin = Self::get_string_arg(args, 0).unwrap_or_default();
                let slide = Self::get_int_arg(args, 1).unwrap_or(1);
                Self::ChannelCmdGoto { origin, slide }
            }

            other => Self::Unknown(other.to_string()),
        }
    }

    /// Helper to extract string argument
    fn get_string_arg(args: &[OscType], index: usize) -> Option<String> {
        args.get(index).and_then(|arg| match arg {
            OscType::String(s) => Some(s.clone()),
            _ => None,
        })
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

    /// Returns true if this is a channel-related command
    #[allow(dead_code)]
    pub fn is_channel_command(&self) -> bool {
        matches!(
            self,
            Self::ChannelAnnounce { .. }
                | Self::ChannelLeave { .. }
                | Self::ChannelHeartbeat { .. }
                | Self::ChannelCmdNext { .. }
                | Self::ChannelCmdPrev { .. }
                | Self::ChannelCmdGoto { .. }
        )
    }
}

// =============================================================================
// OUTGOING CHANNEL MESSAGES
// =============================================================================

/// Builds outgoing OSC messages for channel peer communication.
#[allow(dead_code)]
pub struct ChannelMessages;

#[allow(dead_code)]
impl ChannelMessages {
    /// Announce our presence to peers
    pub fn announce(instance_id: &str, channel: &str, ip: &str, port: u16) -> OscMessage {
        OscMessage {
            addr: "/clicker/channel/announce".to_string(),
            args: vec![
                OscType::String(instance_id.to_string()),
                OscType::String(channel.to_string()),
                OscType::String(ip.to_string()),
                OscType::Int(port as i32),
            ],
        }
    }

    /// Announce we're leaving the channel
    pub fn leave(instance_id: &str) -> OscMessage {
        OscMessage {
            addr: "/clicker/channel/leave".to_string(),
            args: vec![OscType::String(instance_id.to_string())],
        }
    }

    /// Send a heartbeat
    pub fn heartbeat(instance_id: &str) -> OscMessage {
        OscMessage {
            addr: "/clicker/channel/heartbeat".to_string(),
            args: vec![OscType::String(instance_id.to_string())],
        }
    }

    /// Forward a next command to peers
    pub fn cmd_next(origin_id: &str) -> OscMessage {
        OscMessage {
            addr: "/clicker/channel/cmd/next".to_string(),
            args: vec![OscType::String(origin_id.to_string())],
        }
    }

    /// Forward a prev command to peers
    pub fn cmd_prev(origin_id: &str) -> OscMessage {
        OscMessage {
            addr: "/clicker/channel/cmd/prev".to_string(),
            args: vec![OscType::String(origin_id.to_string())],
        }
    }

    /// Forward a goto command to peers
    pub fn cmd_goto(origin_id: &str, slide: i32) -> OscMessage {
        OscMessage {
            addr: "/clicker/channel/cmd/goto".to_string(),
            args: vec![
                OscType::String(origin_id.to_string()),
                OscType::Int(slide),
            ],
        }
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
    /// Create all 5 feedback messages from the current presentation state.
    ///
    /// ## Returns
    ///
    /// A Vec of OscMessage structs ready to be encoded and sent.
    /// Each message contains one piece of state information.
    ///
    /// ## Why send 5 separate messages?
    ///
    /// OSC clients (like Bitfocus Companion) typically bind UI elements
    /// to individual OSC addresses. By sending separate messages, clients
    /// can subscribe to just the data they need.
    pub fn from_state(state: &CachedState) -> Vec<OscMessage> {
        let mut messages = vec![
            // Is a slideshow currently running?
            // OscType::Int is a 32-bit signed integer
            OscMessage {
                addr: "/clicker/state/presenting".to_string(),
                args: vec![OscType::Int(if state.is_presenting { 1 } else { 0 })],
            },
            // Is the presentation file open?
            OscMessage {
                addr: "/clicker/state/open".to_string(),
                args: vec![OscType::Int(if state.is_open { 1 } else { 0 })],
            },
            // Current slide number (0 if not presenting)
            OscMessage {
                addr: "/clicker/slide/current".to_string(),
                args: vec![OscType::Int(state.current_slide)],
            },
            // Total slides in presentation
            OscMessage {
                addr: "/clicker/slide/total".to_string(),
                args: vec![OscType::Int(state.total_slides)],
            },
            // Notes zoom level (0 if not available)
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

    /// Create channel-aware feedback messages for broadcast mode.
    ///
    /// These use the format `/clicker/<channel>/state/<property>` so that
    /// receivers can filter by channel.
    pub fn from_state_with_channel(state: &CachedState, channel: &str) -> Vec<OscMessage> {
        let mut messages = vec![
            OscMessage {
                addr: format!("/clicker/{}/state/presenting", channel),
                args: vec![OscType::Int(if state.is_presenting { 1 } else { 0 })],
            },
            OscMessage {
                addr: format!("/clicker/{}/state/open", channel),
                args: vec![OscType::Int(if state.is_open { 1 } else { 0 })],
            },
            // Combined slide message with current and total
            OscMessage {
                addr: format!("/clicker/{}/state/slide", channel),
                args: vec![
                    OscType::Int(state.current_slide),
                    OscType::Int(state.total_slides),
                ],
            },
            OscMessage {
                addr: format!("/clicker/{}/state/zoom", channel),
                args: vec![OscType::Int(state.zoom_level.unwrap_or(0))],
            },
        ];

        if let (Some(current_build), Some(total_builds)) = (state.current_build, state.total_builds) {
            messages.push(OscMessage {
                addr: format!("/clicker/{}/state/build", channel),
                args: vec![
                    OscType::Int(current_build),
                    OscType::Int(total_builds),
                ],
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
        // Test that known addresses parse correctly
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
    fn test_goto_command() {
        // Goto with integer argument
        let cmd = OscCommand::from_message("/clicker/goto", &[OscType::Int(5)]);
        assert_eq!(cmd, OscCommand::Goto { slide: 5 });

        // Goto with no argument defaults to slide 1
        let cmd = OscCommand::from_message("/clicker/goto", &[]);
        assert_eq!(cmd, OscCommand::Goto { slide: 1 });
    }

    #[test]
    fn test_unknown_command() {
        // Unknown addresses should be captured with their original path
        let cmd = OscCommand::from_message("/some/random/path", &[]);
        match cmd {
            OscCommand::Unknown(addr) => assert_eq!(addr, "/some/random/path"),
            _ => panic!("Expected Unknown variant"),
        }
    }

    #[test]
    fn test_is_query() {
        // Only Zoom and Status are queries
        assert!(OscCommand::Zoom.is_query());
        assert!(OscCommand::Status.is_query());

        // Everything else triggers actions, not queries
        assert!(!OscCommand::Next.is_query());
        assert!(!OscCommand::Previous.is_query());
        assert!(!OscCommand::ZoomIn.is_query());
        assert!(!OscCommand::ZoomOut.is_query());
        assert!(!OscCommand::Refresh.is_query());
    }

    #[test]
    fn test_channel_command_parsing() {
        // Valid channel commands
        let result = OscCommand::from_channel_message("/clicker/main/next", &[]);
        assert!(result.is_some());
        let cmd = result.unwrap();
        assert_eq!(cmd.channel, "main");
        assert_eq!(cmd.command, OscCommand::Next);

        let result = OscCommand::from_channel_message("/clicker/backup/prev", &[]);
        assert!(result.is_some());
        let cmd = result.unwrap();
        assert_eq!(cmd.channel, "backup");
        assert_eq!(cmd.command, OscCommand::Previous);

        let result = OscCommand::from_channel_message("/clicker/keynote5/next", &[]);
        assert!(result.is_some());
        let cmd = result.unwrap();
        assert_eq!(cmd.channel, "keynote5");

        let result = OscCommand::from_channel_message("/clicker/aux3/status", &[]);
        assert!(result.is_some());
        let cmd = result.unwrap();
        assert_eq!(cmd.channel, "aux3");
        assert_eq!(cmd.command, OscCommand::Status);
    }

    #[test]
    fn test_channel_command_invalid() {
        // Invalid channel names should return None
        let result = OscCommand::from_channel_message("/clicker/invalid-channel/next", &[]);
        assert!(result.is_none());

        // Wrong prefix should return None
        let result = OscCommand::from_channel_message("/other/main/next", &[]);
        assert!(result.is_none());

        // Too short path should return None
        let result = OscCommand::from_channel_message("/clicker/main", &[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_matches_channel() {
        assert!(OscCommand::matches_channel("/clicker/main/next", "main"));
        assert!(OscCommand::matches_channel("/clicker/main/state/slide", "main"));
        assert!(!OscCommand::matches_channel("/clicker/main/next", "backup"));
        assert!(!OscCommand::matches_channel("/clicker/next", "main"));
    }
}
