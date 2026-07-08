//! # Discovery commands
//!
//! Frontend surface for the mDNS discovery service. The bulk of the work is
//! done by [`sherpresent_core::DiscoveryService`]; these commands just expose
//! its state to the Svelte UI and let the user rename the bridge.

use sherpresent_core::DiscoveredPeer;
use tauri::{AppHandle, Emitter};
use crate::state::BridgeState;

/// Current list of discovered peers (desktops and other bridges seen on the LAN).
#[tauri::command]
pub fn get_discovered_peers(state: tauri::State<BridgeState>) -> Vec<DiscoveredPeer> {
    let discovery = state.discovery_service.lock().unwrap();
    if let Some(service) = &*discovery {
        service.get_peers()
    } else {
        Vec::new()
    }
}

/// Update the human-readable bridge name and re-advertise via mDNS.
#[tauri::command]
pub fn set_instance_name(
    app: AppHandle,
    state: tauri::State<BridgeState>,
    name: Option<String>,
) -> Result<(), String> {
    // 1. Persist the new name into the bridge config.
    {
        let mut cfg = state.config.lock().unwrap();
        if let Some(ref n) = name {
            if n.trim().is_empty() {
                return Err("Bridge name cannot be empty".to_string());
            }
            cfg.bridge_name = n.clone();
        } else {
            cfg.refresh_default_name();
        }
        crate::config::save_config(&app, &cfg)?;
    }

    // 2. Tell the discovery service to re-advertise.
    {
        let mut discovery = state.discovery_service.lock().unwrap();
        if let Some(service) = discovery.as_mut() {
            service.update_display_name(name.clone())?;
            log::info!("Bridge display name updated to {:?}", name);
        } else {
            return Err("Discovery service not running".to_string());
        }
    }

    // 3. Nudge the frontend so any cached name in the UI refreshes.
    let _ = app.emit("instance-name-updated", name);

    Ok(())
}

/// Stop the mDNS daemon (mostly useful for debugging). The service is normally
/// auto-started once on app launch and runs until the bridge quits.
#[tauri::command]
pub fn stop_discovery(state: tauri::State<BridgeState>) -> Result<(), String> {
    let mut discovery = state.discovery_service.lock().unwrap();
    if let Some(mut service) = discovery.take() {
        service.shutdown()?;
        log::info!("Discovery service stopped");
    }
    Ok(())
}