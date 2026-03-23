use tauri::{AppHandle, Emitter};
use std::sync::Arc;
use crate::config::AppConfig;
use crate::state::AppState;
use crate::osc::{OscServer, StateManager, CachedState};

/// Start the native OSC server.
#[tauri::command]
pub async fn start_osc_server(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    config: AppConfig,
) -> Result<(), String> {
    // Check if already running
    {
        let server = state.osc_server.lock().unwrap();
        if server.is_some() {
            return Err("OSC server is already running".to_string());
        }
    }

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

    // Create state change channel
    let (state_change_tx, state_change_rx) =
        tokio::sync::mpsc::channel::<CachedState>(32);

    // Create state manager
    let state_manager = Arc::new(StateManager::new(
        config.adapter.clone(),
        config.presentation_name.clone(),
        config.adapter_config.clone(),
        state_change_tx,
        state.latency_store.clone(),
        Some(app.clone()),
        state.last_command_at.clone(),
        Some(state.status_broadcast.clone()),
        Some(state.canva_adapter.clone()),
    ));

    // Initial state fetch
    let initial_state = state_manager.force_refresh().await;

    log::info!(
        "Initial state: presenting={}, slide={}/{}",
        initial_state.is_presenting,
        initial_state.current_slide,
        initial_state.total_slides
    );

    let osc_server = OscServer::new(config.osc.clone(), state_manager)
        .with_scroll_broadcast(state.scroll_broadcast.clone());

    let handle = osc_server
        .start(state_change_rx)
        .await
        .map_err(|e| format!("Failed to start OSC server: {}", e))?;

    // Store the handle
    {
        let mut server_slot = state.osc_server.lock().unwrap();
        *server_slot = Some(handle);
    }

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
        let mut server = state.osc_server.lock().unwrap();
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
    let server = state.osc_server.lock().unwrap();
    server.is_some()
}
