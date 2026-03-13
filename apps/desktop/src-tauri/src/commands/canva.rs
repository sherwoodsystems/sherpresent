use tauri::{AppHandle, Emitter};
use crate::adapters::canva::CanvaAdapter;
use crate::adapters::{ConnectionStatus, PresentationAdapter};
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
pub fn update_canva_state(
    app: AppHandle,
    state: tauri::State<AppState>,
    current_page: i32,
    total_pages: i32,
    notes: Option<String>,
) {
    let canva = state.canva_adapter.lock().unwrap();
    if let Some(adapter) = canva.as_ref() {
        adapter.update_state(current_page, total_pages, notes);
        let live_status = adapter.get_live_status("Canva Presentation");
        let _ = app.emit("presentation-status", &live_status);
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
