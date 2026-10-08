mod adapters;
mod applescript;
mod bridge;
mod captions;
mod commands;
mod config;
mod ontime;
mod osc;
mod output;
mod sidecar;
mod state;
mod util;
mod webserver;

use crate::state::AppState;
use crate::util::LockExt;
use sherpresent_core::{DiscoveredPeer, DiscoveryService};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};

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
            commands::presentation::get_status,
            commands::presentation::get_notes_zoom,
            commands::presentation::next_slide,
            commands::presentation::prev_slide,
            commands::presentation::goto_slide,
            commands::presentation::fetch_all_notes,
            commands::presentation::get_all_notes,
            commands::presentation::start_notes_scan,
            commands::presentation::stop_notes_scan,
            commands::presentation::save_text_file,
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
            commands::webserver::page_notes,
            // Captions
            commands::captions::list_audio_input_devices,
            commands::captions::start_captions,
            commands::captions::stop_captions,
            commands::captions::is_captions_running,
            commands::captions::get_caption_status,
            commands::captions::get_captions_url,
            commands::captions::preview_caption_overlay,
            commands::config::get_outputs_status,
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

            // One StateManager owns presentation state for the app's lifetime;
            // OSC, the web server and the UI all go through it.
            {
                let state = app.state::<AppState>();
                let sm = &state.state_manager;
                sm.attach(app.handle().clone());
                sm.set_target(
                    config.adapter.clone(),
                    config.presentation_name.clone(),
                    config.adapter_config.clone(),
                );
                sm.start_polling(2000);
            }

            // Sync the persisted overlay styling into the live channel the
            // web server reads from, so the overlay reflects it from the
            // first page load rather than a hardcoded default.
            {
                let state = app.state::<AppState>();
                state.captions.set_overlay(&config.captions);
                tauri::async_runtime::spawn(captions::run_silence_clear(state.captions.clone()));

                // Native outputs run independently of the caption engine and
                // the presentation, so a receiver stays wired up between talks.
                let sources = state.output_sources();
                state
                    .outputs
                    .locked()
                    .reconcile(app.handle(), &sources, &config);
            }

            // Auto-start OSC server
            let app_handle = app.handle().clone();
            let osc_config = config.osc.clone();
            tauri::async_runtime::spawn(async move {
                log::info!(
                    "Auto-starting OSC server on port {}",
                    osc_config.receive_port
                );
                let state = app_handle.state::<AppState>();
                match state.start_osc_server(osc_config).await {
                    Ok(()) => {
                        let _ = app_handle.emit("osc-server-started", ());
                        log::info!("OSC server auto-started successfully");
                    }
                    Err(e) => log::error!("Failed to auto-start OSC server: {}", e),
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
                            let mut discovery_slot = state.discovery_service.locked();
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

                tauri::async_runtime::spawn(async move {
                    log::info!(
                        "Auto-starting web server on port {}",
                        web_server_config.port
                    );

                    let state = app_handle3.state::<AppState>();
                    match state.start_web_server(web_server_config).await {
                        Ok(()) => log::info!("Web server auto-started successfully"),
                        Err(e) => log::error!("Failed to auto-start web server: {}", e),
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
                    if let Err(e) = state.start_captions(&app_handle4, &captions_config) {
                        log::error!("Failed to auto-start captions: {}", e);
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

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or("Failed to get default window icon")?;

    let _tray = TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        // Menu on either click, the macOS menu-bar convention. "Show" in the
        // menu covers what a left click used to do directly.
        .show_menu_on_left_click(true)
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
        .build(app)?;

    Ok(())
}
