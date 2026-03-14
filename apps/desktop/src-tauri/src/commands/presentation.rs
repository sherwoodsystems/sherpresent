use std::collections::HashMap;
use crate::adapters::{
    get_adapter, get_available_adapters, LiveStatus, PresentationAdapter, PresentationState,
    SlideInfo,
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
    if adapter == "canva" {
        let canva = state.canva_adapter.lock().unwrap();
        return canva.as_ref()
            .ok_or("Canva adapter not initialized".to_string())?
            .get_open_presentations();
    }
    let config = get_adapter_config(&state);
    let adapter_impl =
        get_adapter(&adapter, &config).ok_or_else(|| format!("Unknown adapter: {}", adapter))?;
    adapter_impl.get_open_presentations()
}

#[tauri::command]
pub fn get_presentation_state(adapter: String, name: String, state: tauri::State<AppState>) -> Result<PresentationState, String> {
    if adapter == "canva" {
        let canva = state.canva_adapter.lock().unwrap();
        return canva.as_ref()
            .ok_or("Canva adapter not initialized".to_string())?
            .get_presentation_state(&name);
    }
    let config = get_adapter_config(&state);
    let adapter_impl =
        get_adapter(&adapter, &config).ok_or_else(|| format!("Unknown adapter: {}", adapter))?;
    adapter_impl.get_presentation_state(&name)
}

#[tauri::command]
pub fn get_slide_info(adapter: String, name: String, state: tauri::State<AppState>) -> Result<SlideInfo, String> {
    if adapter == "canva" {
        let canva = state.canva_adapter.lock().unwrap();
        return canva.as_ref()
            .ok_or("Canva adapter not initialized".to_string())?
            .get_slide_info(&name);
    }
    let config = get_adapter_config(&state);
    let adapter_impl =
        get_adapter(&adapter, &config).ok_or_else(|| format!("Unknown adapter: {}", adapter))?;
    adapter_impl.get_slide_info(&name)
}

#[tauri::command]
pub fn get_live_status(adapter: String, name: String, state: tauri::State<AppState>) -> LiveStatus {
    if adapter == "canva" {
        let canva = state.canva_adapter.lock().unwrap();
        return match canva.as_ref() {
            Some(a) => a.get_live_status(&name),
            None => LiveStatus::default(),
        };
    }
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
    if adapter == "canva" {
        let canva = state.canva_adapter.lock().unwrap();
        return canva.as_ref()
            .ok_or("Canva adapter not initialized".to_string())?
            .next_slide(&name);
    }
    let config = get_adapter_config(&state);
    let adapter_impl =
        get_adapter(&adapter, &config).ok_or_else(|| format!("Unknown adapter: {}", adapter))?;
    adapter_impl.next_slide(&name)
}

#[tauri::command]
pub fn prev_slide(adapter: String, name: String, state: tauri::State<AppState>) -> Result<SlideInfo, String> {
    if adapter == "canva" {
        let canva = state.canva_adapter.lock().unwrap();
        return canva.as_ref()
            .ok_or("Canva adapter not initialized".to_string())?
            .prev_slide(&name);
    }
    let config = get_adapter_config(&state);
    let adapter_impl =
        get_adapter(&adapter, &config).ok_or_else(|| format!("Unknown adapter: {}", adapter))?;
    adapter_impl.prev_slide(&name)
}

#[tauri::command]
pub fn goto_slide(adapter: String, name: String, slide: i32, state: tauri::State<AppState>) -> Result<SlideInfo, String> {
    if adapter == "canva" {
        let canva = state.canva_adapter.lock().unwrap();
        return canva.as_ref()
            .ok_or("Canva adapter not initialized".to_string())?
            .goto_slide(&name, slide);
    }
    let config = get_adapter_config(&state);
    let adapter_impl =
        get_adapter(&adapter, &config).ok_or_else(|| format!("Unknown adapter: {}", adapter))?;
    adapter_impl.goto_slide(&name, slide)
}

#[tauri::command]
pub fn fetch_all_notes(adapter: String, name: String, state: tauri::State<AppState>) -> Result<HashMap<i32, String>, String> {
    let bulk_notes = if adapter == "canva" {
        // Canva doesn't support bulk fetch — return whatever is cached
        HashMap::new()
    } else {
        let config = get_adapter_config(&state);
        let adapter_impl =
            get_adapter(&adapter, &config).ok_or_else(|| format!("Unknown adapter: {}", adapter))?;
        adapter_impl.get_all_presenter_notes(&name)?
    };

    let mut cache = state.notes_cache.lock().unwrap();
    // Merge bulk results into cache (bulk results take precedence)
    for (k, v) in bulk_notes {
        cache.insert(k, v);
    }
    Ok(cache.clone())
}

#[tauri::command]
pub fn get_all_notes(state: tauri::State<AppState>) -> HashMap<i32, String> {
    state.notes_cache.lock().unwrap().clone()
}

#[tauri::command]
pub fn clear_notes_cache(state: tauri::State<AppState>) {
    state.notes_cache.lock().unwrap().clear();
}
