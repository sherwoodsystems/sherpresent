use tauri::AppHandle;
use crate::adapters::canva::CanvaAdapter;
use crate::adapters::ConnectionStatus;
use crate::state::AppState;

#[tauri::command]
pub fn open_canva_remote(app: AppHandle, state: tauri::State<AppState>, url: String) -> Result<(), String> {
    let mut canva = state.canva_adapter.lock().unwrap();
    if canva.is_none() {
        *canva = Some(CanvaAdapter::new(app));
    }
    canva.as_ref().unwrap().open_remote(&url)
}

#[tauri::command]
pub fn close_canva_remote(state: tauri::State<AppState>) -> Result<(), String> {
    let canva = state.canva_adapter.lock().unwrap();
    if let Some(adapter) = canva.as_ref() {
        adapter.close_remote();
    }
    Ok(())
}

#[tauri::command]
pub fn log_from_webview(state: tauri::State<AppState>, category: String, message: String) {
    log::info!("[CANVA-WEBVIEW:{}] {}", category, message);
    let canva = state.canva_adapter.lock().unwrap();
    if let Some(adapter) = canva.as_ref() {
        adapter.handle_webview_log(&category, &message);
    }
}

#[tauri::command]
pub fn get_canva_connection_status(state: tauri::State<AppState>) -> ConnectionStatus {
    let canva = state.canva_adapter.lock().unwrap();
    match canva.as_ref() {
        Some(adapter) => {
            use crate::adapters::PresentationAdapter;
            adapter.connection_status()
        }
        None => ConnectionStatus::Disconnected,
    }
}
