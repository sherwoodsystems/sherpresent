use crate::config::AppConfig;
use crate::state::AppState;
use crate::util::LockExt;
use tauri::{AppHandle, Emitter};

/// Start the native OSC server.
#[tauri::command]
pub async fn start_osc_server(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    config: AppConfig,
) -> Result<(), String> {
    log::info!(
        "Starting OSC server on port {} with adapter '{}'",
        config.osc.receive_port,
        config.adapter
    );

    #[cfg(target_os = "windows")]
    {
        log::info!(
            "Windows firewall note: If LAN access doesn't work, run PowerShell as Admin: \
             New-NetFirewallRule -DisplayName 'SherPresent OSC' -Direction Inbound -Protocol UDP -LocalPort {} -Action Allow",
            config.osc.receive_port
        );
    }

    state.start_osc_server(config.osc).await?;
    let _ = app.emit("osc-server-started", ());
    log::info!("OSC server started successfully");

    Ok(())
}

/// Stop the OSC server.
#[tauri::command]
pub async fn stop_osc_server(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let handle = {
        let mut server = state.osc_server.locked();
        server.take()
    };

    if let Some(h) = handle {
        h.stop().await;
        let _ = app.emit("osc-server-stopped", "Server stopped by user");
        log::info!("OSC server stopped");
    }

    Ok(())
}

/// Check if the OSC server is currently running.
#[tauri::command]
pub fn is_osc_server_running(state: tauri::State<AppState>) -> bool {
    let server = state.osc_server.locked();
    server.is_some()
}
