//! # Bridge State
//!
//! Cross-command shared state for the SherPresent Bridge app.
//!
//! Thread safety mirror's the desktop app's pattern:
//! - `Arc<Mutex<Option<T>>>` for replaceable long-lived services (e.g. discovery)
//! - `Arc<Mutex<T>>` for shared mutable state (e.g. config)
//! - `tokio::sync::broadcast::Sender` for fan-out to UI / web config API

use sherpresent_core::DiscoveryService;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

use crate::config::BridgeConfig;
use crate::osc::feedback::{FeedbackListenerHandle, FeedbackState, FeedbackUpdate};
use crate::usb::UsbManager;

/// Application state shared across Tauri commands.
pub struct BridgeState {
    /// mDNS discovery service — registers this bridge as `_sher-present._udp.local.`
    /// with `version=bridge` and `config_port=...`, and browses for desktop peers.
    pub discovery_service: Arc<Mutex<Option<DiscoveryService>>>,

    /// Loaded config. Persisted to disk on every mutation via Tauri commands.
    pub config: Arc<Mutex<BridgeConfig>>,

    /// Latest OSC feedback state received from desktops
    /// (`/oscpoint/slideshow/*`, `/oscpoint/state/*` etc.).
    pub feedback_state: Arc<Mutex<FeedbackState>>,

    /// Broadcast channel — emits a `FeedbackUpdate` snapshot every time a
    /// new oscpoint feedback message updates the cache.
    pub feedback_tx: broadcast::Sender<FeedbackUpdate>,

    /// Background task handle for the UDP OSC feedback listener.
    /// Stashed here so we can shut it down on app exit.
    pub feedback_listener: Mutex<Option<FeedbackListenerHandle>>,

    /// Background task handle for the HTTP config API server.
    pub http_server: Mutex<Option<crate::http::HttpServerHandle>>,

    /// USB clicker manager (Linux only; None on other platforms or if init failed).
    pub usb_manager: Arc<Mutex<Option<UsbManager>>>,

    /// When `Some(action)` (`"next"`/`"prev"`), the next key-up from any device
    /// is offered for binding to that action.
    pub usb_registration_mode: Arc<tokio::sync::Mutex<Option<String>>>,
}

impl Default for BridgeState {
    fn default() -> Self {
        let (feedback_tx, _) = broadcast::channel::<FeedbackUpdate>(64);
        Self {
            discovery_service: Arc::new(Mutex::new(None)),
            config: Arc::new(Mutex::new(BridgeConfig::default())),
            feedback_state: Arc::new(Mutex::new(FeedbackState::default())),
            feedback_tx,
            feedback_listener: Mutex::new(None),
            http_server: Mutex::new(None),
            usb_manager: Arc::new(Mutex::new(None)),
            usb_registration_mode: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }
}

/// Subset of [`BridgeState`] exposed to the HTTP API server.
///
/// The HTTP server runs in a background tokio task and cannot hold a
/// `tauri::State` reference, so it gets a cheap-to-clone struct containing
/// just the shared `Arc<Mutex<...>>` pieces.
#[derive(Clone)]
pub struct ApiState {
    pub config: Arc<Mutex<BridgeConfig>>,
    pub feedback_state: Arc<Mutex<FeedbackState>>,
    pub discovery_service: Arc<Mutex<Option<DiscoveryService>>>,
}

impl ApiState {
    pub fn from_bridge(state: &BridgeState) -> Self {
        Self {
            config: state.config.clone(),
            feedback_state: state.feedback_state.clone(),
            discovery_service: state.discovery_service.clone(),
        }
    }
}