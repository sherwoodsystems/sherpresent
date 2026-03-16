mod adapters;
mod applescript;
mod bridge;
mod channel;
mod config;
mod discovery;
mod generated_constants;
mod osc;
mod state;
mod commands;
mod webserver;

use std::sync::Arc;
use tauri::{Emitter, Manager};
use crate::state::AppState;
use crate::osc::{OscServer, StateManager, CommandSourcePeer};
use crate::discovery::{DiscoveryService, DiscoveredPeer};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging (will use RUST_LOG env var, defaults to info)
    // This helps with debugging OSC server issues
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // Note: We removed tauri_plugin_shell since we no longer use a sidecar
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            // Config
            commands::config::get_config,
            commands::config::save_config,
            // Network
            commands::network::get_local_ip,
            commands::network::get_available_interfaces,
            // Presentations
            commands::presentation::get_adapters,
            commands::presentation::get_open_presentations,
            commands::presentation::get_presentation_state,
            commands::presentation::get_slide_info,
            commands::presentation::get_live_status,
            commands::presentation::get_notes_zoom,
            commands::presentation::next_slide,
            commands::presentation::prev_slide,
            commands::presentation::goto_slide,
            commands::presentation::fetch_all_notes,
            commands::presentation::get_all_notes,
            commands::presentation::clear_notes_cache,
            commands::presentation::start_notes_scan,
            commands::presentation::stop_notes_scan,
            // Polling
            commands::polling::start_status_polling,
            commands::polling::stop_status_polling,
            // OSC Server
            commands::osc::start_osc_server,
            commands::osc::stop_osc_server,
            commands::osc::is_osc_server_running,
            // Canva
            commands::canva::open_canva_remote,
            commands::canva::close_canva_remote,
            commands::canva::log_from_webview,
            commands::canva::get_canva_connection_status,
            commands::canva::update_canva_state,
            commands::canva::update_canva_batch_notes,
            // Channel Discovery
            commands::discovery::get_discovered_peers,
            commands::discovery::start_discovery,
            commands::discovery::stop_discovery,
            commands::discovery::set_instance_name,
            // Bridge Remote Config
            commands::bridge::bridge_get_status,
            commands::bridge::bridge_get_config,
            commands::bridge::bridge_save_config,
            commands::bridge::bridge_get_feedback,
            commands::bridge::bridge_get_devices,
            commands::bridge::bridge_get_registered_devices,
            commands::bridge::bridge_start_registration,
            commands::bridge::bridge_cancel_registration,
            commands::bridge::bridge_confirm_registration,
            commands::bridge::bridge_get_registration_status,
            commands::bridge::bridge_unregister_device,
            commands::bridge::bridge_test_device,
            commands::bridge::bridge_get_logs,
            commands::bridge::bridge_get_satellite_status,
            // Web Server
            commands::webserver::start_web_server,
            commands::webserver::stop_web_server,
            commands::webserver::is_web_server_running,
            commands::webserver::get_web_server_url,
            // Debug
            commands::debug::get_latency_events,
            commands::debug::clear_latency_events,
        ])
        .setup(|app| {
            // Load config on startup (or create default)
            let config = config::load_config(app.handle()).unwrap_or_default();

            // Log startup info
            log::info!(
                "Sher Present Settings starting with adapter: {}",
                config.adapter
            );

            // Auto-start OSC server
            // Store adapter config in AppState
            {
                let state = app.state::<AppState>();
                let mut ac = state.adapter_config.lock().unwrap();
                *ac = config.adapter_config.clone();
            }

            let app_handle = app.handle().clone();
            let osc_config = config.osc.clone();
            let adapter = config.adapter.clone();
            let presentation_name = config.presentation_name.clone();
            let channel_config = config.channel.clone();
            let adapter_config = config.adapter_config.clone();

            let latency_store = {
                let state = app_handle.state::<AppState>();
                state.latency_store.clone()
            };

            tauri::async_runtime::spawn(async move {
                log::info!("Auto-starting OSC server on port {}", osc_config.receive_port);

                // Create state change channel
                let (state_change_tx, state_change_rx) =
                    tokio::sync::mpsc::channel::<osc::CachedState>(32);

                // Create peer tracking channel for broadcast mode
                let (peer_tx, mut peer_rx) = tokio::sync::mpsc::channel::<CommandSourcePeer>(32);

                // Create state manager
                let state_manager = Arc::new(StateManager::new(
                    adapter,
                    presentation_name,
                    adapter_config,
                    state_change_tx,
                    latency_store,
                    Some(app_handle.clone()),
                ));

                // Initial state fetch
                let initial_state = state_manager.force_refresh().await;
                log::info!(
                    "Initial state: presenting={}, slide={}/{}",
                    initial_state.is_presenting,
                    initial_state.current_slide,
                    initial_state.total_slides
                );

                // Start OSC server with broadcast mode if enabled
                let osc_server = if channel_config.enabled && channel_config.broadcast_mode {
                    log::info!(
                        "Broadcast mode enabled for channel '{}' on port {}",
                        channel_config.channel_name,
                        channel_config.broadcast_port
                    );
                    OscServer::with_broadcast_and_peer_tracking(
                        osc_config,
                        state_manager,
                        &channel_config,
                        peer_tx,
                    )
                } else {
                    OscServer::new(osc_config, state_manager)
                };

                match osc_server.start(state_change_rx).await {
                    Ok(handle) => {
                        // Store the handle
                        let state = app_handle.state::<AppState>();
                        let mut server_slot = state.osc_server.lock().unwrap();
                        *server_slot = Some(handle);

                        // Spawn peer tracking task
                        let peer_app = app_handle.clone();
                        let peer_state = app_handle.state::<AppState>().inner().command_source_peers.clone();
                        let peer_id_counter = app_handle.state::<AppState>().inner().next_peer_display_id.clone();

                        tokio::spawn(async move {
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

                                let discovered = DiscoveredPeer {
                                    instance_id: key.clone(),
                                    display_name: Some(display_name),
                                    display_id,
                                    host: peer.address.ip().to_string(),
                                    port: peer.address.port(),
                                    channel: peer.channel,
                                    version: "bridge".to_string(),
                                    is_self: false,
                                    config_port: None,
                                };

                                {
                                    let mut peers = peer_state.lock().unwrap();
                                    peers.insert(key.clone(), discovered);
                                }

                                if is_new {
                                    log::info!("Discovered command source peer: {}", key);
                                    let peers_list: Vec<DiscoveredPeer> = {
                                        let p = peer_state.lock().unwrap();
                                        p.values().cloned().collect()
                                    };
                                    let _ = peer_app.emit("channel-peers-updated", &peers_list);
                                }
                            }
                        });

                        let _ = app_handle.emit("osc-server-started", ());
                        log::info!("OSC server auto-started successfully");
                    }
                    Err(e) => {
                        log::error!("Failed to auto-start OSC server: {}", e);
                    }
                }
            });

            // Auto-start discovery service for peer discovery
            let app_handle2 = app.handle().clone();
            let discovery_config = config.channel.clone();
            let osc_port = config.osc.receive_port;

            tauri::async_runtime::spawn(async move {
                log::info!("Auto-starting discovery service");

                // Create channel for peer updates
                let (peer_tx, mut peer_rx) = tokio::sync::mpsc::channel::<Vec<DiscoveredPeer>>(32);

                // Create discovery service
                // Use empty channel_name to show all peers regardless of channel
                match DiscoveryService::new(
                    discovery_config.instance_id.clone(),
                    discovery_config.display_name.clone(),
                    discovery_config.channel_name.clone(),
                    osc_port,
                    peer_tx,
                    discovery_config.network_interface.clone(),
                ) {
                    Ok(mut service) => {
                        if let Err(e) = service.register() {
                            log::error!("Failed to register mDNS service: {}", e);
                            return;
                        }

                        if let Err(e) = service.start_browsing() {
                            log::error!("Failed to start mDNS browsing: {}", e);
                            return;
                        }

                        // Store the service in a separate scope to release lock
                        {
                            let state = app_handle2.state::<AppState>();
                            let mut discovery_slot = state.discovery_service.lock().unwrap();
                            *discovery_slot = Some(service);
                            log::info!("Discovery service auto-started successfully");
                        }

                        // Forward peer updates to frontend (lock is released)
                        while let Some(peers) = peer_rx.recv().await {
                            let _ = app_handle2.emit("peers-updated", &peers);
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to create discovery service: {}", e);
                    }
                }
            });

            // Auto-start web server if enabled
            if config.web_server.enabled {
                let app_handle3 = app.handle().clone();
                let web_server_config = config.web_server.clone();

                tauri::async_runtime::spawn(async move {
                    log::info!("Auto-starting web server on port {}", web_server_config.port);

                    let state = app_handle3.state::<AppState>();
                    let notes_cache = state.notes_cache.clone();
                    let status_broadcast = state.status_broadcast.clone();
                    let notes_broadcast = state.notes_broadcast.clone();

                    match webserver::start(
                        web_server_config,
                        notes_cache,
                        status_broadcast,
                        notes_broadcast,
                    )
                    .await
                    {
                        Ok(handle) => {
                            let mut ws_handle = state.web_server_handle.lock().unwrap();
                            *ws_handle = Some(handle);
                            log::info!("Web server auto-started successfully");
                        }
                        Err(e) => {
                            log::error!("Failed to auto-start web server: {}", e);
                        }
                    }
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // Stop OSC server when window closes
            //
            // Note: We can't easily call async stop() here, so we just
            // take the handle. The handle's Drop impl will clean up.
            // In practice, the app is closing anyway so this is fine.
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                let state = window.state::<AppState>();
                let mut server = state.osc_server.lock().unwrap();
                if server.take().is_some() {
                    log::info!("Window closing, OSC server handle dropped");
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
