//! Stub USB manager for non-Linux platforms.
//!
//! USB key binding currently works on **Linux only** — the real implementation
//! in `linux.rs` uses the kernel `evdev` subsystem. On Windows and macOS this
//! stub is compiled instead, so no devices are detected and nothing can be
//! bound. The rest of the app (config schema, bindings, coordinator, UI) is
//! platform-agnostic.

use crate::bridge::usb::{UsbDeviceInfo, UsbEvent};
use tokio::sync::broadcast;

pub struct UsbManagerImpl;

impl UsbManagerImpl {
    pub fn new(_event_tx: broadcast::Sender<UsbEvent>) -> std::io::Result<Self> {
        Ok(Self)
    }

    pub async fn devices(&self) -> Vec<UsbDeviceInfo> {
        Vec::new()
    }

    /// No evdev on non-Linux platforms, so there is never a permission problem.
    pub fn access_denied_count(&self) -> usize {
        0
    }
}
