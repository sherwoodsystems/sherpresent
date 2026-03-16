use crate::osc::LatencyEvent;
use crate::state::AppState;

#[tauri::command]
pub fn get_latency_events(state: tauri::State<AppState>) -> Vec<LatencyEvent> {
    state.latency_store.get_all()
}

#[tauri::command]
pub fn clear_latency_events(state: tauri::State<AppState>) {
    state.latency_store.clear();
}
