use sherpresent_core::DiscoveryService;
use crate::adapters::canva::CanvaAdapter;
use crate::adapters::LiveStatus;
use crate::captions::{CaptionEngine, CaptionSinks};
use crate::config::{AdapterConfig, CaptionsConfig, WebServerConfig};
use crate::osc::{LatencyStore, OscServerHandle, ScrollDirection, StateManager};
use crate::output::feed::NotesSources;
use crate::output::{OutputSources, Outputs};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Application state
///
/// ## Thread Safety
///
/// This struct is shared across Tauri commands (which may run concurrently).
/// Each field is wrapped in appropriate synchronization primitives:
/// - `Mutex` for exclusive access to mutable data
/// - `Arc` for shared ownership across tasks
pub struct AppState {
    /// Flag to control status polling
    pub polling_active: Arc<Mutex<bool>>,

    /// Handle to the running OSC server (if any)
    pub osc_server: Mutex<Option<OscServerHandle>>,

    /// Shared StateManager for presentation control (used by OSC server and WebSocket)
    pub state_manager: Arc<Mutex<Option<Arc<StateManager>>>>,

    /// Discovery service for mDNS peer discovery
    ///
    /// Active when channel sync is enabled.
    pub discovery_service: Mutex<Option<DiscoveryService>>,

    /// Per-adapter network configuration
    pub adapter_config: Arc<Mutex<AdapterConfig>>,

    /// Canva adapter singleton (long-lived, holds webview reference)
    pub canva_adapter: Arc<Mutex<Option<CanvaAdapter>>>,

    /// Cache of presenter notes keyed by slide number (1-indexed)
    /// Session-scoped: cleared on adapter/presentation change
    pub notes_cache: Arc<Mutex<HashMap<i32, String>>>,

    /// Whether a notes scan is currently in progress (for cancellation)
    pub notes_scan_active: Arc<Mutex<bool>>,

    /// Broadcast channel for live status updates (consumed by web server SSE)
    pub status_broadcast: tokio::sync::broadcast::Sender<LiveStatus>,

    /// Broadcast channel for notes cache updates (consumed by web server SSE)
    pub notes_broadcast: tokio::sync::broadcast::Sender<HashMap<i32, String>>,

    /// Handle to the running web server (if any)
    pub web_server_handle: Mutex<Option<crate::webserver::WebServerHandle>>,

    /// Latency measurement ring buffer
    pub latency_store: Arc<LatencyStore>,

    /// Timestamp (Unix ms) of the last UI/OSC slide command.
    /// Used by polling to skip cycles during active use, avoiding IPC contention.
    pub last_command_at: Arc<Mutex<u64>>,

    /// Broadcast channel for scroll commands (consumed by web server WebSocket)
    pub scroll_broadcast: tokio::sync::broadcast::Sender<ScrollDirection>,

    /// Handle to the running caption engine (if any)
    pub caption_engine: Mutex<Option<CaptionEngine>>,

    /// Caption fan-out: overlay broadcast, replay buffer, engine status, and
    /// live overlay styling (synced from config at startup, on every save, and
    /// on every preview tick from Settings).
    pub captions: CaptionSinks,

    /// Native video outputs (Syphon captions and notes), reconciled against config
    pub outputs: Mutex<Outputs>,
}

impl AppState {
    /// Start the caption engine. Shared by the Start button and auto-start.
    pub fn start_captions(&self, app: &tauri::AppHandle, config: &CaptionsConfig) -> Result<(), String> {
        let mut slot = self.caption_engine.lock().unwrap();
        if slot.is_some() {
            return Err("Captions are already running".to_string());
        }
        // Clear stale lines so a new session doesn't open with the last one's text.
        self.captions.buffer.lock().unwrap().clear();
        *slot = Some(crate::captions::start(app.clone(), config, self.captions.clone())?);
        Ok(())
    }

    /// Start the LAN web server. Shared by the Start button and auto-start.
    pub async fn start_web_server(&self, config: WebServerConfig) -> Result<(), String> {
        if self.web_server_handle.lock().unwrap().is_some() {
            return Err("Web server is already running".to_string());
        }
        let state_manager = self.state_manager.lock().unwrap().clone();
        let handle = crate::webserver::start(
            config,
            self.notes_cache.clone(),
            self.status_broadcast.clone(),
            self.notes_broadcast.clone(),
            self.scroll_broadcast.clone(),
            state_manager,
            self.captions.clone(),
        )
        .await?;
        *self.web_server_handle.lock().unwrap() = Some(handle);
        Ok(())
    }

    /// Everything the native video outputs are fed from.
    pub fn output_sources(&self) -> OutputSources {
        OutputSources {
            captions: self.captions.clone(),
            notes: NotesSources {
                notes: self.notes_cache.clone(),
                notes_broadcast: self.notes_broadcast.clone(),
                status_broadcast: self.status_broadcast.clone(),
                state_manager: self.state_manager.clone(),
            },
        }
    }
}

// We need to implement Default manually because OscServerHandle doesn't implement Default
impl Default for AppState {
    fn default() -> Self {
        Self {
            polling_active: Arc::new(Mutex::new(false)),
            osc_server: Mutex::new(None),
            state_manager: Arc::new(Mutex::new(None)),
            discovery_service: Mutex::new(None),
            adapter_config: Arc::new(Mutex::new(AdapterConfig::default())),
            canva_adapter: Arc::new(Mutex::new(None)),
            notes_cache: Arc::new(Mutex::new(HashMap::new())),
            notes_scan_active: Arc::new(Mutex::new(false)),
            status_broadcast: tokio::sync::broadcast::channel(64).0,
            notes_broadcast: tokio::sync::broadcast::channel(64).0,
            web_server_handle: Mutex::new(None),
            latency_store: Arc::new(LatencyStore::new(50)),
            last_command_at: Arc::new(Mutex::new(0)),
            scroll_broadcast: tokio::sync::broadcast::channel(16).0,
            caption_engine: Mutex::new(None),
            captions: CaptionSinks::default(),
            outputs: Mutex::new(Outputs::default()),
        }
    }
}
