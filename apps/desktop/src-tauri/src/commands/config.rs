use crate::config::{self, AppConfig};
use crate::state::AppState;
use tauri::{AppHandle, Manager};

#[tauri::command]
pub fn get_config(app: AppHandle) -> Result<AppConfig, String> {
    config::load_config(&app)
}

#[tauri::command]
pub fn save_config(app: AppHandle, config: AppConfig) -> Result<(), String> {
    config::save_config(&app, &config)?;

    // Re-sync the live overlay styling so any open overlay tab picks up the
    // change without an app restart.
    let state = app.state::<AppState>();
    state
        .caption_overlay
        .send_replace(crate::captions::OverlaySettings::from_config(&config.captions));

    // Start/stop/rename native outputs to match; unchanged ones are untouched.
    let sinks = state.caption_sinks();
    state
        .caption_outputs
        .lock()
        .unwrap()
        .reconcile(&app, &sinks, &config.captions.outputs);

    Ok(())
}
