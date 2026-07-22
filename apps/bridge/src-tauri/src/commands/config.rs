//! # Config commands

use crate::state::BridgeState;

/// Return the current bridge config.
#[tauri::command]
pub fn get_config(state: tauri::State<BridgeState>) -> Result<sherpresent_bridge_core::config::BridgeConfig, String> {
    Ok(state.core()?.config())
}

/// Save the bridge config.
#[tauri::command]
pub fn save_config(
    state: tauri::State<BridgeState>,
    config: sherpresent_bridge_core::config::BridgeConfig,
) -> Result<(), String> {
    state.core()?.save_config(&config)
}
