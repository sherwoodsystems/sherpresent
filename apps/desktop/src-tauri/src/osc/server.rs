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

use std::net::SocketAddr;
use std::sync::Arc;

use rosc::{encoder, OscPacket};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use super::latency::CommandSource;
use super::messages::{OscCommand, OscFeedback, ScrollDirection};
use super::state_manager::{CachedState, StateManager};
use crate::config::OscConfig;

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
}

impl OscServer {
    /// Create a new OSC server.
    pub fn new(config: OscConfig, state_manager: Arc<StateManager>) -> Self {
        Self {
            config,
            state_manager,
            scroll_broadcast: None,
        }
    }

    /// Set the scroll broadcast channel for forwarding scroll commands to the web server.
    pub fn with_scroll_broadcast(mut self, tx: tokio::sync::broadcast::Sender<ScrollDirection>) -> Self {
        self.scroll_broadcast = Some(tx);
        self
    }

    /// Start the OSC server.
    ///
    /// ## Returns
    ///
    /// - `Ok(OscServerHandle)` - Server is running, use handle to stop it
    /// - `Err(String)` - Failed to bind sockets
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
        let feedback_socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| format!("Failed to bind feedback socket: {}", e))?;

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
        // CREATE SHUTDOWN CHANNEL
        // =====================================================================

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        // =====================================================================
        // START POLLING
        // =====================================================================

        self.state_manager.start_polling(2000);

        // =====================================================================
        // SPAWN MAIN EVENT LOOP
        // =====================================================================

        let state_manager = self.state_manager.clone();
        let scroll_broadcast = self.scroll_broadcast.clone();

        let task_handle = tokio::spawn(async move {
            let mut buf = [0u8; 1024];

            log::info!("OSC server event loop started");

            loop {
                tokio::select! {
                    // =========================================================
                    // BRANCH 1: Incoming OSC packet (port 9000)
                    // =========================================================
                    result = receive_socket.recv_from(&mut buf) => {
                        match result {
                            Ok((len, src)) => {
                                match rosc::decoder::decode_udp(&buf[..len]) {
                                    Ok((_, packet)) => {
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
                                            scroll_broadcast.as_ref(),
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
                    Some(state) = state_change_rx.recv() => {
                        Self::send_feedback_to_all(&state, &feedback_socket, &feedback_addrs).await;
                    }

                    // =========================================================
                    // BRANCH 3: Shutdown signal
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
    async fn handle_packet(
        packet: OscPacket,
        state_manager: &StateManager,
        feedback_socket: &UdpSocket,
        feedback_addrs: &[SocketAddr],
        scroll_tx: Option<&tokio::sync::broadcast::Sender<ScrollDirection>>,
    ) {
        match packet {
            OscPacket::Message(msg) => {
                Self::handle_message(&msg, state_manager, feedback_socket, feedback_addrs, scroll_tx)
                    .await;
            }
            OscPacket::Bundle(bundle) => {
                for packet in bundle.content {
                    Box::pin(Self::handle_packet(
                        packet,
                        state_manager,
                        feedback_socket,
                        feedback_addrs,
                        scroll_tx,
                    ))
                    .await;
                }
            }
        }
    }

    /// Handle a single OSC message.
    async fn handle_message(
        msg: &rosc::OscMessage,
        state_manager: &StateManager,
        feedback_socket: &UdpSocket,
        feedback_addrs: &[SocketAddr],
        scroll_tx: Option<&tokio::sync::broadcast::Sender<ScrollDirection>>,
    ) {
        let command = OscCommand::from_message(&msg.addr, &msg.args);

        log::debug!("Received OSC command: {:?}", command);

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
                Self::send_single_message_to_all(&msg, feedback_socket, feedback_addrs).await;
            }

            OscCommand::Status => {
                let state = state_manager.get_state();
                Self::send_feedback_to_all(&state, feedback_socket, feedback_addrs).await;
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

    #[test]
    fn test_socket_addr_parsing() {
        let addr: Result<SocketAddr, _> = "127.0.0.1:9001".parse();
        assert!(addr.is_ok());

        let addr: Result<SocketAddr, _> = "0.0.0.0:9000".parse();
        assert!(addr.is_ok());
    }
}
