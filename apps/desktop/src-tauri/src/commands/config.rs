use crate::config::{self, AppConfig};
use crate::qlab;
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

    // Follow QLab at the configured host (no-op if unchanged).
    state.qlab.reconcile(&config.qlab);

    // Move the OSC server if its settings changed (no-op if unchanged).
    let osc_app = app.clone();
    let osc_config = config.clone();
    tauri::async_runtime::spawn(async move {
        let state = osc_app.state::<AppState>();
        if let Err(e) = state.reconcile_osc(&osc_config).await {
            log::error!("{}", e);
        }
    });

    state
        .state_manager
        .set_fast_slide_poll(qlab::fires_on_slides(&config.qlab));

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

/// The workspaces open in QLab on this network, for the tab's list.
#[tauri::command]
pub async fn find_qlab_workspaces() -> Result<Vec<qlab::scan::FoundWorkspace>, String> {
    qlab::scan::scan().await
}

/// The QLab count-out as it stands. Changes are also pushed as `qlab-state`.
#[tauri::command]
pub fn get_qlab_state(state: tauri::State<'_, AppState>) -> crate::qlab::QLabState {
    state.qlab.current()
}
