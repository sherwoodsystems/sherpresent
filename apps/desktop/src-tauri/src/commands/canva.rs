use crate::adapters::canva::CanvaAdapter;
use crate::adapters::{ConnectionStatus, PresentationAdapter};
use crate::state::AppState;
use crate::util::LockExt;
use tauri::AppHandle;

#[tauri::command]
pub fn open_canva_remote(
    app: AppHandle,
    state: tauri::State<AppState>,
    url: String,
) -> Result<(), String> {
    let mut canva = state.canva_adapter.locked();
    if canva.is_none() {
        *canva = Some(CanvaAdapter::new(app));
    }
    canva.as_ref().unwrap().open_remote(&url)
}

#[tauri::command]
pub fn close_canva_remote(state: tauri::State<AppState>) -> Result<(), String> {
    let canva = state.canva_adapter.locked();
    if let Some(adapter) = canva.as_ref() {
        adapter.close_remote();
    }
    Ok(())
}

#[tauri::command]
pub fn log_from_webview(state: tauri::State<AppState>, category: String, message: String) {
    log::info!("[CANVA-WEBVIEW:{}] {}", category, message);
    let canva = state.canva_adapter.locked();
    if let Some(adapter) = canva.as_ref() {
        adapter.handle_webview_log(&category, &message);
    }
}

#[tauri::command]
pub fn update_canva_state(
    state: tauri::State<AppState>,
    current_page: i32,
    total_pages: i32,
    notes: Option<String>,
) {
    let live_status = {
        let canva = state.canva_adapter.locked();
        let Some(adapter) = canva.as_ref() else {
            return;
        };
        adapter.update_state(current_page, total_pages, notes);
        adapter.get_live_status("Canva Presentation")
    };
    state.state_manager.apply_status(live_status);
}

#[tauri::command]
pub fn update_canva_batch_notes(state: tauri::State<AppState>, notes_batch: Vec<(i32, String)>) {
    // 0-indexed pages → 1-indexed slides
    state.state_manager.add_notes(
        notes_batch
            .into_iter()
            .filter(|(page, text)| *page >= 0 && !text.is_empty())
            .map(|(page, text)| (page + 1, text)),
    );
}

#[tauri::command]
pub fn get_canva_connection_status(state: tauri::State<AppState>) -> ConnectionStatus {
    let canva = state.canva_adapter.locked();
    match canva.as_ref() {
        Some(adapter) => {
            use crate::adapters::PresentationAdapter;
            adapter.connection_status()
        }
        None => ConnectionStatus::Disconnected,
    }
}
