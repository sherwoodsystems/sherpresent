use sherpresent_core::DiscoveryService;
use crate::adapters::canva::CanvaAdapter;
use crate::adapters::LiveStatus;
use crate::captions::{
    CaptionEngine, CaptionSegment, CaptionSinks, CaptionStatus, CaptionUpdate,
};
use crate::config::{default_caption_font_size, AdapterConfig};
use crate::osc::{LatencyStore, OscServerHandle, ScrollDirection, StateManager};
use std::collections::{HashMap, VecDeque};
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

    /// Broadcast channel for caption updates (consumed by the overlay page)
    pub caption_broadcast: tokio::sync::broadcast::Sender<CaptionUpdate>,

    /// Recent finalized caption lines, replayed to late-joining overlays
    pub caption_buffer: Arc<Mutex<VecDeque<CaptionSegment>>>,

    /// Latest caption engine status, for the REST/initial-WS snapshot
    pub caption_status: Arc<Mutex<CaptionStatus>>,

    /// Live overlay font size, synced from config at startup and on every
    /// save; see `CaptionSinks::font_size` for why this is a `watch` channel.
    pub caption_font_size: tokio::sync::watch::Sender<u16>,
}

impl AppState {
    /// Bundle the caption publishing handles for the engine.
    pub fn caption_sinks(&self) -> CaptionSinks {
        CaptionSinks {
            broadcast: self.caption_broadcast.clone(),
            buffer: self.caption_buffer.clone(),
            status: self.caption_status.clone(),
            font_size: self.caption_font_size.clone(),
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
            caption_broadcast: tokio::sync::broadcast::channel(64).0,
            caption_buffer: Arc::new(Mutex::new(VecDeque::new())),
            caption_status: Arc::new(Mutex::new(CaptionStatus::default())),
            caption_font_size: tokio::sync::watch::channel(default_caption_font_size()).0,
        }
    }
}
