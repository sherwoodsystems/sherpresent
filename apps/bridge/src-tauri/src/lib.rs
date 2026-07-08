//! # SherPresent Bridge — Tauri entry point
//!
//! Replaces the Python `rpi-osc-bridge` project with a cross-platform Tauri
//! app. Phase 1a wires up:
//! - mDNS announcement (`_sher-present._udp.local.`) with `version=bridge` and
//!   `config_port=<port>` TXT records so desktop instances discover this
//!   bridge and can deep-link into its config UI.
//! - mDNS browsing for desktop peers (`DiscoveredPeer`).
//! - OSC feedback listener (UDP) caching `/oscpoint/*` state from desktops.
//! - Module layout for config, OSC, and (eventually) USB HID detection.

mod commands;
mod config;
mod http;
mod osc;
mod state;
mod usb;

use sherpresent_core::{DiscoveredPeer, DiscoveryService};
use tauri::{Emitter, Manager};
use std::sync::Arc;
use crate::http::start_http_server;
use crate::osc::feedback;
use crate::osc::sender::OscSender;
use crate::state::BridgeState;
use crate::config::BridgeConfig;
use crate::usb::{UsbEvent, UsbManager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Logging uses `RUST_LOG` env var with sensible info-level defaults.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(BridgeState::default())
        .invoke_handler(tauri::generate_handler![
            // Discovery
            commands::discovery::get_discovered_peers,
            commands::discovery::set_instance_name,
            commands::discovery::stop_discovery,
            // Config
            commands::config::get_config,
            commands::config::save_config,
            // App
            commands::app::get_bridge_info,
            // Feedback + OSC test
            commands::feedback::get_feedback_state,
            commands::feedback::send_test_osc,
            // USB clickers
            commands::usb::get_usb_devices,
            commands::usb::start_usb_registration,
            commands::usb::cancel_usb_registration,
            commands::usb::confirm_usb_registration,
            commands::usb::set_device_target,
        ])
        .setup(|app| {
            // 1. Load (or create) the bridge config.
            let config: BridgeConfig = match config::load_config(app.handle()) {
                Ok(c) => c,
                Err(e) => {
                    log::error!("Failed to load bridge config: {e}. Using defaults.");
                    BridgeConfig::default()
                }
            };
            log::info!(
                "Bridge starting — id={}, name={:?}, mode={:?}, feedback_port={}, config_port={}",
                config.bridge_id,
                config.bridge_name,
                config.mode,
                config.feedback_port,
                config.config_port
            );

            // Stash config into BridgeState so commands can read/write it.
            {
                let state = app.state::<BridgeState>();
                let mut cfg_slot = state.config.lock().unwrap();
                *cfg_slot = config.clone();
            }

            // ---------------------------------------------------------------
            // 2. Auto-start the OSC feedback listener (UDP port 9001).
            // ---------------------------------------------------------------
            {
                let state = app.state::<BridgeState>();
                let fb_state = state.feedback_state.clone();
                let fb_tx = state.feedback_tx.clone();
                match feedback::start_feedback_listener(config.feedback_port, fb_state, fb_tx) {
                    Ok(handle) => {
                        let mut slot = state.feedback_listener.lock().unwrap();
                        *slot = Some(handle);
                    }
                    Err(e) => {
                        log::error!("Failed to start OSC feedback listener on port {}: {e}", config.feedback_port);
                    }
                }
            }

            // Forward feedback updates to the Svelte frontend as Tauri events.
            {
                let app_handle_fb = app.handle().clone();
                let state = app.state::<BridgeState>();
                let mut fb_rx = state.feedback_tx.subscribe();
                tauri::async_runtime::spawn(async move {
                    while let Ok(update) = fb_rx.recv().await {
                        let _ = app_handle_fb.emit("feedback-updated", &update.state);
                    }
                });
            }

            // ---------------------------------------------------------------
            // 3. Start the USB HID clicker manager and coordinator.
            // ---------------------------------------------------------------
            #[cfg(target_os = "linux")]
            {
                let state = app.state::<BridgeState>();
                match UsbManager::new() {
                    Ok(manager) => {
                        let mut usb_slot = state.usb_manager.lock().unwrap();
                        *usb_slot = Some(manager);
                        log::info!("USB HID clicker manager started");
                    }
                    Err(e) => {
                        log::error!("Failed to start USB HID manager: {e}");
                    }
                }

                // Coordinator: translate USB key-up events into OSC commands
                // or registration-detection notifications.
                let state = app.state::<BridgeState>();
                let manager = state.usb_manager.lock().unwrap().clone();
                let config = state.config.clone();
                let registration_mode = state.usb_registration_mode.clone();
                if let Some(manager) = manager {
                    let mut usb_rx = manager.subscribe();
                    let app_handle_coord = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        while let Ok(event) = usb_rx.recv().await {
                            match event {
                                UsbEvent::Connected(info) => {
                                    log::info!("USB connected: {} ({})", info.name, info.id);
                                    let _ = app_handle_coord.emit("usb-connected", &info);
                                }
                                UsbEvent::Disconnected { device_id } => {
                                    log::info!("USB disconnected: {device_id}");
                                    let _ = app_handle_coord.emit("usb-disconnected", &device_id);
                                }
                                UsbEvent::KeyUp { device_id, key } => {
                                    // Registration mode: any clicker key-up from an
                                    // unregistered device triggers registration detection.
                                    if let Some(slot) = registration_mode.lock().await.clone() {
                                        let _ = app_handle_coord.emit(
                                            "usb-registration-detected",
                                            &serde_json::json!({"deviceId": device_id, "slot": slot, "key": key }),
                                        );
                                        continue;
                                    }

                                    // Normal mode: send OSC if the device is registered.
                                    let target = {
                                        let cfg = config.lock().unwrap();
                                        cfg.devices
                                            .get(&device_id)
                                            .and_then(|d| d.as_ref())
                                            .and_then(|d| d.target.clone())
                                    };
                                    if let Some(target) = target {
                                        let next = matches!(key.as_str(), "KEY_RIGHT" | "KEY_PAGEDOWN");
                                        let prev = matches!(key.as_str(), "KEY_LEFT" | "KEY_PAGEUP");
                                        if next || prev {
                                            let device_id_log = device_id.clone();
                                            tokio::spawn(async move {
                                                match OscSender::new(&target.host, target.port) {
                                                    Ok(sender) => {
                                                        if next {
                                                            sender.send_next().await;
                                                        } else {
                                                            sender.send_prev().await;
                                                        }
                                                    }
                                                    Err(e) => {
                                                        log::warn!("OSC send failed for {device_id_log}: {e}");
                                                    }
                                                }
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    });
                }
            }

            // ---------------------------------------------------------------
            // 4. Start the HTTP config API server.
            // ---------------------------------------------------------------
            {
                let state = app.state::<BridgeState>();
                let api_state = Arc::new(crate::state::ApiState::from_bridge(&state));
                let cfg_port = config.config_port;
                let app_handle_http = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    match start_http_server(cfg_port, api_state).await {
                        Ok(handle) => {
                            let state = app_handle_http.state::<BridgeState>();
                            let mut slot = state.http_server.lock().unwrap();
                            *slot = Some(handle);
                            log::info!("HTTP config API server listening on port {cfg_port}");
                        }
                        Err(e) => {
                            log::error!("Failed to start HTTP config API server on port {cfg_port}: {e}");
                        }
                    }
                });
            }

            // ---------------------------------------------------------------
            // 4. Auto-start the mDNS discovery service.
            //    Register as `version=bridge` and advertise `config_port` so
            //    the desktop app's Bridges page can find us.
            // ---------------------------------------------------------------
            let app_handle = app.handle().clone();
            let bridge_id = config.bridge_id.to_string();
            let bridge_name = Some(config.bridge_name.clone());
            let feedback_port = config.feedback_port;
            let config_port_str = config.config_port.to_string();

            tauri::async_runtime::spawn(async move {
                log::info!("Auto-starting bridge mDNS service");

                let (peer_tx, mut peer_rx) =
                    tokio::sync::mpsc::channel::<Vec<DiscoveredPeer>>(32);

                let service_result = DiscoveryService::new(
                    bridge_id.clone(),
                    bridge_name.clone(),
                    feedback_port,
                    peer_tx,
                    None, // auto-select interface
                )
                .map(|service| {
                    service
                        .with_version("bridge")
                        .with_property("channel", "bridge")
                        .with_property("config_port", &config_port_str)
                });

                match service_result {
                    Ok(mut service) => {
                        if let Err(e) = service.register() {
                            log::error!("Failed to register bridge mDNS service: {e}");
                            return;
                        }
                        if let Err(e) = service.start_browsing() {
                            log::error!("Failed to start mDNS browsing: {e}");
                            return;
                        }

                        {
                            let state = app_handle.state::<BridgeState>();
                            let mut discovery_slot =
                                state.discovery_service.lock().unwrap();
                            *discovery_slot = Some(service);
                            log::info!(
                                "Bridge mDNS service registered as version=bridge, \
                                 config_port={config_port_str}, feedback_port={feedback_port}"
                            );
                        }

                        // Forward peer updates to the frontend.
                        while let Some(peers) = peer_rx.recv().await {
                            let _ = app_handle.emit("peers-updated", &peers);
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to create bridge mDNS service: {e}");
                    }
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building SherPresent Bridge")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                log::info!("Bridge exit requested; shutting down background services");
                let state = app_handle.state::<BridgeState>();

                if let Some(handle) = state.feedback_listener.lock().unwrap().take() {
                    let _ = handle.cancel.send(());
                }

                if let Some(handle) = state.http_server.lock().unwrap().take() {
                    let _ = handle.cancel.send(());
                }

                if let Some(mut service) = state.discovery_service.lock().unwrap().take() {
                    if let Err(e) = service.shutdown() {
                        log::warn!("Failed to shut down mDNS discovery service: {e}");
                    }
                }

                // Drop the USB manager so its event readers are released.
                let _ = state.usb_manager.lock().unwrap().take();
            }
        });
}