//! # sherpresent-core
//!
//! Shared types and services for SherPresent apps (desktop + bridge).
//!
//! Currently exposes:
//! - [`discovery`] — mDNS-based service registration and peer browsing
//!
//! Future homes:
//! - OSC message types (`/oscpoint/*` schema)
//! - Network interface enumeration
//! - Common config types
//!
//! Both the desktop Tauri app and the bridge Tauri app consume this crate so
//! they speak the same mDNS TXT record format and the same peer types.

pub mod discovery;

pub use discovery::{get_local_ip, get_network_interfaces, DiscoveredPeer, DiscoveryService, NetworkInterface};