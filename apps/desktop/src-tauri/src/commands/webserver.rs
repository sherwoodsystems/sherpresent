use crate::config;
use crate::state::AppState;

#[tauri::command]
pub async fn start_web_server(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    // Check if already running
    {
        let handle = state.web_server_handle.lock().unwrap();
        if handle.is_some() {
            return Err("Web server is already running".to_string());
        }
    }

    let cfg = config::load_config(&app)?;
    if !cfg.web_server.enabled {
        return Err("Web server is not enabled in config".to_string());
    }

    let notes_cache = state.notes_cache.clone();
    let status_broadcast = state.status_broadcast.clone();
    let notes_broadcast = state.notes_broadcast.clone();
    let port = cfg.web_server.port;

    let join_handle = crate::webserver::start(
        cfg.web_server,
        notes_cache,
        status_broadcast,
        notes_broadcast,
    )
    .await?;

    {
        let mut handle = state.web_server_handle.lock().unwrap();
        *handle = Some(join_handle);
    }

    // Get the local IP for the URL
    let local_ip = crate::commands::network::get_local_ip_internal();
    Ok(format!("http://{}:{}", local_ip, port))
}

#[tauri::command]
pub fn stop_web_server(state: tauri::State<AppState>) -> Result<(), String> {
    let mut handle = state.web_server_handle.lock().unwrap();
    if let Some(h) = handle.take() {
        h.abort();
        log::info!("Web server stopped");
        Ok(())
    } else {
        Err("Web server is not running".to_string())
    }
}

#[tauri::command]
pub fn is_web_server_running(state: tauri::State<AppState>) -> bool {
    let handle = state.web_server_handle.lock().unwrap();
    handle.is_some()
}

#[tauri::command]
pub fn get_web_server_url(
    app: tauri::AppHandle,
) -> Result<String, String> {
    let cfg = config::load_config(&app)?;
    let local_ip = crate::commands::network::get_local_ip_internal();
    Ok(format!("http://{}:{}", local_ip, cfg.web_server.port))
}
