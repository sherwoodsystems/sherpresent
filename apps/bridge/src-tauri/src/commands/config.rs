//! # Config commands

use tauri::AppHandle;
use crate::config::BridgeConfig;
use crate::state::BridgeState;

/// Return the current bridge config (bridges load + persist on first run).
#[tauri::command]
pub fn get_config(state: tauri::State<BridgeState>) -> BridgeConfig {
    state.config.lock().unwrap().clone()
}

/// Save the bridge config and update the in-memory state.
#[tauri::command]
pub fn save_config(
    app: AppHandle,
    state: tauri::State<BridgeState>,
    config: BridgeConfig,
) -> Result<(), String> {
    crate::config::save_config(&app, &config)?;
    let mut slot = state.config.lock().unwrap();
    *slot = config;
    Ok(())
}