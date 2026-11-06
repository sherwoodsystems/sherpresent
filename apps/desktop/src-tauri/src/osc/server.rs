//! # OSC Server
//!
//! The main UDP server that receives OSC commands and sends feedback.
//!
//! ## How It Works
//!
//! 1. **Bind one UDP socket (port 9000):** commands arrive on it and feedback
//!    leaves from it, so replies get back through stateful firewalls.
//!
//! 2. **Main event loop using `tokio::select!`:**
//!    - Wait for incoming OSC packets
//!    - Wait for state change notifications
//!    - Wait for shutdown signal
//!    - Whichever happens first gets processed
//!
//! 3. **On incoming OSC packet:**
//!    - Decode the OSC message
//!    - Parse the address to determine the command
//!    - Remember the sender, so it gets feedback (see `FeedbackTargets`)
//!    - Execute the command via StateManager
//!
//! 4. **On state change:**
//!    - Build feedback messages
//!    - Send them to every recent sender and configured destination
//!
//! ## Graceful Shutdown
//!
//! We use a channel to signal shutdown. When `OscServerHandle::stop()` is called,
//! it sends a message through the channel, and the main loop exits cleanly.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rosc::{encoder, OscPacket};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use super::latency::CommandSource;
use super::messages::{OscCommand, OscFeedback, ScrollDirection};
use super::state_manager::{CachedState, StateManager};
use crate::config::OscConfig;

/// How long a client keeps getting feedback after the last command it sent.
/// Clients send `/clicker/status` every few seconds to stay on the list.
const CLIENT_TIMEOUT: Duration = Duration::from_secs(15);

/// How long to wait for a configured feedback hostname to resolve at startup.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(2);

// =============================================================================
// FEEDBACK TARGETS
// =============================================================================

/// Where feedback goes: whoever has sent us a command recently, at the address
/// it sent from, plus any configured destinations (for listen-only devices).
struct FeedbackTargets {
    configured: Vec<SocketAddr>,
    clients: HashMap<SocketAddr, Instant>,
}

impl FeedbackTargets {
    fn new(configured: Vec<SocketAddr>) -> Self {
        Self {
            configured,
            clients: HashMap::new(),
        }
    }

    fn heard_from(&mut self, addr: SocketAddr, now: Instant) {
        if self.clients.insert(addr, now).is_none() {
            log::info!("OSC client connected: {}", addr);
        }
    }

    fn current(&mut self, now: Instant) -> Vec<SocketAddr> {
        self.clients.retain(|addr, last| {
            let alive = now.duration_since(*last) < CLIENT_TIMEOUT;
            if !alive {
                log::info!("OSC client timed out: {}", addr);
            }
            alive
        });

        let mut addrs = self.configured.clone();
        addrs.extend(
            self.clients
                .keys()
                .filter(|addr| !self.configured.contains(addr)),
        );
        addrs
    }
}

// =============================================================================
// SERVER HANDLE
// =============================================================================

/// Handle for controlling a running OSC server.
///
/// When you start the OSC server, you get this handle back.
/// Store it and call `stop()` when you want to shut down the server.
pub struct OscServerHandle {
    /// Channel to signal shutdown
    shutdown_tx: mpsc::Sender<()>,

    /// Handle to the spawned task (for cleanup)
    task_handle: tokio::task::JoinHandle<()>,
}

impl OscServerHandle {
    /// Stop the OSC server gracefully.
    pub async fn stop(self) {
        let _ = self.shutdown_tx.send(()).await;
        let _ = self.task_handle.await;
        log::info!("OSC server stopped");
    }
}

// =============================================================================
// SERVER
// =============================================================================

/// The OSC server that handles UDP communication.
///
/// ## Lifecycle
///
/// 1. Create with `OscServer::new(config, state_manager)`
/// 2. Start with `server.start().await` - returns a handle
/// 3. Server runs in background, processing messages
/// 4. Stop with `handle.stop().await`
pub struct OscServer {
    /// OSC configuration (ports, hosts)
    config: OscConfig,

    /// State manager for handling commands
    state_manager: Arc<StateManager>,

    /// Broadcast channel for scroll commands (forwarded to web server)
    scroll_broadcast: Option<tokio::sync::broadcast::Sender<ScrollDirection>>,

    /// The name clients see when they look for us with `/clicker/discover`
    name: String,
}

impl OscServer {
    /// Create a new OSC server.
    pub fn new(config: OscConfig, state_manager: Arc<StateManager>) -> Self {
        Self {
            config,
            state_manager,
            scroll_broadcast: None,
            name: sherpresent_core::machine_name(),
        }
    }

    /// Set the name clients see when they look for us with `/clicker/discover`.
    pub fn with_name(mut self, name: String) -> Self {
        self.name = name;
        self
    }

    /// Set the scroll broadcast channel for forwarding scroll commands to the web server.
    pub fn with_scroll_broadcast(
        mut self,
        tx: tokio::sync::broadcast::Sender<ScrollDirection>,
    ) -> Self {
        self.scroll_broadcast = Some(tx);
        self
    }

    /// Start the OSC server.
    ///
    /// ## Returns
    ///
    /// - `Ok(OscServerHandle)` - Server is running, use handle to stop it
    /// - `Err(String)` - Failed to bind sockets
    pub async fn start(self) -> Result<OscServerHandle, String> {
        // Subscribe before binding so no change published meanwhile is missed.
        let mut state_changes = self.state_manager.subscribe();

        // =====================================================================
        // BIND SOCKET
        // =====================================================================

        // Commands arrive here (port 9000) and feedback leaves from here
        let receive_addr = format!("{}:{}", self.config.host, self.config.receive_port);
        let socket = UdpSocket::bind(&receive_addr)
            .await
            .map_err(|e| format!("Failed to bind OSC socket on {}: {}", receive_addr, e))?;

        log::info!("OSC server listening on {}", receive_addr);

        #[cfg(target_os = "windows")]
        log::info!(
            "Windows firewall note: If LAN access doesn't work, run PowerShell as Admin: \
             New-NetFirewallRule -DisplayName 'SherPresent OSC' -Direction Inbound -Protocol UDP -LocalPort {} -Action Allow",
            self.config.receive_port
        );

        let configured = Self::resolve_feedback_destinations(&self.config).await;
        log::info!("OSC feedback destinations: {:?}", configured);
        let mut targets = FeedbackTargets::new(configured);

        // =====================================================================
        // CREATE SHUTDOWN CHANNEL
        // =====================================================================

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        // =====================================================================
        // SPAWN MAIN EVENT LOOP
        // =====================================================================

        let state_manager = self.state_manager.clone();
        let scroll_broadcast = self.scroll_broadcast.clone();
        let name = self.name;

        let task_handle = tokio::spawn(async move {
            let mut buf = [0u8; 1024];

            log::info!("OSC server event loop started");

            loop {
                tokio::select! {
                    // =========================================================
                    // BRANCH 1: Incoming OSC packet (port 9000)
                    // =========================================================
                    result = socket.recv_from(&mut buf) => {
                        match result {
                            Ok((len, src)) => {
                                match rosc::decoder::decode_udp(&buf[..len]) {
                                    Ok((_, packet)) => {
                                        let address = Self::get_packet_address(&packet);
                                        // Status is the clients' heartbeat.
                                        let level = if address == "/clicker/status" {
                                            log::Level::Debug
                                        } else {
                                            log::Level::Info
                                        };
                                        log::log!(
                                            level,
                                            "📥 OSC packet from {}: {} {}",
                                            src,
                                            address,
                                            Self::get_packet_args(&packet)
                                        );
                                        Self::handle_packet(
                                            packet,
                                            src,
                                            &state_manager,
                                            &socket,
                                            &mut targets,
                                            scroll_broadcast.as_ref(),
                                            &name,
                                        ).await;
                                    }
                                    Err(e) => {
                                        log::warn!("Failed to decode OSC packet from {}: {}", src, e);
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Error receiving UDP packet: {}", e);
                            }
                        }
                    }

                    // =========================================================
                    // BRANCH 2: State changed -> send feedback
                    // =========================================================
                    changed = state_changes.recv() => {
                        // Lagged: skip the backlog and send where things are now.
                        let state = changed.unwrap_or_else(|_| state_manager.get_state());
                        let addrs = targets.current(Instant::now());
                        Self::send_feedback_to_all(&state, &socket, &addrs).await;
                    }

                    // =========================================================
                    // BRANCH 3: Shutdown signal
                    // =========================================================
                    _ = shutdown_rx.recv() => {
                        log::info!("OSC server received shutdown signal");
                        break;
                    }
                }
            }

            log::info!("OSC server event loop ended");
        });

        Ok(OscServerHandle {
            shutdown_tx,
            task_handle,
        })
    }

    // =========================================================================
    // PACKET HANDLING
    // =========================================================================

    /// Extract the OSC address from a packet for logging.
    fn get_packet_address(packet: &OscPacket) -> String {
        match packet {
            OscPacket::Message(msg) => msg.addr.clone(),
            OscPacket::Bundle(_) => "[bundle]".to_string(),
        }
    }

    /// Extract the OSC arguments from a packet for logging.
    fn get_packet_args(packet: &OscPacket) -> String {
        match packet {
            OscPacket::Message(msg) => {
                if msg.args.is_empty() {
                    "[]".to_string()
                } else {
                    format!("{:?}", msg.args)
                }
            }
            OscPacket::Bundle(bundle) => format!("[{} messages]", bundle.content.len()),
        }
    }

    /// Resolve the configured feedback destinations (IPs or hostnames). One
    /// that can't be resolved is skipped, so a typo can't stop OSC commands.
    async fn resolve_feedback_destinations(config: &OscConfig) -> Vec<SocketAddr> {
        let mut addrs = Vec::new();

        for dest in config.get_all_destinations() {
            if dest.host.trim().is_empty() {
                continue;
            }
            let target = dest.to_addr_string();
            let lookup = tokio::time::timeout(RESOLVE_TIMEOUT, tokio::net::lookup_host(&target));
            // Our socket is IPv4.
            let resolved = match lookup.await {
                Ok(Ok(mut found)) => found.find(SocketAddr::is_ipv4),
                Ok(Err(e)) => {
                    log::warn!("Skipping feedback destination {}: {}", target, e);
                    continue;
                }
                Err(_) => {
                    log::warn!("Skipping feedback destination {}: lookup timed out", target);
                    continue;
                }
            };
            match resolved {
                Some(addr) => addrs.push(addr),
                None => log::warn!("Skipping feedback destination {}: no IPv4 address", target),
            }
        }

        addrs
    }

    /// Handle an incoming OSC packet.
    async fn handle_packet(
        packet: OscPacket,
        src: SocketAddr,
        state_manager: &Arc<StateManager>,
        socket: &UdpSocket,
        targets: &mut FeedbackTargets,
        scroll_tx: Option<&tokio::sync::broadcast::Sender<ScrollDirection>>,
        name: &str,
    ) {
        match packet {
            OscPacket::Message(msg) => {
                Self::handle_message(&msg, src, state_manager, socket, targets, scroll_tx, name)
                    .await;
            }
            OscPacket::Bundle(bundle) => {
                for packet in bundle.content {
                    Box::pin(Self::handle_packet(
                        packet,
                        src,
                        state_manager,
                        socket,
                        targets,
                        scroll_tx,
                        name,
                    ))
                    .await;
                }
            }
        }
    }

    /// Handle a single OSC message.
    async fn handle_message(
        msg: &rosc::OscMessage,
        src: SocketAddr,
        state_manager: &Arc<StateManager>,
        socket: &UdpSocket,
        targets: &mut FeedbackTargets,
        scroll_tx: Option<&tokio::sync::broadcast::Sender<ScrollDirection>>,
        name: &str,
    ) {
        let command = OscCommand::from_message(&msg.addr, &msg.args);

        log::debug!("Received OSC command: {:?}", command);

        // Looking for us isn't connecting, and stray traffic isn't a client.
        if !matches!(command, OscCommand::Discover | OscCommand::Unknown(_)) {
            targets.heard_from(src, Instant::now());
        }

        match command {
            OscCommand::Next => {
                state_manager.next_slide(CommandSource::Osc);
            }

            OscCommand::Previous => {
                state_manager.prev_slide(CommandSource::Osc);
            }

            OscCommand::ZoomIn => {
                state_manager.zoom_in();
            }

            OscCommand::ZoomOut => {
                state_manager.zoom_out();
            }

            OscCommand::Zoom => {
                let state = state_manager.get_state();
                let msg = OscFeedback::zoom_only(state.zoom_level);
                let addrs = targets.current(Instant::now());
                Self::send_single_message_to_all(&msg, socket, &addrs).await;
            }

            OscCommand::Status => {
                let state = state_manager.get_state();
                let addrs = targets.current(Instant::now());
                Self::send_feedback_to_all(&state, socket, &addrs).await;
            }

            OscCommand::Discover => {
                let msg = OscFeedback::here(name);
                Self::send_single_message_to_all(&msg, socket, &[src]).await;
            }

            OscCommand::Goto { slide } => {
                state_manager.goto_slide(slide, CommandSource::Osc);
            }

            OscCommand::Refresh => {
                state_manager.refresh_state();
            }

            OscCommand::ScrollUp => {
                if let Some(tx) = scroll_tx {
                    let _ = tx.send(ScrollDirection::Up);
                }
            }

            OscCommand::ScrollDown => {
                if let Some(tx) = scroll_tx {
                    let _ = tx.send(ScrollDirection::Down);
                }
            }

            OscCommand::NotesPage => {
                if let Some(tx) = scroll_tx {
                    let _ = tx.send(ScrollDirection::Page);
                }
            }

            OscCommand::Unknown(addr) => {
                log::debug!("Ignoring unknown OSC address: {}", addr);
            }
        }
    }

    // =========================================================================
    // FEEDBACK SENDING
    // =========================================================================

    /// Send all state feedback messages to all destinations.
    async fn send_feedback_to_all(state: &CachedState, socket: &UdpSocket, addrs: &[SocketAddr]) {
        let messages = OscFeedback::from_state(state);

        for msg in messages {
            Self::send_single_message_to_all(&msg, socket, addrs).await;
        }

        log::debug!(
            "Sent feedback to {} destination(s): presenting={}, slide={}/{}",
            addrs.len(),
            state.is_presenting,
            state.current_slide,
            state.total_slides
        );
    }

    /// Send a single OSC message to all destinations.
    async fn send_single_message_to_all(
        msg: &rosc::OscMessage,
        socket: &UdpSocket,
        addrs: &[SocketAddr],
    ) {
        let packet = OscPacket::Message(msg.clone());

        match encoder::encode(&packet) {
            Ok(bytes) => {
                for addr in addrs {
                    if let Err(e) = socket.send_to(&bytes, addr).await {
                        log::warn!("Failed to send OSC feedback to {}: {}", addr, e);
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to encode OSC message: {}", e);
            }
        }
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    use crate::osc::state_manager::StateSinks;
    use rosc::{OscMessage, OscType};

    fn addr(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    #[test]
    fn a_client_gets_feedback_until_it_goes_quiet() {
        let now = Instant::now();
        let client = addr("10.0.0.5:50000");
        let mut targets = FeedbackTargets::new(vec![]);

        targets.heard_from(client, now);
        assert_eq!(targets.current(now + CLIENT_TIMEOUT / 2), vec![client]);

        targets.heard_from(client, now + CLIENT_TIMEOUT / 2);
        assert_eq!(targets.current(now + CLIENT_TIMEOUT), vec![client]);

        assert!(targets.current(now + CLIENT_TIMEOUT * 2).is_empty());
    }

    #[test]
    fn configured_destinations_stay_and_are_not_doubled() {
        let now = Instant::now();
        let fixed = addr("10.0.0.9:9001");
        let mut targets = FeedbackTargets::new(vec![fixed]);

        targets.heard_from(fixed, now);
        assert_eq!(targets.current(now), vec![fixed]);
        assert_eq!(targets.current(now + CLIENT_TIMEOUT * 2), vec![fixed]);
    }

    /// A port nothing is listening on, for the server under test.
    async fn free_port() -> u16 {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        socket.local_addr().unwrap().port()
    }

    async fn start_server(feedback_host: &str) -> (OscServerHandle, u16) {
        let port = free_port().await;
        let config = OscConfig {
            receive_port: port,
            host: "127.0.0.1".to_string(),
            feedback_host: feedback_host.to_string(),
            ..OscConfig::default()
        };
        let state_manager = Arc::new(StateManager::new(StateSinks::default()));
        let handle = OscServer::new(config, state_manager)
            .with_name("Stage Left".to_string())
            .start()
            .await
            .unwrap();
        (handle, port)
    }

    async fn client(port: u16) -> UdpSocket {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        socket.connect(("127.0.0.1", port)).await.unwrap();
        socket
    }

    async fn send(socket: &UdpSocket, address: &str) {
        let packet = OscPacket::Message(OscMessage {
            addr: address.to_string(),
            args: vec![],
        });
        socket
            .send(&encoder::encode(&packet).unwrap())
            .await
            .unwrap();
    }

    async fn receive(socket: &UdpSocket) -> OscMessage {
        let mut buf = [0u8; 1024];
        let len = tokio::time::timeout(Duration::from_secs(2), socket.recv(&mut buf))
            .await
            .expect("no reply from the OSC server")
            .unwrap();
        match rosc::decoder::decode_udp(&buf[..len]).unwrap().1 {
            OscPacket::Message(msg) => msg,
            OscPacket::Bundle(_) => panic!("expected a message"),
        }
    }

    // The sockets are connected, so these also prove replies come from the
    // port the client sent to.
    #[tokio::test]
    async fn status_replies_to_the_sender() {
        let (handle, port) = start_server("127.0.0.1").await;
        let socket = client(port).await;

        send(&socket, "/clicker/status").await;
        assert_eq!(receive(&socket).await.addr, "/clicker/state/presenting");

        handle.stop().await;
    }

    #[tokio::test]
    async fn discover_replies_with_our_name_without_connecting() {
        let (handle, port) = start_server("127.0.0.1").await;
        let looking = client(port).await;
        let connected = client(port).await;

        send(&looking, "/clicker/discover").await;
        let reply = receive(&looking).await;
        assert_eq!(reply.addr, "/clicker/here");
        assert_eq!(reply.args, vec![OscType::String("Stage Left".to_string())]);

        // Only the connected client hears the status it asked for.
        send(&connected, "/clicker/status").await;
        assert_eq!(receive(&connected).await.addr, "/clicker/state/presenting");
        let mut buf = [0u8; 64];
        let heard = tokio::time::timeout(Duration::from_millis(100), looking.recv(&mut buf)).await;
        assert!(heard.is_err(), "discovering should not subscribe");

        handle.stop().await;
    }

    #[tokio::test]
    async fn a_bad_feedback_host_does_not_stop_commands() {
        let (handle, port) = start_server("not a host!!").await;
        let socket = client(port).await;

        send(&socket, "/clicker/status").await;
        assert_eq!(receive(&socket).await.addr, "/clicker/state/presenting");

        handle.stop().await;
    }
}
