//! # Bridge core
//!
//! The bridge's engine: USB clicker detection, OSC sending/receiving, mDNS
//! discovery, and the coordinator that translates key-ups into OSC commands.
//! Driven by the Tauri layer in the parent crate.

pub mod config;
pub mod osc;
pub mod state;
pub mod usb;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use sherpresent_core::{DiscoveredPeer, DiscoveryService};
use tokio::sync::broadcast;

use crate::bridge::config::{
    load_config, save_config, BridgeConfig, DeviceConfig, DeviceTarget, KeyAction,
};
use crate::bridge::osc::feedback::{start_feedback_listener, FeedbackState, FeedbackUpdate};
use crate::bridge::osc::sender::OscSender;
use crate::bridge::state::CoreState;
use crate::bridge::usb::{UsbDeviceInfo, UsbEvent, UsbManager};

/// Event emitted by the bridge core for UI consumers.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BridgeEvent {
    UsbConnected(UsbDeviceInfo),
    UsbDisconnected { device_id: String },
    UsbAccessDenied { count: usize },
    RegistrationDetected {
        device_id: String,
        action: String,
        key: String,
    },
}

/// Peer list update broadcast to UI consumers.
#[derive(Debug, Clone)]
pub struct PeerUpdate {
    pub peers: Vec<DiscoveredPeer>,
}

/// The central bridge coordinator. Owns all background services and shared state.
#[derive(Clone)]
pub struct BridgeCore {
    state: Arc<CoreState>,
    config_dir: Option<PathBuf>,
    events: broadcast::Sender<BridgeEvent>,
    peer_tx: broadcast::Sender<PeerUpdate>,
}

impl BridgeCore {
    /// Load config and create the core. Does not start background services yet.
    pub fn new(config_dir: Option<PathBuf>) -> Result<Self, String> {
        let config = load_config(config_dir.as_deref())?;
        log::info!(
            "Bridge core initialized — id={}, name={:?}, mode={:?}, feedback_port={}, config_port={}",
            config.bridge_id,
            config.bridge_name,
            config.mode,
            config.feedback_port,
            config.config_port
        );

        let state = Arc::new(CoreState {
            config: Arc::new(Mutex::new(config)),
            ..CoreState::default()
        });

        let (events, _) = broadcast::channel::<BridgeEvent>(256);
        let (peer_tx, _) = broadcast::channel::<PeerUpdate>(32);

        Ok(Self {
            state,
            config_dir,
            events,
            peer_tx,
        })
    }

    /// Subscribe to bridge events (USB connect/disconnect/access-denied,
    /// registration detected, etc.).
    pub fn subscribe(&self) -> broadcast::Receiver<BridgeEvent> {
        self.events.subscribe()
    }

    /// Subscribe to peer list updates from mDNS discovery.
    pub fn subscribe_peers(&self) -> broadcast::Receiver<PeerUpdate> {
        self.peer_tx.subscribe()
    }

    /// Access the underlying shared state.
    pub fn state(&self) -> Arc<CoreState> {
        self.state.clone()
    }

    /// Get a snapshot of the current config.
    pub fn config(&self) -> BridgeConfig {
        self.state.config.lock().unwrap().clone()
    }

    /// Save config and update the in-memory copy.
    pub fn save_config(&self, config: &BridgeConfig) -> Result<(), String> {
        save_config(self.config_dir.as_deref(), config)?;
        *self.state.config.lock().unwrap() = config.clone();
        Ok(())
    }

    /// Start all background services: feedback listener, USB manager (Linux),
    /// and mDNS discovery. Spawns the coordinator task.
    pub async fn start(&self) -> Result<(), String> {
        let config = self.config();

        // Start feedback listener.
        self.start_feedback_listener(config.feedback_port);

        // Start USB manager and coordinator (Linux only).
        #[cfg(target_os = "linux")]
        self.start_usb_coordinator().await;

        // Start mDNS discovery service.
        self.start_discovery(config).await;

        Ok(())
    }

    fn start_feedback_listener(&self, port: u16) {
        let state = self.state.clone();
        match start_feedback_listener(port, state.feedback_state.clone(), state.feedback_tx.clone()) {
            Ok(handle) => {
                *state.feedback_listener.lock().unwrap() = Some(handle);
            }
            Err(e) => {
                log::error!("Failed to start OSC feedback listener on port {port}: {e}");
            }
        }
    }

    #[cfg(target_os = "linux")]
    async fn start_usb_coordinator(&self) {
        let state = self.state.clone();
        match UsbManager::new() {
            Ok(manager) => {
                *state.usb_manager.lock().unwrap() = Some(manager);
                log::info!("USB HID clicker manager started");
            }
            Err(e) => {
                log::error!("Failed to start USB HID manager: {e}");
                return;
            }
        }

        let manager = state.usb_manager.lock().unwrap().clone().unwrap();
        let mut usb_rx = manager.subscribe();
        let events = self.events.clone();
        tokio::spawn(async move {
            while let Ok(event) = usb_rx.recv().await {
                handle_usb_event(event, &state, &events).await;
            }
        });
    }

    async fn start_discovery(&self, config: BridgeConfig) {
        let state = self.state.clone();
        let peer_tx = self.peer_tx.clone();
        let bridge_id = config.bridge_id.to_string();
        let bridge_name = Some(config.bridge_name.clone());
        let feedback_port = config.feedback_port;
        let config_port_str = config.config_port.to_string();

        tokio::spawn(async move {
            log::info!("Auto-starting bridge mDNS service");

            let (mdns_peer_tx, mut peer_rx) = tokio::sync::mpsc::channel::<Vec<DiscoveredPeer>>(32);

            let service_result = DiscoveryService::new(
                bridge_id.clone(),
                bridge_name.clone(),
                feedback_port,
                mdns_peer_tx,
                None,
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
                        let mut discovery_slot = state.discovery_service.lock().unwrap();
                        *discovery_slot = Some(service);
                        log::info!(
                            "Bridge mDNS service registered as version=bridge, \
                             config_port={config_port_str}, feedback_port={feedback_port}"
                        );
                    }

                    while let Some(peers) = peer_rx.recv().await {
                        let _ = peer_tx.send(PeerUpdate { peers: peers.clone() });
                    }
                }
                Err(e) => {
                    log::error!("Failed to create bridge mDNS service: {e}");
                }
            }
        });
    }

    // -------------------------------------------------------------------------
    // Commands used by both Tauri handlers and the HTTP/headless interface
    // -------------------------------------------------------------------------

    /// Currently connected USB clicker devices.
    pub async fn usb_devices(&self) -> Vec<UsbDeviceInfo> {
        let manager = self.state.usb_manager.lock().unwrap().clone();
        match manager {
            Some(manager) => manager.devices().await,
            None => Vec::new(),
        }
    }

    /// Number of USB key-capable input devices that exist but can't be opened.
    pub fn usb_access_denied_count(&self) -> usize {
        self.state
            .usb_manager
            .lock()
            .unwrap()
            .clone()
            .map(|m| m.access_denied_count())
            .unwrap_or(0)
    }

    /// Enter registration mode for a presentation action.
    pub async fn start_usb_registration(&self, action: String) {
        *self.state.usb_registration_mode.lock().await = Some(action);
    }

    /// Cancel registration mode.
    pub async fn cancel_usb_registration(&self) {
        *self.state.usb_registration_mode.lock().await = None;
    }

    /// Bind a key on a device to an action and persist the config.
    pub async fn confirm_usb_binding(
        &self,
        device_id: String,
        device_name: String,
        key: String,
        action: String,
    ) -> Result<(), String> {
        let action: KeyAction = action.parse()?;

        // Clear registration mode first.
        *self.state.usb_registration_mode.lock().await = None;

        let mut config = self.config();
        let device = config
            .devices
            .entry(device_id.clone())
            .or_insert_with(|| DeviceConfig::new(device_name, device_id));
        device.bindings.insert(key, action);

        self.save_config(&config)
    }

    /// Remove a single key binding from a device.
    pub async fn remove_usb_binding(&self, device_id: String, key: String) -> Result<(), String> {
        let mut config = self.config();
        if let Some(device) = config.devices.get_mut(&device_id) {
            device.bindings.remove(&key);
            if device.bindings.is_empty() {
                config.devices.remove(&device_id);
            }
        }
        self.save_config(&config)
    }

    /// Assign (or clear) the OSC target for a registered device slot.
    pub async fn set_device_target(
        &self,
        device_id: String,
        target: Option<DeviceTarget>,
    ) -> Result<(), String> {
        let mut config = self.config();
        let device = config.devices.get_mut(&device_id).ok_or("device not found")?;
        device.target = target;
        self.save_config(&config)
    }

    /// Send a test OSC command to a target host:port.
    pub async fn send_test_osc(&self, host: String, port: u16, action: KeyAction) {
        let _ = tokio::spawn(async move {
            match OscSender::new(&host, port) {
                Ok(sender) => match action {
                    KeyAction::Next => sender.send_next().await,
                    KeyAction::Prev => sender.send_prev().await,
                },
                Err(e) => {
                    log::warn!("OSC test send failed for {host}:{port}: {e}");
                }
            }
        });
    }

    /// Update the human-readable bridge name and re-advertise via mDNS.
    pub fn set_bridge_name(&self, name: String) -> Result<(), String> {
        if name.trim().is_empty() {
            return Err("Bridge name cannot be empty".to_string());
        }
        let mut config = self.config();
        config.bridge_name = name.clone();
        self.save_config(&config)?;

        let mut discovery = self.state.discovery_service.lock().unwrap();
        if let Some(service) = discovery.as_mut() {
            service.update_display_name(Some(name))?;
        }
        Ok(())
    }

    /// Stop the mDNS discovery service.
    pub fn stop_discovery(&self) -> Result<(), String> {
        let mut discovery = self.state.discovery_service.lock().unwrap();
        if let Some(mut service) = discovery.take() {
            service.shutdown()?;
            log::info!("Discovery service stopped");
        }
        Ok(())
    }

    /// Get currently discovered desktop peers.
    pub fn discovered_peers(&self) -> Vec<DiscoveredPeer> {
        let discovery = self.state.discovery_service.lock().unwrap();
        match discovery.as_ref() {
            Some(s) => s.get_peers(),
            None => Vec::new(),
        }
    }

    /// Cached OSC feedback state.
    pub fn feedback_state(&self) -> FeedbackState {
        self.state.feedback_state.lock().unwrap().clone()
    }

    /// Subscribe to feedback updates.
    pub fn subscribe_feedback(&self) -> broadcast::Receiver<FeedbackUpdate> {
        self.state.feedback_tx.subscribe()
    }

    /// Shutdown background services.
    pub fn shutdown(&self) {
        if let Some(handle) = self.state.feedback_listener.lock().unwrap().take() {
            let _ = handle.cancel.send(());
        }
        if let Some(mut service) = self.state.discovery_service.lock().unwrap().take() {
            if let Err(e) = service.shutdown() {
                log::warn!("Failed to shut down mDNS discovery service: {e}");
            }
        }
        let _ = self.state.usb_manager.lock().unwrap().take();
    }
}

async fn handle_usb_event(
    event: UsbEvent,
    state: &Arc<CoreState>,
    events: &broadcast::Sender<BridgeEvent>,
) {
    match event {
        UsbEvent::Connected(info) => {
            log::info!("USB connected: {} ({})", info.name, info.id);
            let _ = events.send(BridgeEvent::UsbConnected(info));
        }
        UsbEvent::Disconnected { device_id } => {
            log::info!("USB disconnected: {device_id}");
            let _ = events.send(BridgeEvent::UsbDisconnected { device_id });
        }
        UsbEvent::AccessDenied { count } => {
            let _ = events.send(BridgeEvent::UsbAccessDenied { count });
        }
        UsbEvent::KeyUp { device_id, key } => {
            // Registration mode: any key-up from any device is offered as the key
            // to bind to the pending action.
            if let Some(action) = state.usb_registration_mode.lock().await.clone() {
                let _ = events.send(BridgeEvent::RegistrationDetected {
                    device_id,
                    action,
                    key,
                });
                return;
            }

            // Normal mode: look up this device's binding for the pressed key
            // and fire OSC to its target.
            let resolved = {
                let cfg = state.config.lock().unwrap();
                cfg.devices.get(&device_id).and_then(|d| {
                    let action = d.bindings.get(&key).copied()?;
                    let target = d.target.clone()?;
                    Some((action, target))
                })
            };
            if let Some((action, target)) = resolved {
                let device_id_log = device_id.clone();
                tokio::spawn(async move {
                    match OscSender::new(&target.host, target.port) {
                        Ok(sender) => match action {
                            KeyAction::Next => {
                                sender.send_next().await;
                            }
                            KeyAction::Prev => {
                                sender.send_prev().await;
                            }
                        },
                        Err(e) => {
                            log::warn!("OSC send failed for {device_id_log}: {e}");
                        }
                    }
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_event_serializes() {
        let ev = BridgeEvent::UsbAccessDenied { count: 1 };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("usbAccessDenied"));
    }
}
