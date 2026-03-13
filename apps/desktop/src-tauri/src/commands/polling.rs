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
    let mut polling = state.polling_active.lock().unwrap();
    if *polling {
        return Ok(()); // Already polling
    }
    *polling = true;
    drop(polling);

    // Clone the app handle for the thread
    let app_clone = app.clone();
    let state_ref = app.state::<AppState>().polling_active.clone();
    let adapter_config = app.state::<AppState>().adapter_config.lock().unwrap().clone();

    std::thread::spawn(move || {
        loop {
            // Check if we should stop
            {
                let polling = state_ref.lock().unwrap();
                if !*polling {
                    break;
                }
            }

            // Get current status
            let status = if adapter == "canva" {
                let app_state = app_clone.state::<AppState>();
                let canva = app_state.canva_adapter.lock().unwrap();
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

            // Emit to frontend
            let _ = app_clone.emit("presentation-status", &status);

            // Sleep for 2 seconds
            std::thread::sleep(std::time::Duration::from_millis(2000));
        }
    });

    Ok(())
}

#[tauri::command]
pub fn stop_status_polling(state: tauri::State<AppState>) {
    let mut polling = state.polling_active.lock().unwrap();
    *polling = false;
}
