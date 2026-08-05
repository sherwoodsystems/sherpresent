use crate::captions::provider::apple::AppleCaptionSupport;
use crate::captions::{self, audio::AudioDevice, CaptionStatus};
use crate::config;
use crate::state::AppState;

/// List audio input devices the user can capture from.
#[tauri::command]
pub fn list_audio_input_devices() -> Result<Vec<AudioDevice>, String> {
    captions::audio::list_input_devices()
}

/// Start live captions using the saved caption settings.
#[tauri::command]
pub async fn start_captions(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    // Scoped so the guard drops before any await, since MutexGuard is !Send.
    {
        let engine = state.caption_engine.lock().unwrap();
        if engine.is_some() {
            return Err("Captions are already running".to_string());
        }
    }

    let cfg = config::load_config(&app)?;
    let sinks = state.caption_sinks();

    // Clear stale lines so a new session doesn't open with the last one's text.
    {
        let mut buf = sinks.buffer.lock().unwrap();
        buf.clear();
    }

    let engine = captions::start(app.clone(), &cfg.captions, sinks)?;

    {
        let mut slot = state.caption_engine.lock().unwrap();
        *slot = Some(engine);
    }

    Ok(())
}

/// Stop live captions.
#[tauri::command]
pub async fn stop_captions(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let engine = {
        let mut slot = state.caption_engine.lock().unwrap();
        slot.take()
    };

    match engine {
        Some(e) => {
            e.stop().await;
            Ok(())
        }
        None => Err("Captions are not running".to_string()),
    }
}

#[tauri::command]
pub fn is_captions_running(state: tauri::State<AppState>) -> bool {
    let engine = state.caption_engine.lock().unwrap();
    engine.is_some()
}

#[tauri::command]
pub fn get_caption_status(state: tauri::State<AppState>) -> CaptionStatus {
    state.caption_status.lock().unwrap().clone()
}

/// Ask the Apple speech helper what it can actually do on this machine.
///
/// Settings calls this to decide whether the Apple provider is selectable and
/// to tell the operator *why* not — a missing translation language pack is not
/// something the app can fix on their behalf, so it has to be legible before a
/// show rather than as a failure during one.
#[tauri::command]
pub async fn check_apple_captions_support(
    source: Option<String>,
    target: String,
) -> Result<AppleCaptionSupport, String> {
    Ok(captions::provider::apple::probe_support(source.as_deref().unwrap_or(""), &target).await)
}

/// Open System Settings at the Translation Languages pane.
///
/// Done here rather than through the opener plugin: its default ACL covers only
/// http/https/mailto/tel, and scoping a custom scheme with no `//` authority has
/// murky glob semantics. One spawn is deterministic.
#[tauri::command]
pub fn open_translation_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.Localization-Settings.extension")
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Could not open System Settings: {}", e))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Translation language settings are only available on macOS".to_string())
    }
}

/// LAN URL of the chroma-key caption overlay, for a browser source or a
/// fullscreen second display.
#[tauri::command]
pub fn get_captions_url(app: tauri::AppHandle) -> Result<String, String> {
    let cfg = config::load_config(&app)?;
    let local_ip = crate::commands::network::get_local_ip_internal();
    Ok(format!(
        "http://{}:{}/captions",
        local_ip, cfg.web_server.port
    ))
}
