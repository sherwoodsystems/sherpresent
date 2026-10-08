use crate::config::{self, AppConfig};
use crate::state::AppState;
use crate::util::LockExt;
use tauri::{AppHandle, Manager};

#[tauri::command]
pub fn get_config(app: AppHandle) -> Result<AppConfig, String> {
    config::load_config(&app)
}

#[tauri::command]
pub fn save_config(app: AppHandle, config: AppConfig) -> Result<(), String> {
    config::save_config(&app, &config)?;

    // Re-sync the live overlay styling so any open overlay tab picks up the
    // change without an app restart. Unrelated saves (OSC, web server...)
    // push nothing.
    let state = app.state::<AppState>();
    state.captions.set_overlay(&config.captions);

    // Point the StateManager at the selected presentation (no-op if unchanged).
    state.state_manager.set_target(
        config.adapter.clone(),
        config.presentation_name.clone(),
        config.adapter_config.clone(),
    );

    // Start/stop/rename native outputs to match; unchanged ones are untouched.
    let sources = state.output_sources();
    state.outputs.locked().reconcile(&app, &sources, &config);

    Ok(())
}

/// Status of the native video outputs (Syphon captions and notes), for
/// Settings. Changes are also pushed as `outputs-status` events.
#[tauri::command]
pub fn get_outputs_status(state: tauri::State<'_, AppState>) -> crate::output::OutputsStatus {
    state.outputs.locked().status()
}
