use tauri::{AppHandle, Emitter};
use std::sync::Arc;
use crate::config::AppConfig;
use crate::state::AppState;
use crate::osc::{OscServer, StateManager, CommandSourcePeer, CachedState};

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
             New-NetFirewallRule -DisplayName 'sher-present OSC' -Direction Inbound -Protocol UDP -LocalPort {} -Action Allow",
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
    ));

    // Initial state fetch
    let initial_state = state_manager.force_refresh().await;

    log::info!(
        "Initial state: presenting={}, slide={}/{}",
        initial_state.is_presenting,
        initial_state.current_slide,
        initial_state.total_slides
    );

    // Create peer tracking channel for broadcast mode
    let (peer_tx, mut peer_rx) = tokio::sync::mpsc::channel::<CommandSourcePeer>(32);

    // Use broadcast mode if channel is enabled and broadcast_mode is true
    let osc_server = if config.channel.enabled && config.channel.broadcast_mode {
        log::info!(
            "Broadcast mode enabled for channel '{}' on port {}",
            config.channel.channel_name,
            config.channel.broadcast_port
        );
        OscServer::with_broadcast_and_peer_tracking(
            config.osc.clone(),
            state_manager,
            &config.channel,
            peer_tx,
        )
    } else {
        OscServer::new(config.osc.clone(), state_manager)
    };

    let handle = osc_server
        .start(state_change_rx)
        .await
        .map_err(|e| format!("Failed to start OSC server: {}", e))?;

    // Store the handle
    {
        let mut server_slot = state.osc_server.lock().unwrap();
        *server_slot = Some(handle);
    }

    // Spawn peer tracking task
    let peer_app = app.clone();
    let peer_state = state.command_source_peers.clone();
    let peer_id_counter = state.next_peer_display_id.clone();

    tokio::spawn(async move {
        use crate::discovery::DiscoveredPeer;

        while let Some(peer) = peer_rx.recv().await {
            let key = peer.address.to_string();
            let display_name = format!("Device @ {}", peer.address.ip());

            // Check if this is a new peer and get/assign display ID
            let (is_new, display_id) = {
                let peers = peer_state.lock().unwrap();
                if let Some(existing) = peers.get(&key) {
                    (false, existing.display_id)
                } else {
                    let mut id = peer_id_counter.lock().unwrap();
                    let new_id = *id;
                    *id = id.wrapping_add(1);
                    (true, new_id)
                }
            };

            // Create a DiscoveredPeer from the command source
            let discovered = DiscoveredPeer {
                instance_id: key.clone(),
                display_name: Some(display_name),
                display_id,
                host: peer.address.ip().to_string(),
                port: peer.address.port(),
                channel: peer.channel,
                version: "bridge".to_string(), // Mark as bridge device
                is_self: false,
                config_port: None,
            };

            // Update the peer map
            {
                let mut peers = peer_state.lock().unwrap();
                peers.insert(key.clone(), discovered);
            }

            // Emit event if this is a new peer
            if is_new {
                log::info!("Discovered command source peer: {}", key);
                // Get current peer list and emit
                let peers_list: Vec<DiscoveredPeer> = {
                    let p = peer_state.lock().unwrap();
                    p.values().cloned().collect()
                };
                let _ = peer_app.emit("channel-peers-updated", &peers_list);
            }
        }
    });

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
