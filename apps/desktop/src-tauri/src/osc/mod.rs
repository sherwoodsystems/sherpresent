//! # OSC (Open Sound Control) Server Module
//!
//! This module provides a native Rust OSC server for remote presentation control.
//! It receives commands over UDP and sends state feedback back to clients.
//!
//! ## What is OSC?
//!
//! OSC (Open Sound Control) is a protocol commonly used in audio/video production
//! for real-time communication between devices. It uses UDP for fast, low-latency
//! messaging. Tools like Bitfocus Companion, TouchOSC, and many others speak OSC.
//!
//! ## Architecture Overview
//!
//! ```text
//! ┌─────────────────┐     UDP:9000      ┌──────────────────┐
//! │  OSC Controller │ ───────────────▶  │    OscServer     │
//! │  (Companion)    │                   │                  │
//! │                 │  ◀───────────────  │  ┌────────────┐ │
//! └─────────────────┘     UDP:9001      │  │StateManager│ │
//!                      (feedback)       │  └────────────┘ │
//!                                       │        │        │
//!                                       └────────┼────────┘
//!                                                │
//!                                                ▼
//!                                       ┌────────────────┐
//!                                       │ PowerPoint/    │
//!                                       │ Keynote        │
//!                                       │ (via adapter)  │
//!                                       └────────────────┘
//! ```
//!
//! ## Module Structure
//!
//! - `messages` - OSC message type definitions (commands and feedback)
//! - `state_manager` - Cached presentation state with optimistic updates
//! - `server` - UDP server that ties everything together

// Declare submodules
pub mod latency;
pub mod messages;
pub mod server;
pub mod state_manager;

// Re-export main types for convenient access from lib.rs
// This means you can do `osc::OscServer` instead of `osc::server::OscServer`
pub use latency::{CommandSource, LatencyEvent, LatencyStore};
pub use server::{OscServer, OscServerHandle};
pub use state_manager::{CachedState, StateManager};

use std::net::SocketAddr;

/// Represents a device discovered via OSC command source.
///
/// When a device sends an OSC command (like `/clicker/main/next`),
/// we track its source address to show in the UI.
#[derive(Debug, Clone)]
pub struct CommandSourcePeer {
    /// Source address of the device
    pub address: SocketAddr,
    /// Channel the command was sent on
    pub channel: String,
}
