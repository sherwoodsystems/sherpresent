use crate::config;
use crate::state::AppState;

#[tauri::command]
pub async fn start_web_server(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let cfg = config::load_config(&app)?;
    let port = cfg.web_server.port;
    state.start_web_server(cfg.web_server).await?;
    Ok(crate::commands::network::lan_url(port, ""))
}

#[tauri::command]
pub fn is_web_server_running(state: tauri::State<AppState>) -> bool {
    let handle = state.web_server_handle.lock().unwrap();
    handle.is_some()
}

#[tauri::command]
pub fn get_web_server_url(app: tauri::AppHandle) -> Result<String, String> {
    let cfg = config::load_config(&app)?;
    Ok(crate::commands::network::lan_url(cfg.web_server.port, ""))
}
