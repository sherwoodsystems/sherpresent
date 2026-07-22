//! USB HID clicker detection and event routing.
//!
//! This module wraps the platform-specific implementation. On Linux it uses the
//! kernel `evdev` subsystem to enumerate USB input devices (any key-capable
//! device — clickers, keyboards, …), read key events passively, and emit
//! key-up events.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub use linux::UsbManagerImpl;

#[cfg(not(target_os = "linux"))]
mod stub;

#[cfg(not(target_os = "linux"))]
pub use stub::UsbManagerImpl;

/// Static information about a discovered USB HID clicker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbDeviceInfo {
    /// Stable identifier derived from the USB physical path.
    pub id: String,
    /// Human-readable device name from the kernel.
    pub name: String,
    /// /dev/input/event* path.
    pub path: String,
    /// USB vendor ID.
    pub vendor_id: u16,
    /// USB product ID.
    pub product_id: u16,
    /// Physical location reported by the kernel.
    pub phys: String,
    /// True if this looks like a DSan Perfect Cue.
    pub is_perfect_cue: bool,
}

/// Event emitted by the USB manager.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UsbEvent {
    /// A new clicker device was detected.
    Connected(UsbDeviceInfo),
    /// A previously detected device is no longer present.
    Disconnected { device_id: String },
    /// A key was released on a clicker.
    KeyUp { device_id: String, key: String },
    /// One or more USB key-capable input devices exist but could not be opened
    /// because the app lacks permission to read `/dev/input/event*`. `count` is
    /// the number of inaccessible USB devices (0 = access is fine).
    AccessDenied { count: usize },
}

/// Shared USB manager state.
///
/// The manager owns a background task that continuously scans `/dev/input` for
/// compatible clickers and a task per open device that reads key events. It
/// sends events through a broadcast channel that the rest of the app can
/// subscribe to.
#[derive(Clone)]
pub struct UsbManager {
    /// Platform-specific implementation.
    inner: Arc<UsbManagerImpl>,
    /// Broadcast channel for USB events.
    events: broadcast::Sender<UsbEvent>,
}

impl UsbManager {
    /// Spawn the USB device scanner and event readers.
    pub fn new() -> std::io::Result<Self> {
        let (tx, _rx) = broadcast::channel::<UsbEvent>(256);
        let inner = Arc::new(UsbManagerImpl::new(tx.clone())?);
        Ok(Self { inner, events: tx })
    }

    /// Subscribe to USB events.
    pub fn subscribe(&self) -> broadcast::Receiver<UsbEvent> {
        self.events.subscribe()
    }

    /// Currently connected clicker devices.
    pub async fn devices(&self) -> Vec<UsbDeviceInfo> {
        self.inner.devices().await
    }

    /// Number of USB key-capable input devices that exist but can't be opened
    /// due to missing permissions on `/dev/input/event*`. 0 means access is OK.
    pub fn access_denied_count(&self) -> usize {
        self.inner.access_denied_count()
    }
}

/// True if the device name looks like a DSan Perfect Cue.
pub fn is_perfect_cue(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("perfect cue") || lower.contains("dsan")
}
