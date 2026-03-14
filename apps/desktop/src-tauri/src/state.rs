use crate::adapters::canva::CanvaAdapter;
use crate::config::AdapterConfig;
use crate::discovery::{DiscoveredPeer, DiscoveryService};
use crate::osc::OscServerHandle;
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
    ///
    /// We store the handle so we can stop the server later.
    /// `Option` because the server might not be running.
    pub osc_server: Mutex<Option<OscServerHandle>>,

    /// Discovery service for mDNS peer discovery
    ///
    /// Active when channel sync is enabled.
    pub discovery_service: Mutex<Option<DiscoveryService>>,

    /// Peers discovered via OSC command sources (e.g., rpi-osc-bridge devices)
    ///
    /// Tracks devices by their source IP when they send OSC commands.
    /// Key is the source address string (e.g., "192.168.1.100:9002")
    /// Wrapped in Arc for sharing across async tasks.
    pub command_source_peers: Arc<Mutex<HashMap<String, DiscoveredPeer>>>,

    /// Counter for assigning display IDs to command source peers
    pub next_peer_display_id: Arc<Mutex<u8>>,

    /// Per-adapter network configuration
    pub adapter_config: Arc<Mutex<AdapterConfig>>,

    /// Canva adapter singleton (long-lived, holds webview reference)
    pub canva_adapter: Arc<Mutex<Option<CanvaAdapter>>>,

    /// Cache of presenter notes keyed by slide number (1-indexed)
    /// Session-scoped: cleared on adapter/presentation change
    pub notes_cache: Arc<Mutex<HashMap<i32, String>>>,

    /// Whether a notes scan is currently in progress (for cancellation)
    pub notes_scan_active: Arc<Mutex<bool>>,
}

// We need to implement Default manually because OscServerHandle doesn't implement Default
impl Default for AppState {
    fn default() -> Self {
        Self {
            polling_active: Arc::new(Mutex::new(false)),
            osc_server: Mutex::new(None),
            discovery_service: Mutex::new(None),
            command_source_peers: Arc::new(Mutex::new(HashMap::new())),
            next_peer_display_id: Arc::new(Mutex::new(100)), // Start at 100 for command source peers
            adapter_config: Arc::new(Mutex::new(AdapterConfig::default())),
            canva_adapter: Arc::new(Mutex::new(None)),
            notes_cache: Arc::new(Mutex::new(HashMap::new())),
            notes_scan_active: Arc::new(Mutex::new(false)),
        }
    }
}
