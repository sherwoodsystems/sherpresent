use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use crate::adapters::{
    canva::CanvaAdapter,
    get_adapter, get_available_adapters, LiveStatus, PresentationAdapter, PresentationState,
    SlideInfo,
};
use crate::config::AdapterConfig;
use crate::osc::latency::{self, CommandSource};
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
    state.adapter_config.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Resolve the correct adapter (Canva singleton or get_adapter()) and call `f` on it.
fn with_adapter<T>(
    adapter_name: &str,
    state: &AppState,
    f: impl FnOnce(&dyn PresentationAdapter) -> T,
) -> Result<T, String> {
    if adapter_name == "canva" {
        let canva = state.canva_adapter.lock().unwrap_or_else(|e| e.into_inner());
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
        let canva = canva_adapter.lock().unwrap_or_else(|e| e.into_inner());
        let a = canva.as_ref().ok_or("Canva adapter not initialized".to_string())?;
        Ok(f(a))
    } else {
        let config = adapter_config.lock().unwrap_or_else(|e| e.into_inner()).clone();
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

/// Record the current time as the last command timestamp (suppresses polling for 3s).
fn stamp_command_time(state: &AppState) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    *state.last_command_at.lock().unwrap_or_else(|e| e.into_inner()) = now;
}

/// Immediately broadcast an updated LiveStatus to the web server after a slide command.
/// This ensures the stage view updates instantly instead of waiting for the next 2s poll cycle.
fn broadcast_status_now(app: &AppHandle, state: &AppState, adapter: &str, name: &str) {
    let status = with_adapter(adapter, state, |a| a.get_live_status(name))
        .unwrap_or_default();
    let _ = app.emit("presentation-status", &status);
    let _ = state.status_broadcast.send(status);
}

#[tauri::command]
pub fn next_slide(app: AppHandle, adapter: String, name: String, state: tauri::State<AppState>) -> Result<SlideInfo, String> {
    stamp_command_time(&state);
    let before = latency::monotonic_ms();
    let result = with_adapter(&adapter, &state, |a| a.next_slide(&name))?;
    let after = latency::monotonic_ms();
    let event = latency::make_event(before, after, "next".to_string(), CommandSource::Ui, adapter.clone());
    state.latency_store.push(event.clone());
    let _ = app.emit("latency-event", &event);
    broadcast_status_now(&app, &state, &adapter, &name);
    result
}

#[tauri::command]
pub fn prev_slide(app: AppHandle, adapter: String, name: String, state: tauri::State<AppState>) -> Result<SlideInfo, String> {
    stamp_command_time(&state);
    let before = latency::monotonic_ms();
    let result = with_adapter(&adapter, &state, |a| a.prev_slide(&name))?;
    let after = latency::monotonic_ms();
    let event = latency::make_event(before, after, "prev".to_string(), CommandSource::Ui, adapter.clone());
    state.latency_store.push(event.clone());
    let _ = app.emit("latency-event", &event);
    broadcast_status_now(&app, &state, &adapter, &name);
    result
}

#[tauri::command]
pub fn goto_slide(app: AppHandle, adapter: String, name: String, slide: i32, state: tauri::State<AppState>) -> Result<SlideInfo, String> {
    stamp_command_time(&state);
    let before = latency::monotonic_ms();
    let result = with_adapter(&adapter, &state, |a| a.goto_slide(&name, slide))?;
    let after = latency::monotonic_ms();
    let event = latency::make_event(before, after, format!("goto:{}", slide), CommandSource::Ui, adapter.clone());
    state.latency_store.push(event.clone());
    let _ = app.emit("latency-event", &event);
    broadcast_status_now(&app, &state, &adapter, &name);
    result
}

#[tauri::command]
pub fn fetch_all_notes(adapter: String, name: String, state: tauri::State<AppState>) -> Result<HashMap<i32, String>, String> {
    log::info!("fetch_all_notes: called for adapter={}, name={}", adapter, name);
    // Return cache if already populated (avoids duplicate fetch race with polling)
    let cache = state.notes_cache.lock().map_err(|e| e.to_string())?;
    if !cache.is_empty() {
        log::info!("fetch_all_notes: returning cached {} entries (skipping expensive fetch)", cache.len());
        return Ok(cache.clone());
    }
    drop(cache);

    log::info!("fetch_all_notes: cache empty, doing expensive AppleScript fetch...");
    let bulk_notes = if adapter == "canva" {
        // Canva doesn't support bulk fetch — return whatever is cached
        HashMap::new()
    } else {
        with_adapter(&adapter, &state, |a| a.get_all_presenter_notes(&name))??
    };

    log::debug!("fetch_all_notes: bulk fetch returned {} entries", bulk_notes.len());
    let mut cache = state.notes_cache.lock().map_err(|e| e.to_string())?;
    let cache_before = cache.len();
    // Merge bulk results into cache (bulk results take precedence)
    for (k, v) in bulk_notes {
        cache.insert(k, v);
    }
    log::debug!("fetch_all_notes: cache {} -> {} entries, keys: {:?}", cache_before, cache.len(), cache.keys().collect::<Vec<_>>());
    Ok(cache.clone())
}

#[tauri::command]
pub fn get_all_notes(state: tauri::State<AppState>) -> HashMap<i32, String> {
    state.notes_cache.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
pub fn clear_notes_cache(state: tauri::State<AppState>) {
    log::info!("clear_notes_cache: clearing notes cache and compiled AppleScript cache");
    state.notes_cache.lock().unwrap_or_else(|e| e.into_inner()).clear();
    // Also clear compiled AppleScript cache since adapter/presentation may have changed
    crate::applescript::clear_compiled_cache();
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
        let active = state.notes_scan_active.lock().unwrap_or_else(|e| e.into_inner());
        if *active {
            return Err("Scan already in progress".to_string());
        }
    }

    // Get total slides and current slide to restore later
    let slide_info = with_adapter(&adapter, &state, |a| a.get_slide_info(&name))??;
    if slide_info.total <= 0 {
        return Err("Cannot scan: total slides unknown".to_string());
    }
    let total = slide_info.total;
    let original_slide = slide_info.current;

    // Mark scan as active
    *state.notes_scan_active.lock().unwrap_or_else(|e| e.into_inner()) = true;

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
                let active = scan_active.lock().unwrap_or_else(|e| e.into_inner());
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
                    let cache = notes_cache.lock().unwrap_or_else(|e| e.into_inner());
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
                        let mut cache = notes_cache.lock().unwrap_or_else(|e| e.into_inner());
                        cache.insert(i, notes_text);
                    }
                }
            }

            // Emit updated cache
            {
                let cache = notes_cache.lock().unwrap_or_else(|e| e.into_inner());
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
        *scan_active.lock().unwrap_or_else(|e| e.into_inner()) = false;
        emit_progress(total, total, "complete");
    });

    Ok(())
}

#[tauri::command]
pub fn stop_notes_scan(state: tauri::State<AppState>) {
    *state.notes_scan_active.lock().unwrap_or_else(|e| e.into_inner()) = false;
}
