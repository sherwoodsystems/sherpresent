use crate::adapters::canva::CanvaAdapter;
use crate::adapters::powerpoint_events::PowerPointEvents;
use crate::adapters::LiveStatus;
use crate::captions::{CaptionEngine, CaptionSinks};
use crate::config::{AppConfig, CaptionsConfig, OscConfig, WebServerConfig};
use crate::osc::state_manager::StateSinks;
use crate::osc::{LatencyStore, OscServer, OscServerHandle, ScrollDirection, StateManager};
use crate::output::feed::NotesSources;
use crate::output::{OutputSources, Outputs};
use crate::qlab::QLab;
use crate::util::LockExt;
use sherpresent_core::DiscoveryService;
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
    /// The running OSC server (if any). An async lock: reconciling holds it
    /// while the old server stops, so overlapping saves can't race for the port.
    pub osc_server: tokio::sync::Mutex<Option<RunningOsc>>,

    /// Owner of presentation state and the only poller. Every control surface
    /// (UI, OSC, web) sends commands through it.
    pub state_manager: Arc<StateManager>,

    /// Discovery service for mDNS peer discovery
    ///
    /// Active when channel sync is enabled.
    pub discovery_service: Mutex<Option<DiscoveryService>>,

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

    /// QLab count-out client and its latest state, reconciled against config
    pub qlab: QLab,

    /// Whether PowerPoint's slide show events are arriving (Windows only)
    pub powerpoint_events: Arc<PowerPointEvents>,
}

/// The OSC server and what it was started with.
pub struct RunningOsc {
    config: OscConfig,
    name: String,
    handle: OscServerHandle,
}

impl AppState {
    /// Start the caption engine. Shared by the Start button and auto-start.
    pub fn start_captions(
        &self,
        app: &tauri::AppHandle,
        config: &CaptionsConfig,
    ) -> Result<(), String> {
        let mut slot = self.caption_engine.locked();
        if slot.is_some() {
            return Err("Captions are already running".to_string());
        }
        // Clear stale lines so a new session doesn't open with the last one's text.
        self.captions.buffer.locked().clear();
        *slot = Some(crate::captions::start(
            app.clone(),
            config,
            self.captions.clone(),
        )?);
        Ok(())
    }

    /// Start the LAN web server. Shared by the Start button and auto-start.
    pub async fn start_web_server(&self, config: WebServerConfig) -> Result<(), String> {
        if self.web_server_handle.locked().is_some() {
            return Err("Web server is already running".to_string());
        }
        let handle = crate::webserver::start(config, self.output_sources()).await?;
        *self.web_server_handle.locked() = Some(handle);
        Ok(())
    }

    /// Run the OSC server as configured, restarting it only if its settings
    /// changed. Called on startup and on every config save.
    pub async fn reconcile_osc(&self, config: &AppConfig) -> Result<(), String> {
        let name = config
            .discovery
            .display_name
            .clone()
            .unwrap_or_else(sherpresent_core::machine_name);

        let mut slot = self.osc_server.lock().await;
        if let Some(running) = slot.as_ref() {
            if running.config == config.osc && running.name == name {
                return Ok(());
            }
        }
        if let Some(running) = slot.take() {
            running.handle.stop().await;
        }

        let handle = OscServer::new(config.osc.clone(), self.state_manager.clone())
            .with_scroll_broadcast(self.scroll_broadcast.clone())
            .with_name(name.clone())
            .start()
            .await
            .map_err(|e| format!("Failed to start OSC server: {}", e))?;
        *slot = Some(RunningOsc {
            config: config.osc.clone(),
            name,
            handle,
        });
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
                scroll_broadcast: self.scroll_broadcast.clone(),
                state_manager: self.state_manager.clone(),
            },
            qlab: self.qlab.subscribe(),
        }
    }
}

// We need to implement Default manually because OscServerHandle doesn't implement Default
impl Default for AppState {
    fn default() -> Self {
        let sinks = StateSinks::default();
        Self {
            osc_server: tokio::sync::Mutex::new(None),
            state_manager: Arc::new(StateManager::new(sinks.clone())),
            discovery_service: Mutex::new(None),
            canva_adapter: sinks.canva_adapter,
            notes_cache: sinks.notes,
            notes_scan_active: Arc::new(Mutex::new(false)),
            status_broadcast: sinks.status_broadcast,
            notes_broadcast: sinks.notes_broadcast,
            web_server_handle: Mutex::new(None),
            latency_store: sinks.latency_store,
            scroll_broadcast: tokio::sync::broadcast::channel(16).0,
            caption_engine: Mutex::new(None),
            captions: CaptionSinks::default(),
            outputs: Mutex::new(Outputs::default()),
            qlab: QLab::default(),
            powerpoint_events: Arc::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosc::{encoder, OscMessage, OscPacket};
    use std::time::Duration;
    use tokio::net::UdpSocket;

    async fn free_port() -> u16 {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        socket.local_addr().unwrap().port()
    }

    /// Whether an OSC server answers `/clicker/status` on this port.
    async fn answers(port: u16) -> bool {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        socket.connect(("127.0.0.1", port)).await.unwrap();
        let packet = OscPacket::Message(OscMessage {
            addr: "/clicker/status".to_string(),
            args: vec![],
        });
        let _ = socket.send(&encoder::encode(&packet).unwrap()).await;
        let mut buf = [0u8; 256];
        matches!(
            tokio::time::timeout(Duration::from_millis(500), socket.recv(&mut buf)).await,
            Ok(Ok(_))
        )
    }

    #[tokio::test]
    async fn changing_the_osc_port_moves_the_server_without_a_restart() {
        let state = AppState::default();
        let mut config = AppConfig::default();
        config.osc.host = "127.0.0.1".to_string();

        let first = free_port().await;
        config.osc.receive_port = first;
        state.reconcile_osc(&config).await.unwrap();
        assert!(answers(first).await);

        // An unrelated save leaves it running.
        state.reconcile_osc(&config).await.unwrap();
        assert!(answers(first).await);

        let second = free_port().await;
        config.osc.receive_port = second;
        state.reconcile_osc(&config).await.unwrap();
        assert!(answers(second).await);
        assert!(!answers(first).await);
    }
}
