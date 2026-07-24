//! # Config commands

use crate::state::BridgeState;

/// Return the current bridge config.
#[tauri::command]
pub fn get_config(state: tauri::State<BridgeState>) -> Result<crate::bridge::config::BridgeConfig, String> {
    Ok(state.core()?.config())
}

/// Save the bridge config.
#[tauri::command]
pub fn save_config(
    state: tauri::State<BridgeState>,
    config: crate::bridge::config::BridgeConfig,
) -> Result<(), String> {
    state.core()?.save_config(&config)
}
