//! # USB clicker commands
//!
//! Device enumeration and registration-mode controls for the bridge frontend.

use serde::Serialize;
use tauri::State;

use crate::state::BridgeState;

/// Permission status for reading USB input devices on Linux.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsbPermissionStatus {
    /// True if one or more USB input devices exist but can't be read.
    pub access_denied: bool,
    /// Number of inaccessible USB input devices.
    pub count: usize,
    /// Human-readable fix instructions.
    pub fix_command: String,
}

/// List all currently connected USB clicker devices.
#[tauri::command]
pub async fn get_usb_devices(
    state: State<'_, BridgeState>,
) -> Result<Vec<crate::bridge::usb::UsbDeviceInfo>, String> {
    Ok(state.core()?.usb_devices().await)
}

/// Report whether the app can read USB input devices.
#[tauri::command]
pub async fn get_usb_permission_status(
    state: State<'_, BridgeState>,
) -> Result<UsbPermissionStatus, String> {
    let core = state.core()?;
    let count = core.usb_access_denied_count();
    Ok(UsbPermissionStatus {
        access_denied: count > 0,
        count,
        fix_command: "sudo usermod -aG input $USER".to_string(),
    })
}

/// Enter registration mode for a presentation action (`"next"` or `"prev"`).
#[tauri::command]
pub async fn start_usb_registration(
    state: State<'_, BridgeState>,
    action: String,
) -> Result<(), String> {
    state.core()?.start_usb_registration(action).await;
    Ok(())
}

/// Cancel registration mode.
#[tauri::command]
pub async fn cancel_usb_registration(state: State<'_, BridgeState>) -> Result<(), String> {
    state.core()?.cancel_usb_registration().await;
    Ok(())
}

/// Bind a key on a device to an action and persist the config.
#[tauri::command]
pub async fn confirm_usb_binding(
    state: State<'_, BridgeState>,
    device_id: String,
    device_name: String,
    key: String,
    action: String,
) -> Result<(), String> {
    state
        .core()?
        .confirm_usb_binding(device_id, device_name, key, action)
        .await
}

/// Remove a single key binding from a device.
#[tauri::command]
pub async fn remove_usb_binding(
    state: State<'_, BridgeState>,
    device_id: String,
    key: String,
) -> Result<(), String> {
    state.core()?.remove_usb_binding(device_id, key).await
}

/// Assign (or clear) the OSC target for a registered device slot.
#[tauri::command]
pub async fn set_device_target(
    state: State<'_, BridgeState>,
    device_id: String,
    host: Option<String>,
    port: Option<u16>,
    name: Option<String>,
    instance_id: Option<String>,
) -> Result<(), String> {
    let target = match (host, port) {
        (Some(h), Some(p)) => Some(crate::bridge::config::DeviceTarget {
            host: h,
            port: p,
            name,
            instance_id,
        }),
        _ => None,
    };
    state.core()?.set_device_target(device_id, target).await
}
