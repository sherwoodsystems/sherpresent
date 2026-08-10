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

    // Re-sync the live overlay font size so a running caption session and any
    // open overlay tab pick up the change without an app restart.
    let state = app.state::<AppState>();
    let _ = state.caption_font_size.send(config.captions.font_size);

    Ok(())
}
