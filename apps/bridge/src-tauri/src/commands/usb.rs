//! # USB clicker commands
//!
//! Device enumeration and registration-mode controls for the bridge frontend.

use tauri::{AppHandle, State};

use crate::config::{DeviceConfig, DeviceTarget};
use crate::state::BridgeState;
use crate::usb::UsbDeviceInfo;

/// List all currently connected USB clicker devices.
#[tauri::command]
pub async fn get_usb_devices(state: State<'_, BridgeState>) -> Result<Vec<UsbDeviceInfo>, String> {
    let manager = state.usb_manager.lock().map_err(|e| e.to_string())?.clone();
    match manager {
        Some(manager) => Ok(manager.devices().await),
        None => Ok(Vec::new()),
    }
}

/// Enter registration mode for a device slot. The next key-up from an
/// unregistered clicker will emit a `usb-registration-detected` event.
#[tauri::command]
pub async fn start_usb_registration(
    state: State<'_, BridgeState>,
    slot: String,
) -> Result<(), String> {
    *state.usb_registration_mode.lock().await = Some(slot);
    Ok(())
}

/// Cancel registration mode.
#[tauri::command]
pub async fn cancel_usb_registration(state: State<'_, BridgeState>) -> Result<(), String> {
    *state.usb_registration_mode.lock().await = None;
    Ok(())
}

/// Confirm a detected device into a slot and persist the config.
#[tauri::command]
pub async fn confirm_usb_registration(
    app: AppHandle,
    state: State<'_, BridgeState>,
    _slot: String,
    device_id: String,
    device_name: String,
) -> Result<(), String> {
    // Clear registration mode first.
    *state.usb_registration_mode.lock().await = None;

    let config = {
        let mut cfg = state.config.lock().map_err(|e| e.to_string())?;
        cfg.devices.insert(
            device_id.clone(),
            Some(DeviceConfig {
                label: device_name,
                usb_phys: device_id,
                target: None,
            }),
        );
        cfg.clone()
    };

    crate::config::save_config(&app, &config)?;
    *state.config.lock().map_err(|e| e.to_string())? = config;
    Ok(())
}

/// Assign (or clear) the OSC target for a registered device slot.
#[tauri::command]
pub async fn set_device_target(
    app: AppHandle,
    state: State<'_, BridgeState>,
    device_id: String,
    host: Option<String>,
    port: Option<u16>,
    name: Option<String>,
    instance_id: Option<String>,
) -> Result<(), String> {
    let config = {
        let mut cfg = state.config.lock().map_err(|e| e.to_string())?;
        let entry = cfg.devices.get_mut(&device_id).ok_or("device not found")?;
        let device = entry.as_mut().ok_or("device slot is empty")?;
        device.target = match (host, port) {
            (Some(h), Some(p)) => Some(DeviceTarget {
                host: h,
                port: p,
                name,
                instance_id,
            }),
            _ => None,
        };
        cfg.clone()
    };

    crate::config::save_config(&app, &config)?;
    *state.config.lock().map_err(|e| e.to_string())? = config;
    Ok(())
}
