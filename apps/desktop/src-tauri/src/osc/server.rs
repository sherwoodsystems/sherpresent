//! # OSC Server
//!
//! The main UDP server that receives OSC commands and sends feedback.
//!
//! ## How It Works
//!
//! 1. **Bind two UDP sockets:**
//!    - Receive socket (port 9000) - listens for incoming commands
//!    - Feedback socket - sends state updates to clients (port 9001)
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
//!    - Execute the command via StateManager
//!
//! 4. **On state change:**
//!    - Build feedback messages
//!    - Send them to the feedback address
//!
//! ## Graceful Shutdown
//!
//! We use a channel to signal shutdown. When `OscServerHandle::stop()` is called,
//! it sends a message through the channel, and the main loop exits cleanly.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

use rosc::{encoder, OscPacket};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use super::latency::CommandSource;
use super::messages::{OscCommand, OscFeedback};
use super::state_manager::{CachedState, StateManager};
use super::CommandSourcePeer;
use crate::config::{ChannelConfig, OscConfig};

// =============================================================================
// SUBNET BROADCAST CALCULATION
// =============================================================================

/// Calculate the subnet broadcast address from the local network interface.
/// Returns the broadcast address (e.g., 192.168.1.255) or falls back to 255.255.255.255.
fn get_subnet_broadcast_address() -> Ipv4Addr {
    match if_addrs::get_if_addrs() {
        Ok(interfaces) => {
            // Find the first non-loopback IPv4 interface with a broadcast address
            for iface in interfaces {
                if iface.is_loopback() {
                    continue;
                }

                if let if_addrs::IfAddr::V4(v4_addr) = &iface.addr {
                    // Get the IP and netmask
                    let ip = v4_addr.ip;
                    let netmask = v4_addr.netmask;

                    // Skip link-local addresses (169.254.x.x)
                    if ip.octets()[0] == 169 && ip.octets()[1] == 254 {
                        continue;
                    }

                    // Calculate broadcast address: IP | ~netmask
                    let ip_u32 = u32::from(ip);
                    let mask_u32 = u32::from(netmask);
                    let broadcast_u32 = ip_u32 | !mask_u32;
                    let broadcast = Ipv4Addr::from(broadcast_u32);

                    log::debug!(
                        "Using subnet broadcast {} (from {} on {})",
                        broadcast, ip, iface.name
                    );
                    return broadcast;
                }
            }

            log::warn!("No suitable network interface found, using 255.255.255.255");
            Ipv4Addr::BROADCAST
        }
        Err(e) => {
            log::warn!("Failed to enumerate network interfaces: {}, using 255.255.255.255", e);
            Ipv4Addr::BROADCAST
        }
    }
}

/// Configuration for broadcast mode
#[derive(Debug, Clone)]
pub struct BroadcastConfig {
    /// Channel name to filter on
    pub channel_name: String,
    /// Port for broadcast communication
    pub broadcast_port: u16,
}

// =============================================================================
// SERVER HANDLE
// =============================================================================

/// Handle for controlling a running OSC server.
///
/// When you start the OSC server, you get this handle back.
/// Store it and call `stop()` when you want to shut down the server.
///
/// ## Why a Separate Handle?
///
/// The actual server runs in a spawned async task. The handle gives you
/// a way to communicate with that task (specifically, to tell it to stop).
/// This is the "actor pattern" - the task is an actor, and we communicate
/// via channels.
pub struct OscServerHandle {
    /// Channel to signal shutdown
    shutdown_tx: mpsc::Sender<()>,

    /// Handle to the spawned task (for cleanup)
    task_handle: tokio::task::JoinHandle<()>,
}

impl OscServerHandle {
    /// Stop the OSC server gracefully.
    ///
    /// This sends a shutdown signal and waits for the server task to exit.
    /// After calling this, the handle is consumed (can't be used again).
    pub async fn stop(self) {
        // Send shutdown signal (ignore errors if already closed)
        let _ = self.shutdown_tx.send(()).await;

        // Wait for the task to finish (ignore errors if it panicked)
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

    /// Broadcast mode configuration (optional)
    broadcast_config: Option<BroadcastConfig>,

    /// Channel to send discovered command source peers
    peer_tx: Option<mpsc::Sender<CommandSourcePeer>>,
}

impl OscServer {
    /// Create a new OSC server.
    ///
    /// ## Parameters
    ///
    /// - `config` - OSC port and host configuration
    /// - `state_manager` - Shared state manager (wrapped in Arc for sharing)
    pub fn new(config: OscConfig, state_manager: Arc<StateManager>) -> Self {
        Self {
            config,
            state_manager,
            broadcast_config: None,
            peer_tx: None,
        }
    }

    /// Create a new OSC server with broadcast mode and peer tracking.
    ///
    /// ## Parameters
    ///
    /// - `config` - OSC port and host configuration
    /// - `state_manager` - Shared state manager
    /// - `channel_config` - Channel configuration
    /// - `peer_tx` - Channel to send discovered command source peers
    pub fn with_broadcast_and_peer_tracking(
        config: OscConfig,
        state_manager: Arc<StateManager>,
        channel_config: &ChannelConfig,
        peer_tx: mpsc::Sender<CommandSourcePeer>,
    ) -> Self {
        let broadcast_config = if channel_config.enabled && channel_config.broadcast_mode {
            Some(BroadcastConfig {
                channel_name: channel_config.channel_name.clone(),
                broadcast_port: channel_config.broadcast_port,
            })
        } else {
            None
        };

        Self {
            config,
            state_manager,
            broadcast_config,
            peer_tx: Some(peer_tx),
        }
    }

    /// Start the OSC server.
    ///
    /// ## Returns
    ///
    /// - `Ok(OscServerHandle)` - Server is running, use handle to stop it
    /// - `Err(String)` - Failed to bind sockets
    ///
    /// ## What Happens
    ///
    /// 1. Bind receive socket to listen for commands
    /// 2. Bind feedback socket to send state updates
    /// 3. Optionally bind broadcast socket for channel-based communication
    /// 4. Spawn the main event loop as a background task
    /// 5. Start state polling (every 2 seconds)
    /// 6. Return the handle
    pub async fn start(
        self,
        mut state_change_rx: mpsc::Receiver<CachedState>,
    ) -> Result<OscServerHandle, String> {
        // =====================================================================
        // BIND SOCKETS
        // =====================================================================

        // Receive socket - where we listen for direct commands (port 9000)
        let receive_addr = format!("{}:{}", self.config.host, self.config.receive_port);
        let receive_socket = UdpSocket::bind(&receive_addr)
            .await
            .map_err(|e| format!("Failed to bind receive socket on {}: {}", receive_addr, e))?;

        log::info!("OSC server listening on {}", receive_addr);

        // Feedback socket - we bind to any available port since we're only sending
        // Using 0.0.0.0:0 lets the OS pick an available port
        let feedback_socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| format!("Failed to bind feedback socket: {}", e))?;

        // Enable broadcast on feedback socket (needed for broadcast mode)
        feedback_socket
            .set_broadcast(true)
            .map_err(|e| format!("Failed to enable broadcast on feedback socket: {}", e))?;

        // Parse all feedback destination addresses
        let feedback_addrs = Self::parse_feedback_destinations(&self.config)?;

        if feedback_addrs.is_empty() {
            log::warn!("No feedback destinations configured");
        } else {
            log::info!(
                "OSC feedback will be sent to {} destination(s): {:?}",
                feedback_addrs.len(),
                feedback_addrs
            );
        }

        // =====================================================================
        // BROADCAST SOCKET (optional)
        // =====================================================================

        let broadcast_socket = if let Some(ref bc) = self.broadcast_config {
            let broadcast_addr = format!("0.0.0.0:{}", bc.broadcast_port);
            let socket = UdpSocket::bind(&broadcast_addr)
                .await
                .map_err(|e| format!("Failed to bind broadcast socket on {}: {}", broadcast_addr, e))?;

            // Enable receiving broadcast packets
            socket
                .set_broadcast(true)
                .map_err(|e| format!("Failed to enable broadcast on socket: {}", e))?;

            log::info!(
                "Broadcast mode enabled: listening on port {} for channel '{}'",
                bc.broadcast_port,
                bc.channel_name
            );

            Some(socket)
        } else {
            None
        };

        // Calculate broadcast address for sending feedback
        let broadcast_feedback_addr: Option<SocketAddr> = if let Some(ref bc) = self.broadcast_config {
            // Calculate subnet broadcast address (e.g., 192.168.1.255) instead of 255.255.255.255
            let broadcast_ip = get_subnet_broadcast_address();
            let addr = SocketAddr::new(std::net::IpAddr::V4(broadcast_ip), bc.broadcast_port);
            log::info!("Broadcast feedback will be sent to {}", addr);
            Some(addr)
        } else {
            None
        };

        // =====================================================================
        // CREATE SHUTDOWN CHANNEL
        // =====================================================================

        // mpsc channel with buffer size 1 - we only need to send one shutdown signal
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        // =====================================================================
        // START POLLING
        // =====================================================================

        // Start background polling for external state changes
        self.state_manager.start_polling(2000);

        // =====================================================================
        // SPAWN MAIN EVENT LOOP
        // =====================================================================

        // Clone what we need for the spawned task
        let state_manager = self.state_manager.clone();
        let broadcast_config = self.broadcast_config.clone();
        let peer_tx = self.peer_tx.clone();

        let task_handle = tokio::spawn(async move {
            // Buffer for incoming UDP packets
            // OSC messages are typically small, 1024 bytes is plenty
            let mut buf = [0u8; 1024];
            let mut broadcast_buf = [0u8; 1024];

            // Deduplication cache: (source_addr, osc_address) -> last_seen timestamp
            // Prevents triple-execution from bridge retry logic (3x sends)
            let mut dedup_cache: HashMap<(SocketAddr, String), Instant> = HashMap::new();
            let mut last_dedup_prune = Instant::now();

            log::info!("OSC server event loop started");

            // Main event loop
            loop {
                // `tokio::select!` waits for multiple async operations
                // and runs the code for whichever completes first.
                //
                // This is like Promise.race() in JavaScript, but more powerful
                // because it can handle multiple branches with different types.
                tokio::select! {
                    // =========================================================
                    // BRANCH 1: Incoming OSC packet (direct mode - port 9000)
                    // =========================================================
                    result = receive_socket.recv_from(&mut buf) => {
                        match result {
                            Ok((len, src)) => {
                                // Decode the OSC packet
                                // rosc::decoder::decode_udp expects a slice of the actual data
                                match rosc::decoder::decode_udp(&buf[..len]) {
                                    Ok((_, packet)) => {
                                        // Log ALL incoming packets at info level
                                        log::info!(
                                            "📥 OSC packet from {}: {} {}",
                                            src,
                                            Self::get_packet_address(&packet),
                                            Self::get_packet_args(&packet)
                                        );
                                        Self::handle_packet(
                                            packet,
                                            &state_manager,
                                            &feedback_socket,
                                            &feedback_addrs,
                                            None, // No channel filtering for direct mode
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
                    // BRANCH 2: Incoming broadcast packet (broadcast mode - port 9002)
                    // =========================================================
                    result = async {
                        if let Some(ref socket) = broadcast_socket {
                            socket.recv_from(&mut broadcast_buf).await
                        } else {
                            // If no broadcast socket, pend forever (won't be selected)
                            std::future::pending().await
                        }
                    } => {
                        if let Ok((len, src)) = result {
                            match rosc::decoder::decode_udp(&broadcast_buf[..len]) {
                                Ok((_, packet)) => {
                                    let addr_str = Self::get_packet_address(&packet);

                                    // Deduplication: skip if same (src, address) seen within 100ms
                                    let dedup_key = (src, addr_str.clone());
                                    let now = Instant::now();
                                    if let Some(last_seen) = dedup_cache.get(&dedup_key) {
                                        if now.duration_since(*last_seen).as_millis() < 100 {
                                            log::trace!(
                                                "Dedup: skipping duplicate broadcast from {}: {}",
                                                src, addr_str
                                            );
                                            // Prune old entries periodically
                                            if now.duration_since(last_dedup_prune).as_secs() >= 1 {
                                                dedup_cache.retain(|_, t| now.duration_since(*t).as_secs() < 1);
                                                last_dedup_prune = now;
                                            }
                                            continue;
                                        }
                                    }
                                    dedup_cache.insert(dedup_key, now);

                                    // Prune old entries periodically
                                    if now.duration_since(last_dedup_prune).as_secs() >= 1 {
                                        dedup_cache.retain(|_, t| now.duration_since(*t).as_secs() < 1);
                                        last_dedup_prune = now;
                                    }

                                    // Log ALL incoming broadcast packets at info level
                                    log::info!(
                                        "📡 OSC broadcast from {}: {} {}",
                                        src,
                                        addr_str,
                                        Self::get_packet_args(&packet)
                                    );

                                    // Track the command source for peer discovery
                                    if let (Some(ref bc), Some(ref tx)) = (&broadcast_config, &peer_tx) {
                                        if Self::is_command_packet(&packet) {
                                            let _ = tx.try_send(CommandSourcePeer {
                                                address: src,
                                                channel: bc.channel_name.clone(),
                                            });
                                        }
                                    }

                                    Self::handle_packet(
                                        packet,
                                        &state_manager,
                                        &feedback_socket,
                                        &feedback_addrs,
                                        broadcast_config.as_ref().map(|c| c.channel_name.as_str()),
                                    ).await;
                                }
                                Err(e) => {
                                    log::warn!("Failed to decode broadcast OSC packet from {}: {}", src, e);
                                }
                            }
                        }
                    }

                    // =========================================================
                    // BRANCH 3: State changed -> send feedback
                    // =========================================================
                    Some(state) = state_change_rx.recv() => {
                        // Send to direct destinations
                        Self::send_feedback_to_all(&state, &feedback_socket, &feedback_addrs).await;

                        // Also broadcast feedback if in broadcast mode
                        if let (Some(ref bc), Some(ref addr)) = (&broadcast_config, &broadcast_feedback_addr) {
                            Self::send_broadcast_feedback(
                                &state,
                                &bc.channel_name,
                                &feedback_socket,
                                addr,
                            ).await;
                        }
                    }

                    // =========================================================
                    // BRANCH 4: Shutdown signal
                    // =========================================================
                    _ = shutdown_rx.recv() => {
                        log::info!("OSC server received shutdown signal");
                        state_manager.stop_polling();
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

    /// Check if an OSC packet contains a command (not feedback).
    ///
    /// Used for peer tracking - we only want to track devices that send commands,
    /// not our own feedback messages bouncing back.
    fn is_command_packet(packet: &OscPacket) -> bool {
        match packet {
            OscPacket::Message(msg) => {
                // Command addresses contain /next, /prev, /goto, /status, /refresh
                // Feedback addresses contain /state/
                let addr = &msg.addr;
                !addr.contains("/state/")
                    && (addr.contains("/next")
                        || addr.contains("/prev")
                        || addr.contains("/goto")
                        || addr.contains("/status")
                        || addr.contains("/refresh"))
            }
            OscPacket::Bundle(bundle) => {
                // If any message in the bundle is a command, consider it a command packet
                bundle.content.iter().any(Self::is_command_packet)
            }
        }
    }

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

    /// Parse all feedback destinations from config into SocketAddrs
    fn parse_feedback_destinations(config: &OscConfig) -> Result<Vec<SocketAddr>, String> {
        let destinations = config.get_all_destinations();
        let mut addrs = Vec::with_capacity(destinations.len());

        for dest in destinations {
            let addr: SocketAddr = dest
                .to_addr_string()
                .parse()
                .map_err(|e| format!("Invalid feedback address {}: {}", dest.to_addr_string(), e))?;
            addrs.push(addr);
        }

        Ok(addrs)
    }

    /// Handle an incoming OSC packet.
    ///
    /// OSC packets can contain:
    /// - A single message
    /// - A bundle of messages (with a timestamp)
    ///
    /// We handle both cases recursively.
    ///
    /// ## Parameters
    /// - `channel_filter` - If Some, only process messages for this channel (broadcast mode)
    async fn handle_packet(
        packet: OscPacket,
        state_manager: &StateManager,
        feedback_socket: &UdpSocket,
        feedback_addrs: &[SocketAddr],
        channel_filter: Option<&str>,
    ) {
        match packet {
            OscPacket::Message(msg) => {
                Self::handle_message(&msg, state_manager, feedback_socket, feedback_addrs, channel_filter)
                    .await;
            }
            OscPacket::Bundle(bundle) => {
                // Process all messages in the bundle
                for packet in bundle.content {
                    // Recursive call to handle nested messages/bundles
                    Box::pin(Self::handle_packet(
                        packet,
                        state_manager,
                        feedback_socket,
                        feedback_addrs,
                        channel_filter,
                    ))
                    .await;
                }
            }
        }
    }

    /// Handle a single OSC message.
    ///
    /// ## Parameters
    /// - `channel_filter` - If Some, only process channel-based messages for this channel
    async fn handle_message(
        msg: &rosc::OscMessage,
        state_manager: &StateManager,
        feedback_socket: &UdpSocket,
        feedback_addrs: &[SocketAddr],
        channel_filter: Option<&str>,
    ) {
        // If we have a channel filter, try to parse as a channel command first
        if let Some(filter_channel) = channel_filter {
            if let Some(channel_cmd) = OscCommand::from_channel_message(&msg.addr, &msg.args) {
                // Only process if the channel matches our filter
                if channel_cmd.channel != filter_channel {
                    log::trace!(
                        "Ignoring message for channel '{}' (we are on '{}')",
                        channel_cmd.channel,
                        filter_channel
                    );
                    return;
                }

                log::debug!(
                    "Received broadcast command for channel '{}': {:?}",
                    channel_cmd.channel,
                    channel_cmd.command
                );

                // Execute the command
                match channel_cmd.command {
                    OscCommand::Next => state_manager.next_slide(CommandSource::OscBroadcast),
                    OscCommand::Previous => state_manager.prev_slide(CommandSource::OscBroadcast),
                    OscCommand::Status => {
                        let state = state_manager.get_state();
                        Self::send_feedback_to_all(&state, feedback_socket, feedback_addrs).await;
                    }
                    OscCommand::Refresh => state_manager.refresh_state(),
                    OscCommand::ChannelCmdGoto { slide, .. } => {
                        state_manager.goto_slide(slide, CommandSource::OscBroadcast);
                    }
                    _ => {}
                }
                return;
            }

            // If it's not a valid channel command in broadcast mode, ignore it
            // (unless it matches our channel prefix for state feedback, etc.)
            if !OscCommand::matches_channel(&msg.addr, filter_channel) {
                log::trace!("Ignoring non-channel message in broadcast mode: {}", msg.addr);
                return;
            }
        }

        // Standard (direct mode) message handling
        let command = OscCommand::from_message(&msg.addr, &msg.args);

        log::debug!("Received OSC command: {:?}", command);

        match command {
            OscCommand::Next => {
                state_manager.next_slide(CommandSource::Osc);
                // Feedback is sent automatically when state changes
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
                // Query only - send current zoom level to all destinations
                let state = state_manager.get_state();
                let msg = OscFeedback::zoom_only(state.zoom_level);
                Self::send_single_message_to_all(&msg, feedback_socket, feedback_addrs).await;
            }

            OscCommand::Status => {
                // Query only - send full state to all destinations
                let state = state_manager.get_state();
                Self::send_feedback_to_all(&state, feedback_socket, feedback_addrs).await;
            }

            OscCommand::Goto { slide } => {
                state_manager.goto_slide(slide, CommandSource::Osc);
            }

            OscCommand::Refresh => {
                // Force a state refresh (will trigger feedback when done)
                state_manager.refresh_state();
            }

            // Channel commands - handled separately when channel sync is enabled
            OscCommand::ChannelAnnounce { instance_id, channel, ip, port } => {
                log::debug!(
                    "Channel announce from {} (channel: {}, addr: {}:{})",
                    instance_id, channel, ip, port
                );
                // TODO: Forward to channel manager when integrated
            }
            OscCommand::ChannelLeave { instance_id } => {
                log::debug!("Channel leave from {}", instance_id);
                // TODO: Forward to channel manager when integrated
            }
            OscCommand::ChannelHeartbeat { instance_id } => {
                log::trace!("Channel heartbeat from {}", instance_id);
                // TODO: Forward to channel manager when integrated
            }
            OscCommand::ChannelCmdNext { origin } => {
                log::debug!("Channel cmd/next from {}", origin);
                // TODO: Execute if channel enabled and origin != self
            }
            OscCommand::ChannelCmdPrev { origin } => {
                log::debug!("Channel cmd/prev from {}", origin);
                // TODO: Execute if channel enabled and origin != self
            }
            OscCommand::ChannelCmdGoto { origin, slide } => {
                log::debug!("Channel cmd/goto from {} to slide {}", origin, slide);
                // TODO: Execute if channel enabled and origin != self
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
    async fn send_feedback_to_all(
        state: &CachedState,
        socket: &UdpSocket,
        addrs: &[SocketAddr],
    ) {
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

    /// Send channel-aware feedback via broadcast.
    ///
    /// Uses the format `/clicker/<channel>/state/<property>` so receivers
    /// can filter by channel.
    async fn send_broadcast_feedback(
        state: &CachedState,
        channel: &str,
        socket: &UdpSocket,
        broadcast_addr: &SocketAddr,
    ) {
        let messages = OscFeedback::from_state_with_channel(state, channel);

        for msg in messages {
            let packet = OscPacket::Message(msg);
            match encoder::encode(&packet) {
                Ok(bytes) => {
                    if let Err(e) = socket.send_to(&bytes, broadcast_addr).await {
                        log::warn!("Failed to broadcast feedback to {}: {}", broadcast_addr, e);
                    }
                }
                Err(e) => {
                    log::error!("Failed to encode broadcast feedback: {}", e);
                }
            }
        }

        log::debug!(
            "Broadcast feedback for channel '{}': presenting={}, slide={}/{}",
            channel,
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
        // Wrap the message in a packet for encoding
        let packet = OscPacket::Message(msg.clone());

        // Encode to bytes once, send to all destinations
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

    /// Send a single OSC message to a specific destination.
    #[allow(dead_code)]
    async fn send_single_message(
        msg: &rosc::OscMessage,
        socket: &UdpSocket,
        addr: &SocketAddr,
    ) {
        // Wrap the message in a packet for encoding
        let packet = OscPacket::Message(msg.clone());

        // Encode to bytes
        match encoder::encode(&packet) {
            Ok(bytes) => {
                if let Err(e) = socket.send_to(&bytes, addr).await {
                    log::warn!("Failed to send OSC feedback to {}: {}", addr, e);
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

    #[test]
    fn test_socket_addr_parsing() {
        // Test that we can parse typical addresses
        let addr: Result<SocketAddr, _> = "127.0.0.1:9001".parse();
        assert!(addr.is_ok());

        let addr: Result<SocketAddr, _> = "0.0.0.0:9000".parse();
        assert!(addr.is_ok());
    }
}
