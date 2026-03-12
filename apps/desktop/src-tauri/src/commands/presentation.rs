use crate::adapters::{
    get_adapter, get_available_adapters, LiveStatus, PresentationState, SlideInfo,
};
use crate::config::AdapterConfig;
use crate::state::AppState;

#[tauri::command]
pub fn get_adapters() -> Vec<(String, String)> {
    get_available_adapters()
        .into_iter()
        .map(|(id, name)| (id.to_string(), name.to_string()))
        .collect()
}

/// Helper to get the adapter config from AppState
fn get_adapter_config(state: &AppState) -> AdapterConfig {
    state.adapter_config.lock().unwrap().clone()
}

#[tauri::command]
pub fn get_open_presentations(adapter: String, state: tauri::State<AppState>) -> Result<Vec<String>, String> {
    let config = get_adapter_config(&state);
    let adapter_impl =
        get_adapter(&adapter, &config).ok_or_else(|| format!("Unknown adapter: {}", adapter))?;
    adapter_impl.get_open_presentations()
}

#[tauri::command]
pub fn get_presentation_state(adapter: String, name: String, state: tauri::State<AppState>) -> Result<PresentationState, String> {
    let config = get_adapter_config(&state);
    let adapter_impl =
        get_adapter(&adapter, &config).ok_or_else(|| format!("Unknown adapter: {}", adapter))?;
    adapter_impl.get_presentation_state(&name)
}

#[tauri::command]
pub fn get_slide_info(adapter: String, name: String, state: tauri::State<AppState>) -> Result<SlideInfo, String> {
    let config = get_adapter_config(&state);
    let adapter_impl =
        get_adapter(&adapter, &config).ok_or_else(|| format!("Unknown adapter: {}", adapter))?;
    adapter_impl.get_slide_info(&name)
}

#[tauri::command]
pub fn get_live_status(adapter: String, name: String, state: tauri::State<AppState>) -> LiveStatus {
    let config = get_adapter_config(&state);
    match get_adapter(&adapter, &config) {
        Some(adapter_impl) => adapter_impl.get_live_status(&name),
        None => LiveStatus::default(),
    }
}

#[tauri::command]
pub fn get_notes_zoom() -> Result<Option<i32>, String> {
    // Notes zoom only works for PowerPoint
    #[cfg(target_os = "macos")]
    {
        use crate::adapters::PresentationAdapter;
        let adapter = crate::adapters::powerpoint::PowerPointAdapter;
        adapter.get_notes_zoom()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(None)
    }
}

#[tauri::command]
pub fn next_slide(adapter: String, name: String, state: tauri::State<AppState>) -> Result<SlideInfo, String> {
    let config = get_adapter_config(&state);
    let adapter_impl =
        get_adapter(&adapter, &config).ok_or_else(|| format!("Unknown adapter: {}", adapter))?;
    adapter_impl.next_slide(&name)
}

#[tauri::command]
pub fn prev_slide(adapter: String, name: String, state: tauri::State<AppState>) -> Result<SlideInfo, String> {
    let config = get_adapter_config(&state);
    let adapter_impl =
        get_adapter(&adapter, &config).ok_or_else(|| format!("Unknown adapter: {}", adapter))?;
    adapter_impl.prev_slide(&name)
}
