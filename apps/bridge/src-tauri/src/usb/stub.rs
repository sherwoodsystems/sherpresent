//! Stub USB manager for non-Linux platforms.
//!
//! # Windows / macOS support (future)
//!
//! USB key binding currently works on **Linux only** — the real implementation
//! in `linux.rs` uses the kernel `evdev` subsystem to enumerate input devices,
//! read key events, and `grab()` a device for exclusive capture. On Windows and
//! macOS this stub is compiled instead, so no devices are detected and nothing
//! can be bound. The rest of the app (config schema, bindings, coordinator, UI)
//! is platform-agnostic, so bringing another platform online is "just" writing a
//! new `UsbManagerImpl` that produces the same `UsbEvent`s. Notes for whoever
//! does that:
//!
//! - **macOS**: use IOKit HID (`IOHIDManager`) to enumerate/read devices. There
//!   is no direct `evdev`-style grab; "exclusive capture" needs a `CGEventTap`
//!   (which can swallow the event system-wide) and the app must be granted
//!   *Input Monitoring* / *Accessibility* permission. Emit `KeyUp` with an
//!   evdev-compatible key name (e.g. `"KEY_RIGHT"`) so existing bindings match.
//! - **Windows**: use Raw Input (`WM_INPUT`) or the HID API to read per-device
//!   input. Windows has no true per-device exclusive grab for keyboards; a
//!   low-level keyboard hook (`WH_KEYBOARD_LL`) can suppress keys globally but
//!   can't scope to one device, so the "exclusive for registered only" model
//!   maps imperfectly — decide whether to suppress globally or run passive.
//! - Keep the on-disk key names normalized to the evdev vocabulary so a config
//!   registered on one platform stays meaningful on another.

use crate::config::BridgeConfig;
use crate::usb::{UsbDeviceInfo, UsbEvent};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

pub struct UsbManagerImpl;

impl UsbManagerImpl {
    pub fn new(
        _event_tx: broadcast::Sender<UsbEvent>,
        _config: Arc<Mutex<BridgeConfig>>,
    ) -> std::io::Result<Self> {
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
