use crate::adapters::{
    get_adapter, get_available_adapters, LiveStatus, PresentationState, SlideInfo,
};

#[tauri::command]
pub fn get_adapters() -> Vec<(String, String)> {
    get_available_adapters()
        .into_iter()
        .map(|(id, name)| (id.to_string(), name.to_string()))
        .collect()
}

#[tauri::command]
pub fn get_open_presentations(adapter: String) -> Result<Vec<String>, String> {
    let adapter_impl =
        get_adapter(&adapter).ok_or_else(|| format!("Unknown adapter: {}", adapter))?;
    adapter_impl.get_open_presentations()
}

#[tauri::command]
pub fn get_presentation_state(adapter: String, name: String) -> Result<PresentationState, String> {
    let adapter_impl =
        get_adapter(&adapter).ok_or_else(|| format!("Unknown adapter: {}", adapter))?;
    adapter_impl.get_presentation_state(&name)
}

#[tauri::command]
pub fn get_slide_info(adapter: String, name: String) -> Result<SlideInfo, String> {
    let adapter_impl =
        get_adapter(&adapter).ok_or_else(|| format!("Unknown adapter: {}", adapter))?;
    adapter_impl.get_slide_info(&name)
}

#[tauri::command]
pub fn get_live_status(adapter: String, name: String) -> LiveStatus {
    match get_adapter(&adapter) {
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
        let adapter = adapters::powerpoint::PowerPointAdapter;
        adapter.get_notes_zoom()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(None)
    }
}

#[tauri::command]
pub fn next_slide(adapter: String, name: String) -> Result<SlideInfo, String> {
    let adapter_impl =
        get_adapter(&adapter).ok_or_else(|| format!("Unknown adapter: {}", adapter))?;
    adapter_impl.next_slide(&name)
}

#[tauri::command]
pub fn prev_slide(adapter: String, name: String) -> Result<SlideInfo, String> {
    let adapter_impl =
        get_adapter(&adapter).ok_or_else(|| format!("Unknown adapter: {}", adapter))?;
    adapter_impl.prev_slide(&name)
}
