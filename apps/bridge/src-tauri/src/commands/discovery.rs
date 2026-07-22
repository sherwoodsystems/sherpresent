//! # Discovery commands
//!
//! Frontend surface for the mDNS discovery service.

use sherpresent_core::DiscoveredPeer;
use tauri::Emitter;

use crate::state::BridgeState;

/// Current list of discovered peers (desktops and other bridges seen on the LAN).
#[tauri::command]
pub fn get_discovered_peers(state: tauri::State<BridgeState>) -> Result<Vec<DiscoveredPeer>, String> {
    Ok(state.core()?.discovered_peers())
}

/// Update the human-readable bridge name and re-advertise via mDNS.
#[tauri::command]
pub fn set_instance_name(
    app: tauri::AppHandle,
    state: tauri::State<BridgeState>,
    name: Option<String>,
) -> Result<(), String> {
    let core = state.core()?;
    let new_name = name.unwrap_or_else(|| {
        let mut cfg = core.config();
        cfg.refresh_default_name();
        cfg.bridge_name
    });
    core.set_bridge_name(new_name.clone())?;
    let _ = app.emit("instance-name-updated", new_name);
    Ok(())
}

/// Stop the mDNS daemon (mostly useful for debugging).
#[tauri::command]
pub fn stop_discovery(state: tauri::State<BridgeState>) -> Result<(), String> {
    state.core()?.stop_discovery()
}
