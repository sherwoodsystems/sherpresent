//! # OSC Sender
//!
//! Sends `/oscpoint/*` OSC commands to a single target host:port.
//!
//! Mirrors the Python bridge's `DirectSender`:
//! - Plain UDP socket (no connection)
//! - Each command is sent 3x with a 10ms inter-retry gap for reliability on
//!   lossy consumer-grade WiFi.
//! - One `OscSender` per device slot, owned by [`BridgeState`].

use rosc::{OscMessage, OscPacket};
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;
use tokio::time::sleep;

use super::messages::out;

/// Number of times each outgoing packet is retransmitted for reliability.
/// Two retries for a total of three transmissions, matching the Python bridge.
const RETRIES: u8 = 2;

/// Inter-retry gap. The Python bridge used 10ms.
const RETRY_GAP: Duration = Duration::from_millis(10);

/// A simple OSC UDP sender targeting a single `host:port` pair.
///
/// Cheap to clone: each clone shares the underlying `UdpSocket` via `Arc`.
#[derive(Clone)]
pub struct OscSender {
    /// Resolved target the OSC packets are addressed to.
    target: SocketAddr,
    socket: std::sync::Arc<UdpSocket>,
}

impl OscSender {
    /// Construct a sender targeting `host:port`.
    /// Binds the local UDP socket to `0.0.0.0:0`.
    pub fn new(host: &str, port: u16) -> std::io::Result<Self> {
        let target: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;
        Ok(Self {
            target,
            socket: std::sync::Arc::new(socket),
        })
    }

    /// Send `NEXT` (`/oscpoint/next`).
    ///
    /// Takes `self` by value so the returned future owns the socket and is
    /// `'static` + `Send` — required for axum/Tauri handlers that spawn the
    /// future on a tokio task.
    pub async fn send_next(self) {
        self.send_msg_owned(out::NEXT.to_string(), vec![]).await;
    }

    /// Send `PREVIOUS` (`/oscpoint/previous`).
    pub async fn send_prev(self) {
        self.send_msg_owned(out::PREVIOUS.to_string(), vec![]).await;
    }

    /// Send `GOTO <n>` (`/oscpoint/goto/slide` with one int32 arg).
    pub async fn send_goto(self, slide: i32) {
        self.send_msg_owned(out::GOTO_SLIDE.to_string(), vec![rosc::OscType::Int(slide)]).await;
    }

    /// Encodes + sends a single OSC message with the configured redundancy.
    ///
    /// `self` is consumed and moved into the returned future; `address` and
    /// `args` are owned to guarantee the future is `'static`.
    async fn send_msg_owned(self, address: String, args: Vec<rosc::OscType>) {
        let packet = OscPacket::Message(OscMessage {
            addr: address.clone(),
            args,
        });
        let bytes = match rosc::encoder::encode(&packet) {
            Ok(bytes) => bytes,
            Err(e) => {
                log::error!("Failed to encode OSC packet {address}: {e}");
                return;
            }
        };
        for attempt in 0..=RETRIES {
            match self.socket.send_to(&bytes, self.target) {
                Ok(_) => {}
                Err(e) => {
                    log::warn!(
                        "OSC send attempt {}/{} to {} failed: {}",
                        attempt + 1,
                        RETRIES + 1,
                        self.target,
                        e
                    );
                }
            }
            if attempt < RETRIES {
                sleep(RETRY_GAP).await;
            }
        }
        log::debug!("Sent {} -> {} ({}B x{})", address, self.target, bytes.len(), RETRIES + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_target_address() {
        let s = OscSender::new("127.0.0.1", 9000).unwrap();
        assert_eq!(s.target.port(), 9000);
    }

    #[test]
    fn rejects_invalid_host() {
        assert!(OscSender::new("not a host!!", 9000).is_err());
    }
}