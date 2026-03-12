//! # Discovery Module
//!
//! Handles mDNS-based service discovery for finding other sher-present instances
//! on the local network.
//!
//! ## How It Works
//!
//! 1. When channel sync is enabled, we register our instance as an mDNS service
//! 2. We browse for other instances with the same service type
//! 3. When we find instances in the same channel, we track them as peers
//! 4. Peers are notified via Tauri events so the UI can display them

pub mod mdns_service;

pub use mdns_service::{get_network_interfaces, DiscoveredPeer, DiscoveryService, NetworkInterface};
