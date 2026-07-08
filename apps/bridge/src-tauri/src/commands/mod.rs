//! # Tauri commands exposed to the bridge frontend.
//!
//! Grouped by feature to mirror the desktop app's `commands/` layout:
//! - [`discovery`] — mDNS peer list, instance name updates
//! - [`config`] — bridge config get/save
//! - [`app`] — bridge self-info (id, name, ports, LAN IP)
//! - [`feedback`] — cached desktop feedback state + OSC test send

pub mod discovery;
pub mod config;
pub mod app;
pub mod feedback;
pub mod usb;