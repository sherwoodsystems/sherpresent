mod adapters;
mod applescript;
mod bridge;
mod captions;
mod config;
mod generated_constants;
mod osc;
mod state;
mod commands;
mod webserver;

use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use sherpresent_core::{DiscoveryService, DiscoveredPeer};
use crate::state::AppState;
use crate::osc::{OscServer, StateManager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging (will use RUST_LOG env var, defaults to info)
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
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
            commands::presentation::save_text_file,
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
            // Discovery
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
            commands::bridge::bridge_shutdown,
            // Web Server
            commands::webserver::start_web_server,
            commands::webserver::is_web_server_running,
            commands::webserver::get_web_server_url,
            // Captions
            commands::captions::list_audio_input_devices,
            commands::captions::start_captions,
            commands::captions::stop_captions,
            commands::captions::is_captions_running,
            commands::captions::get_caption_status,
            commands::captions::get_captions_url,
            commands::captions::check_apple_captions_support,
            commands::captions::open_translation_settings,
            // Debug
            commands::debug::get_latency_events,
            commands::debug::clear_latency_events,
            // App
            commands::app::quit_app,
        ])
        .setup(|app| {
            // Load config on startup (or create default)
            let config = config::load_config(app.handle()).unwrap_or_default();

            log::info!(
                "Sher Present Settings starting with adapter: {}",
                config.adapter
            );

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
            let adapter_config = config.adapter_config.clone();

            let latency_store = {
                let state = app_handle.state::<AppState>();
                state.latency_store.clone()
            };
            let last_command_at = {
                let state = app_handle.state::<AppState>();
                state.last_command_at.clone()
            };
            let status_broadcast = {
                let state = app_handle.state::<AppState>();
                state.status_broadcast.clone()
            };
            let scroll_broadcast = {
                let state = app_handle.state::<AppState>();
                state.scroll_broadcast.clone()
            };

            // Auto-start OSC server
            tauri::async_runtime::spawn(async move {
                log::info!("Auto-starting OSC server on port {}", osc_config.receive_port);

                // Create state change channel
                let (state_change_tx, state_change_rx) =
                    tokio::sync::mpsc::channel::<osc::CachedState>(32);

                // Create state manager
                let canva_adapter = {
                    let state = app_handle.state::<AppState>();
                    state.canva_adapter.clone()
                };
                let state_manager = Arc::new(StateManager::new(
                    adapter,
                    presentation_name,
                    adapter_config,
                    state_change_tx,
                    latency_store,
                    Some(app_handle.clone()),
                    last_command_at,
                    Some(status_broadcast),
                    Some(canva_adapter),
                ));

                // Initial state fetch
                let initial_state = state_manager.force_refresh().await;
                log::info!(
                    "Initial state: presenting={}, slide={}/{}",
                    initial_state.is_presenting,
                    initial_state.current_slide,
                    initial_state.total_slides
                );

                // Store state manager in AppState for webserver access
                {
                    let state = app_handle.state::<AppState>();
                    let mut sm = state.state_manager.lock().unwrap();
                    *sm = Some(state_manager.clone());
                }

                // Start OSC server (direct mode only)
                let osc_server = OscServer::new(osc_config, state_manager)
                    .with_scroll_broadcast(scroll_broadcast.clone());

                match osc_server.start(state_change_rx).await {
                    Ok(handle) => {
                        let state = app_handle.state::<AppState>();
                        let mut server_slot = state.osc_server.lock().unwrap();
                        *server_slot = Some(handle);

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
            let discovery_config = config.discovery.clone();
            let osc_port = config.osc.receive_port;

            tauri::async_runtime::spawn(async move {
                log::info!("Auto-starting discovery service");

                // Create channel for peer updates
                let (peer_tx, mut peer_rx) = tokio::sync::mpsc::channel::<Vec<DiscoveredPeer>>(32);

                // Create discovery service
                match DiscoveryService::new(
                    discovery_config.instance_id.clone(),
                    discovery_config.display_name.clone(),
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

                        {
                            let state = app_handle2.state::<AppState>();
                            let mut discovery_slot = state.discovery_service.lock().unwrap();
                            *discovery_slot = Some(service);
                            log::info!("Discovery service auto-started successfully");
                        }

                        // Forward peer updates to frontend
                        while let Some(peers) = peer_rx.recv().await {
                            let _ = app_handle2.emit("peers-updated", &peers);
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to create discovery service: {}", e);
                    }
                }
            });

            // Auto-start web server (always on)
            {
                let app_handle3 = app.handle().clone();
                let web_server_config = config.web_server.clone();
                let captions_config = config.captions.clone();

                tauri::async_runtime::spawn(async move {
                    log::info!("Auto-starting web server on port {}", web_server_config.port);

                    let state = app_handle3.state::<AppState>();
                    let notes_cache = state.notes_cache.clone();
                    let status_broadcast = state.status_broadcast.clone();
                    let notes_broadcast = state.notes_broadcast.clone();
                    let scroll_broadcast = state.scroll_broadcast.clone();
                    let state_manager = state.state_manager.lock().unwrap().clone();
                    let caption_sinks = state.caption_sinks();

                    match webserver::start(
                        web_server_config,
                        notes_cache,
                        status_broadcast,
                        notes_broadcast,
                        scroll_broadcast,
                        state_manager,
                        caption_sinks,
                        &captions_config,
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

            // Resume captions if they were running when the app last closed.
            // `enabled` is set by the Start/Stop buttons, so this only fires for
            // an operator who deliberately left them on — it opens a *billable*
            // provider session, so it must never be on by default.
            if config.captions.enabled {
                let app_handle4 = app.handle().clone();
                let captions_config = config.captions.clone();

                tauri::async_runtime::spawn(async move {
                    log::info!("Auto-starting captions ({})", captions_config.provider);

                    let state = app_handle4.state::<AppState>();
                    let sinks = state.caption_sinks();

                    match captions::start(app_handle4.clone(), &captions_config, sinks) {
                        Ok(engine) => {
                            let mut slot = state.caption_engine.lock().unwrap();
                            *slot = Some(engine);
                        }
                        Err(e) => {
                            log::error!("Failed to auto-start captions: {}", e);
                        }
                    }
                });
            }

            // System tray setup
            setup_tray(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            // Minimize to tray on close instead of quitting
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                log::info!("Window hidden to tray");
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Simplified tray icon and menu setup for production
fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show_i = MenuItemBuilder::with_id("show", "Show").build(app)?;
    let quit_i = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app)
        .item(&show_i)
        .separator()
        .item(&quit_i)
        .build()?;

    let icon = app.default_window_icon()
        .cloned()
        .ok_or("Failed to get default window icon")?;

    let _tray = TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, .. } = event {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}
