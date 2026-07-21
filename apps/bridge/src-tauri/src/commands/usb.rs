//! # USB clicker commands
//!
//! Device enumeration and registration-mode controls for the bridge frontend.

use std::collections::BTreeMap;

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::config::{DeviceConfig, DeviceTarget, KeyAction};
use crate::state::BridgeState;
use crate::usb::UsbDeviceInfo;

/// Permission status for reading USB input devices on Linux.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsbPermissionStatus {
    /// True if one or more USB input devices exist but can't be read.
    pub access_denied: bool,
    /// Number of inaccessible USB input devices.
    pub count: usize,
}

/// List all currently connected USB clicker devices.
#[tauri::command]
pub async fn get_usb_devices(state: State<'_, BridgeState>) -> Result<Vec<UsbDeviceInfo>, String> {
    let manager = state.usb_manager.lock().map_err(|e| e.to_string())?.clone();
    match manager {
        Some(manager) => Ok(manager.devices().await),
        None => Ok(Vec::new()),
    }
}

/// Report whether the app can read USB input devices, so the UI can surface a
/// "fix permissions" banner on load rather than silently showing nothing.
#[tauri::command]
pub async fn get_usb_permission_status(
    state: State<'_, BridgeState>,
) -> Result<UsbPermissionStatus, String> {
    let manager = state.usb_manager.lock().map_err(|e| e.to_string())?.clone();
    let count = manager.map(|m| m.access_denied_count()).unwrap_or(0);
    Ok(UsbPermissionStatus {
        access_denied: count > 0,
        count,
    })
}

/// Install the bundled udev rule that grants the logged-in session ACL access
/// to USB input devices (`uaccess`), via a one-time privileged `pkexec` call.
/// After this succeeds the user must unplug/replug the clicker (or re-login)
/// for the ACL to apply. Linux-only.
#[tauri::command]
pub async fn install_udev_rules(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use tauri::Manager;

        let src = app
            .path()
            .resource_dir()
            .map_err(|e| format!("could not resolve resource dir: {e}"))?
            .join("resources/70-sherpresent-clicker.rules");
        if !src.exists() {
            return Err(format!("bundled udev rule not found at {}", src.display()));
        }
        let src = src.to_string_lossy().to_string();

        let script = format!(
            "install -m0644 '{src}' /etc/udev/rules.d/70-sherpresent-clicker.rules \
             && udevadm control --reload && udevadm trigger"
        );

        let status = std::process::Command::new("pkexec")
            .arg("sh")
            .arg("-c")
            .arg(&script)
            .status()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    "pkexec not found — install polkit, or run the manual command shown below".to_string()
                } else {
                    format!("failed to launch pkexec: {e}")
                }
            })?;

        if !status.success() {
            // pkexec exits 126 when the user dismisses/authentication fails.
            return Err(
                "permission install was cancelled or failed — try the manual command below"
                    .to_string(),
            );
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = app;
        Err("udev rules are only used on Linux".to_string())
    }
}

/// Enter registration mode for a presentation action (`"next"` or `"prev"`).
/// The next key-up from any device will emit a `usb-registration-detected`
/// event carrying the device and key to bind.
#[tauri::command]
pub async fn start_usb_registration(
    state: State<'_, BridgeState>,
    action: String,
) -> Result<(), String> {
    *state.usb_registration_mode.lock().await = Some(action);
    Ok(())
}

/// Cancel registration mode.
#[tauri::command]
pub async fn cancel_usb_registration(state: State<'_, BridgeState>) -> Result<(), String> {
    *state.usb_registration_mode.lock().await = None;
    Ok(())
}

/// Bind a key on a device to an action and persist the config. Creates the
/// device entry if it doesn't exist yet, preserving any existing target and
/// bindings. Once a device has ≥1 binding it will be grabbed exclusively.
#[tauri::command]
pub async fn confirm_usb_binding(
    app: AppHandle,
    state: State<'_, BridgeState>,
    device_id: String,
    device_name: String,
    key: String,
    action: String,
) -> Result<(), String> {
    let action: KeyAction = match action.as_str() {
        "next" => KeyAction::Next,
        "prev" => KeyAction::Prev,
        other => return Err(format!("unknown action: {other}")),
    };

    // Clear registration mode first.
    *state.usb_registration_mode.lock().await = None;

    let config = {
        let mut cfg = state.config.lock().map_err(|e| e.to_string())?;
        let entry = cfg
            .devices
            .entry(device_id.clone())
            .or_insert_with(|| {
                Some(DeviceConfig {
                    label: device_name.clone(),
                    usb_phys: device_id.clone(),
                    target: None,
                    bindings: BTreeMap::new(),
                })
            });
        // Slot may have been present-but-empty (`None`); materialize it.
        let device = entry.get_or_insert_with(|| DeviceConfig {
            label: device_name.clone(),
            usb_phys: device_id.clone(),
            target: None,
            bindings: BTreeMap::new(),
        });
        device.bindings.insert(key, action);
        cfg.clone()
    };

    crate::config::save_config(&app, &config)?;
    *state.config.lock().map_err(|e| e.to_string())? = config;
    Ok(())
}

/// Remove a single key binding from a device. If the device has no bindings
/// left afterwards, the whole device entry is removed (releasing its grab).
#[tauri::command]
pub async fn remove_usb_binding(
    app: AppHandle,
    state: State<'_, BridgeState>,
    device_id: String,
    key: String,
) -> Result<(), String> {
    let config = {
        let mut cfg = state.config.lock().map_err(|e| e.to_string())?;
        if let Some(Some(device)) = cfg.devices.get_mut(&device_id) {
            device.bindings.remove(&key);
            if device.bindings.is_empty() {
                cfg.devices.remove(&device_id);
            }
        }
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
