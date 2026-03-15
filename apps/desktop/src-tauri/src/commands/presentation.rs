use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use crate::adapters::{
    canva::CanvaAdapter,
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

/// Resolve the correct adapter (Canva singleton or get_adapter()) and call `f` on it.
fn with_adapter<T>(
    adapter_name: &str,
    state: &AppState,
    f: impl FnOnce(&dyn PresentationAdapter) -> T,
) -> Result<T, String> {
    if adapter_name == "canva" {
        let canva = state.canva_adapter.lock().unwrap();
        let a = canva.as_ref().ok_or("Canva adapter not initialized".to_string())?;
        Ok(f(a))
    } else {
        let config = get_adapter_config(state);
        let a = get_adapter(adapter_name, &config)
            .ok_or_else(|| format!("Unknown adapter: {}", adapter_name))?;
        Ok(f(a.as_ref()))
    }
}

/// Like `with_adapter` but works with cloned `Arc<Mutex<...>>` values (for use in spawned threads).
fn with_adapter_from_arcs<T>(
    adapter_name: &str,
    adapter_config: &Arc<Mutex<AdapterConfig>>,
    canva_adapter: &Arc<Mutex<Option<CanvaAdapter>>>,
    f: impl FnOnce(&dyn PresentationAdapter) -> T,
) -> Result<T, String> {
    if adapter_name == "canva" {
        let canva = canva_adapter.lock().unwrap();
        let a = canva.as_ref().ok_or("Canva adapter not initialized".to_string())?;
        Ok(f(a))
    } else {
        let config = adapter_config.lock().unwrap().clone();
        let a = get_adapter(adapter_name, &config)
            .ok_or_else(|| format!("Unknown adapter: {}", adapter_name))?;
        Ok(f(a.as_ref()))
    }
}

#[tauri::command]
pub fn get_open_presentations(adapter: String, state: tauri::State<AppState>) -> Result<Vec<String>, String> {
    with_adapter(&adapter, &state, |a| a.get_open_presentations())?
}

#[tauri::command]
pub fn get_presentation_state(adapter: String, name: String, state: tauri::State<AppState>) -> Result<PresentationState, String> {
    with_adapter(&adapter, &state, |a| a.get_presentation_state(&name))?
}

#[tauri::command]
pub fn get_slide_info(adapter: String, name: String, state: tauri::State<AppState>) -> Result<SlideInfo, String> {
    with_adapter(&adapter, &state, |a| a.get_slide_info(&name))?
}

#[tauri::command]
pub fn get_live_status(adapter: String, name: String, state: tauri::State<AppState>) -> LiveStatus {
    with_adapter(&adapter, &state, |a| a.get_live_status(&name))
        .unwrap_or_default()
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
    with_adapter(&adapter, &state, |a| a.next_slide(&name))?
}

#[tauri::command]
pub fn prev_slide(adapter: String, name: String, state: tauri::State<AppState>) -> Result<SlideInfo, String> {
    with_adapter(&adapter, &state, |a| a.prev_slide(&name))?
}

#[tauri::command]
pub fn goto_slide(adapter: String, name: String, slide: i32, state: tauri::State<AppState>) -> Result<SlideInfo, String> {
    with_adapter(&adapter, &state, |a| a.goto_slide(&name, slide))?
}

#[tauri::command]
pub fn fetch_all_notes(adapter: String, name: String, state: tauri::State<AppState>) -> Result<HashMap<i32, String>, String> {
    let bulk_notes = if adapter == "canva" {
        // Canva doesn't support bulk fetch — return whatever is cached
        HashMap::new()
    } else {
        with_adapter(&adapter, &state, |a| a.get_all_presenter_notes(&name))??
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

#[derive(Clone, serde::Serialize)]
struct NotesScanProgress {
    current: i32,
    total: i32,
    status: String,
}

#[tauri::command]
pub fn start_notes_scan(
    app: AppHandle,
    state: tauri::State<AppState>,
    adapter: String,
    name: String,
) -> Result<(), String> {
    // Check if already scanning
    {
        let active = state.notes_scan_active.lock().unwrap();
        if *active {
            return Err("Scan already in progress".to_string());
        }
    }

    // Get total slides and current slide to restore later
    let total = with_adapter(&adapter, &state, |a| a.get_slide_info(&name))??. total;
    if total <= 0 {
        return Err("Cannot scan: total slides unknown".to_string());
    }
    let original_slide = with_adapter(&adapter, &state, |a| a.get_slide_info(&name))??.current;

    // Mark scan as active
    *state.notes_scan_active.lock().unwrap() = true;

    let scan_active = state.notes_scan_active.clone();
    let notes_cache = state.notes_cache.clone();
    let adapter_config = state.adapter_config.clone();
    let canva_adapter = state.canva_adapter.clone();
    let app_clone = app.clone();

    std::thread::spawn(move || {
        let emit_progress = |current: i32, total: i32, status: &str| {
            let _ = app_clone.emit("notes-scan-progress", NotesScanProgress {
                current,
                total,
                status: status.to_string(),
            });
        };

        for i in 1..=total {
            // Check cancellation
            {
                let active = scan_active.lock().unwrap();
                if !*active {
                    emit_progress(i, total, "cancelled");
                    return;
                }
            }

            emit_progress(i, total, "scanning");

            // Navigate to slide i
            let goto_result = with_adapter_from_arcs(
                &adapter, &adapter_config, &canva_adapter,
                |a| a.goto_slide(&name, i),
            );

            match goto_result {
                Ok(Ok(_)) => {}
                other => {
                    log::warn!("Notes scan: failed to goto slide {}: {:?}", i, other);
                    continue;
                }
            }

            if adapter == "canva" {
                // For Canva: wait for notes_cache to get populated by the batch notes handler
                // Poll for up to 1.5s
                let mut found = false;
                for _ in 0..15 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    let cache = notes_cache.lock().unwrap();
                    if cache.contains_key(&i) {
                        found = true;
                        break;
                    }
                }
                if !found {
                    log::debug!("Notes scan: no notes arrived for slide {} (Canva)", i);
                }
            } else {
                // For non-Canva: brief wait for adapter state to settle, then read notes
                std::thread::sleep(std::time::Duration::from_millis(200));
                if let Ok(Ok(Some(notes_text))) = with_adapter_from_arcs(
                    &adapter, &adapter_config, &canva_adapter,
                    |a| a.get_presenter_notes(&name),
                ) {
                    if !notes_text.is_empty() {
                        let mut cache = notes_cache.lock().unwrap();
                        cache.insert(i, notes_text);
                    }
                }
            }

            // Emit updated cache
            {
                let cache = notes_cache.lock().unwrap();
                let snapshot = cache.clone();
                drop(cache);
                let _ = app_clone.emit("notes-cache-updated", &snapshot);
            }
        }

        // Restore original slide position
        let _ = with_adapter_from_arcs(
            &adapter, &adapter_config, &canva_adapter,
            |a| a.goto_slide(&name, original_slide),
        );

        // Mark scan as complete
        *scan_active.lock().unwrap() = false;
        emit_progress(total, total, "complete");
    });

    Ok(())
}

#[tauri::command]
pub fn stop_notes_scan(state: tauri::State<AppState>) {
    *state.notes_scan_active.lock().unwrap() = false;
}
