use crate::adapters::{
    get_available_adapters, LiveStatus, PresentationAdapter, PresentationState, SlideInfo,
};
use crate::osc::latency::CommandSource;
use crate::osc::state_manager::NotesCache;
use crate::state::AppState;
use crate::util::LockExt;
use tauri::{AppHandle, Emitter};

#[tauri::command]
pub fn get_adapters() -> Vec<(String, String)> {
    get_available_adapters()
        .into_iter()
        .map(|(id, name)| (id.to_string(), name.to_string()))
        .collect()
}

/// Resolve the named adapter (the Canva singleton or a fresh one) and call `f` on it.
fn with_adapter<T>(
    adapter_name: &str,
    state: &AppState,
    f: impl FnOnce(&dyn PresentationAdapter) -> T,
) -> Result<T, String> {
    state.state_manager.with_adapter(adapter_name, f)
}

#[tauri::command]
pub fn get_open_presentations(
    adapter: String,
    state: tauri::State<AppState>,
) -> Result<Vec<String>, String> {
    with_adapter(&adapter, &state, |a| a.get_open_presentations())?
}

#[tauri::command]
pub fn get_presentation_state(
    adapter: String,
    name: String,
    state: tauri::State<AppState>,
) -> Result<PresentationState, String> {
    with_adapter(&adapter, &state, |a| a.get_presentation_state(&name))?
}

#[tauri::command]
pub fn get_slide_info(
    adapter: String,
    name: String,
    state: tauri::State<AppState>,
) -> Result<SlideInfo, String> {
    with_adapter(&adapter, &state, |a| a.get_slide_info(&name))?
}

/// The cached status, as last published in `presentation-status` events.
#[tauri::command]
pub fn get_status(state: tauri::State<AppState>) -> LiveStatus {
    LiveStatus::from(&state.state_manager.get_state())
}

#[tauri::command]
pub fn get_notes_zoom(state: tauri::State<AppState>) -> Result<Option<i32>, String> {
    state
        .state_manager
        .with_current_adapter(|a, _| a.get_notes_zoom())?
}

// Navigation goes through the StateManager like OSC and the web page do, so
// every surface gets the same optimistic update, latency log and feedback.
// The new position arrives as a `presentation-status` event.

#[tauri::command]
pub fn next_slide(state: tauri::State<AppState>) {
    state.state_manager.next_slide(CommandSource::Ui);
}

#[tauri::command]
pub fn prev_slide(state: tauri::State<AppState>) {
    state.state_manager.prev_slide(CommandSource::Ui);
}

#[tauri::command]
pub fn goto_slide(slide: i32, state: tauri::State<AppState>) {
    state.state_manager.goto_slide(slide, CommandSource::Ui);
}

/// Every slide's notes for the current presentation, fetching them in bulk
/// if nothing is cached yet.
#[tauri::command]
pub fn fetch_all_notes(state: tauri::State<AppState>) -> Result<NotesCache, String> {
    let sm = &state.state_manager;
    let cached = sm.notes();
    // Polling fills the cache when a show starts; don't race it for the
    // AppleScript lock if it already has.
    if !cached.is_empty() {
        return Ok(cached);
    }
    let bulk = sm.with_current_adapter(|a, name| a.get_all_presenter_notes(name))??;
    sm.add_notes(bulk);
    Ok(sm.notes())
}

#[tauri::command]
pub fn get_all_notes(state: tauri::State<AppState>) -> NotesCache {
    state.state_manager.notes()
}

#[derive(Clone, serde::Serialize)]
struct NotesScanProgress {
    current: i32,
    total: i32,
    status: String,
}

/// Visit every slide of the current presentation to collect notes from
/// adapters that can't fetch them in bulk, then return to where it was.
#[tauri::command]
pub fn start_notes_scan(app: AppHandle, state: tauri::State<AppState>) -> Result<(), String> {
    {
        let mut active = state.notes_scan_active.locked();
        if *active {
            return Err("Scan already in progress".to_string());
        }
        *active = true;
    }
    let sm = state.state_manager.clone();
    let scan_active = state.notes_scan_active.clone();

    // Get total slides and current slide to restore later
    let slide_info = match sm.with_current_adapter(|a, name| a.get_slide_info(name)) {
        Ok(Ok(info)) if info.total > 0 => Ok(info),
        Ok(Ok(_)) => Err("Cannot scan: total slides unknown".to_string()),
        Ok(Err(e)) | Err(e) => Err(e),
    }
    .inspect_err(|_| *scan_active.locked() = false)?;
    let total = slide_info.total;
    let is_canva = sm.adapter_name() == "canva";

    std::thread::spawn(move || {
        let emit_progress = |current: i32, status: &str| {
            let _ = app.emit(
                "notes-scan-progress",
                NotesScanProgress {
                    current,
                    total,
                    status: status.to_string(),
                },
            );
        };

        for i in 1..=total {
            if !*scan_active.locked() {
                emit_progress(i, "cancelled");
                return;
            }
            emit_progress(i, "scanning");

            match sm.with_current_adapter(|a, name| a.goto_slide(name, i)) {
                Ok(Ok(_)) => {}
                other => {
                    log::warn!("Notes scan: failed to goto slide {}: {:?}", i, other);
                    continue;
                }
            }

            if is_canva {
                // Canva's webview pushes notes into the cache itself; wait up to 1.5s.
                let arrived = (0..15).any(|_| {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    sm.notes().contains_key(&i)
                });
                if !arrived {
                    log::debug!("Notes scan: no notes arrived for slide {} (Canva)", i);
                }
            } else {
                // Brief wait for adapter state to settle, then read notes
                std::thread::sleep(std::time::Duration::from_millis(200));
                if let Ok(Ok(Some(text))) =
                    sm.with_current_adapter(|a, name| a.get_presenter_notes(name))
                {
                    if !text.is_empty() {
                        sm.add_notes([(i, text)]);
                    }
                }
            }
        }

        // Restore original slide position
        let current = slide_info.current;
        let _ = sm.with_current_adapter(|a, name| a.goto_slide(name, current));

        *scan_active.locked() = false;
        emit_progress(total, "complete");
    });

    Ok(())
}

#[tauri::command]
pub fn stop_notes_scan(state: tauri::State<AppState>) {
    *state.notes_scan_active.locked() = false;
}

#[tauri::command]
pub fn save_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, &content).map_err(|e| e.to_string())
}
