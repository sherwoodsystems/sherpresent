//! # SherPresent Bridge — Tauri GUI entry point
//!
//! Single-crate bridge app. The core engine (USB clicker capture, OSC
//! sending/receiving, mDNS discovery, config) lives in the `bridge` module;
//! this file adds the native webview UI and event forwarding via Tauri.

mod bridge;
mod commands;
mod state;

use crate::bridge::BridgeCore;
use tauri::{Emitter, Manager};

use crate::state::BridgeState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(BridgeState::default())
        .invoke_handler(tauri::generate_handler![
            commands::discovery::get_discovered_peers,
            commands::discovery::set_instance_name,
            commands::discovery::stop_discovery,
            commands::config::get_config,
            commands::config::save_config,
            commands::app::get_bridge_info,
            commands::feedback::get_feedback_state,
            commands::feedback::send_test_osc,
            commands::usb::get_usb_devices,
            commands::usb::get_usb_permission_status,
            commands::usb::start_usb_registration,
            commands::usb::cancel_usb_registration,
            commands::usb::confirm_usb_binding,
            commands::usb::remove_usb_binding,
            commands::usb::set_device_target,
        ])
        .setup(|app| {
            let app_handle = app.handle().clone();
            let state = app.state::<BridgeState>();

            // Create the bridge core. This loads config and prepares shared state
            // but does not yet spawn background tasks.
            let core = match BridgeCore::new(None) {
                Ok(core) => {
                    *state.core.lock().unwrap() = Some(core.clone());
                    core
                }
                Err(e) => {
                    log::error!("Failed to initialize bridge core: {e}");
                    return Err(e.into());
                }
            };

            // Start the core services in Tauri's async runtime.
            let app_handle_start = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = core.start().await {
                    log::error!("Failed to start bridge core: {e}");
                    return;
                }

                // Forward core events to Tauri frontend events.
                let mut events = core.subscribe();
                let app_handle_events = app_handle_start.clone();
                tauri::async_runtime::spawn(async move {
                    while let Ok(event) = events.recv().await {
                        match event {
                            crate::bridge::BridgeEvent::UsbConnected(info) => {
                                let _ = app_handle_events.emit("usb-connected", &info);
                            }
                            crate::bridge::BridgeEvent::UsbDisconnected {
                                device_id,
                            } => {
                                let _ = app_handle_events.emit("usb-disconnected", &device_id);
                            }
                            crate::bridge::BridgeEvent::UsbAccessDenied { count } => {
                                let _ = app_handle_events.emit("usb-access-denied", count);
                            }
                            crate::bridge::BridgeEvent::RegistrationDetected {
                                device_id,
                                action,
                                key,
                            } => {
                                let _ = app_handle_events.emit(
                                    "usb-registration-detected",
                                    &serde_json::json!({
                                        "deviceId": device_id,
                                        "action": action,
                                        "key": key,
                                    }),
                                );
                            }
                        }
                    }
                });

                // Forward mDNS peer updates to Tauri frontend events.
                let mut peer_rx = core.subscribe_peers();
                let app_handle_peers = app_handle_start.clone();
                tauri::async_runtime::spawn(async move {
                    while let Ok(update) = peer_rx.recv().await {
                        let _ = app_handle_peers.emit("peers-updated", &update.peers);
                    }
                });

                // Forward OSC feedback updates to Tauri frontend events.
                let mut feedback_rx = core.subscribe_feedback();
                tauri::async_runtime::spawn(async move {
                    while let Ok(update) = feedback_rx.recv().await {
                        let _ = app_handle_start.emit("feedback-updated", &update.state);
                    }
                });
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building SherPresent Bridge")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                log::info!("Bridge exit requested; shutting down background services");
                let core = {
                    let state = app_handle.state::<BridgeState>();
                    let core = state.core.lock().unwrap().take();
                    core
                };
                if let Some(core) = core {
                    core.shutdown();
                }
            }
        });
}
