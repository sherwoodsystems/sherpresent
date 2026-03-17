use crate::adapters::{get_adapter, LiveStatus, PresentationAdapter};
use crate::state::AppState;
use tauri::{AppHandle, Emitter, Manager};

#[tauri::command]
pub fn start_status_polling(
    app: AppHandle,
    state: tauri::State<AppState>,
    adapter: String,
    presentation_name: String,
) -> Result<(), String> {
    log::info!("start_status_polling: called for adapter={}, presentation={}", adapter, presentation_name);
    let mut polling = state.polling_active.lock().unwrap_or_else(|e| e.into_inner());
    if *polling {
        log::info!("start_status_polling: already polling, returning early");
        return Ok(()); // Already polling
    }
    *polling = true;
    drop(polling);

    // Clone the app handle for the thread
    let app_clone = app.clone();
    let state_ref = app.state::<AppState>().polling_active.clone();
    let adapter_config = app.state::<AppState>().adapter_config.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let status_broadcast = app.state::<AppState>().status_broadcast.clone();
    let notes_broadcast = app.state::<AppState>().notes_broadcast.clone();

    let notes_cache = app.state::<AppState>().notes_cache.clone();

    let last_command_at = app.state::<AppState>().last_command_at.clone();

    std::thread::spawn(move || {
        let mut last_broadcast_status: Option<LiveStatus> = None;
        let mut was_presenting = false;
        loop {
            // Check if we should stop
            {
                let polling = state_ref.lock().unwrap_or_else(|e| e.into_inner());
                if !*polling {
                    break;
                }
            }

            // Skip this poll cycle if a slide command was executed within the last 3s.
            // This avoids competing for the Apple Event IPC channel during active use.
            {
                let last_cmd = *last_command_at.lock().unwrap_or_else(|e| e.into_inner());
                if last_cmd > 0 {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    if now.saturating_sub(last_cmd) < 3000 {
                        std::thread::sleep(std::time::Duration::from_millis(2000));
                        continue;
                    }
                }
            }

            // Get current status
            log::info!("Polling: calling get_live_status for adapter={}, presentation={}", adapter, presentation_name);
            let status = if adapter == "canva" {
                let app_state = app_clone.state::<AppState>();
                let canva = app_state.canva_adapter.lock().unwrap_or_else(|e| e.into_inner());
                match canva.as_ref() {
                    Some(a) => a.get_live_status(&presentation_name),
                    None => LiveStatus::default(),
                }
            } else {
                match get_adapter(&adapter, &adapter_config) {
                    Some(adapter_impl) => adapter_impl.get_live_status(&presentation_name),
                    None => LiveStatus::default(),
                }
            };
            log::info!("Polling: status = open={}, presenting={}, slide={}/{}", status.is_open, status.is_presenting, status.current_slide, status.total_slides);

            // Detect transition to presenting state → bulk fetch all notes on a background thread
            if status.is_presenting && !was_presenting {
                log::info!("Polling: presenting state detected (was_presenting=false), spawning background notes fetch");
                if adapter != "canva" {
                    let bg_adapter = adapter.clone();
                    let bg_adapter_config = adapter_config.clone();
                    let bg_name = presentation_name.clone();
                    let bg_notes_cache = notes_cache.clone();
                    let bg_app = app_clone.clone();
                    let bg_notes_broadcast = notes_broadcast.clone();
                    std::thread::spawn(move || {
                        if let Some(adapter_impl) = get_adapter(&bg_adapter, &bg_adapter_config) {
                            let bulk_notes = adapter_impl.get_all_presenter_notes(&bg_name).unwrap_or_default();
                            if !bulk_notes.is_empty() {
                                let mut cache = bg_notes_cache.lock().unwrap_or_else(|e| e.into_inner());
                                for (k, v) in bulk_notes {
                                    cache.insert(k, v);
                                }
                                let cache_snapshot = cache.clone();
                                drop(cache);
                                let _ = bg_app.emit("notes-cache-updated", &cache_snapshot);
                                let _ = bg_notes_broadcast.send(cache_snapshot);
                                log::info!("Polling: background notes fetch complete");
                            }
                        }
                    });
                }
            }
            was_presenting = status.is_presenting;

            // Accumulate presenter notes into cache
            if let Some(ref notes) = status.presenter_notes {
                if status.current_slide > 0 {
                    log::debug!("Polling: caching notes for slide {} ({} chars, empty={})", status.current_slide, notes.len(), notes.trim().is_empty());
                    let app_state = app_clone.state::<AppState>();
                    let mut cache = app_state.notes_cache.lock().unwrap_or_else(|e| e.into_inner());
                    cache.insert(status.current_slide, notes.clone());
                    log::debug!("Polling: notes cache now has {} entries, keys: {:?}", cache.len(), cache.keys().collect::<Vec<_>>());
                    let cache_snapshot = cache.clone();
                    drop(cache);
                    let _ = app_clone.emit("notes-cache-updated", &cache_snapshot);
                    let _ = notes_broadcast.send(cache_snapshot);
                }
            }

            // Emit to frontend
            let _ = app_clone.emit("presentation-status", &status);

            // Broadcast to web server SSE clients only when status changed
            if last_broadcast_status.as_ref() != Some(&status) {
                last_broadcast_status = Some(status.clone());
                let _ = status_broadcast.send(status);
            }

            // Sleep for 2 seconds
            std::thread::sleep(std::time::Duration::from_millis(2000));
        }
    });

    Ok(())
}

#[tauri::command]
pub fn stop_status_polling(state: tauri::State<AppState>) {
    log::info!("stop_status_polling: setting polling_active=false");
    let mut polling = state.polling_active.lock().unwrap_or_else(|e| e.into_inner());
    *polling = false;
}
