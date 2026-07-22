//! # OSC Feedback State + Listener
//!
//! Mirrors the Python bridge's `FeedbackListener` + state-cache pattern.
//!
//! The bridge runs a UDP OSC server on `feedback_port` (default 9001) and
//! desktops / OSCPoint send oscpoint feedback messages to it. We cache the
//! latest state in [`FeedbackState`] and broadcast updates via [`FeedbackUpdate`].

use chrono::Utc;
use rosc::{OscMessage, OscPacket};
use serde::Serialize;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

use super::messages;

/// Cached snapshot of the latest OSC feedback received from any desktop.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FeedbackState {
    /// `true` when a desktop has reported an active slideshow.
    pub presenting: Option<bool>,
    /// `true` when a desktop has reported an open presentation.
    pub open: Option<bool>,
    /// 1-indexed currently visible slide number.
    #[serde(rename = "currentSlide", default)]
    pub current_slide: Option<i32>,
    /// Total number of slides in the active presentation.
    #[serde(rename = "slideCount", default)]
    pub slide_count: Option<i32>,
    /// Filename of the active presentation.
    #[serde(rename = "presentationName", default)]
    pub presentation_name: Option<String>,
    /// Currently visible slide's presenter notes.
    #[serde(rename = "slideNotes", default)]
    pub slide_notes: Option<String>,
    /// ISO-8601 timestamp of the last received feedback message.
    #[serde(rename = "lastUpdated", default)]
    pub last_updated: Option<chrono::DateTime<Utc>>,
    /// Last outgoing OSC command sent.
    #[serde(rename = "lastCommand", default)]
    pub last_command: Option<String>,
    /// ISO-8601 timestamp of the last sent OSC command.
    #[serde(rename = "lastCommandTime", default)]
    pub last_command_time: Option<chrono::DateTime<Utc>>,
}

impl FeedbackState {
    /// Apply an inbound OSC message to the cached state.
    fn apply_message(&mut self, msg: &OscMessage) -> bool {
        let changed = match msg.addr.as_str() {
            messages::feedback::CURRENT_SLIDE => {
                self.current_slide = msg.args.first().and_then(|a| a.int());
                true
            }
            messages::feedback::SLIDE_COUNT => {
                self.slide_count = msg.args.first().and_then(|a| a.int());
                true
            }
            messages::feedback::PRESENTATION_NAME => {
                self.presentation_name = msg.args.first().and_then(|a| a.string());
                true
            }
            messages::feedback::SLIDESHOW_NOTES => {
                self.slide_notes = msg.args.first().and_then(|a| a.string());
                true
            }
            addr if addr.starts_with(messages::feedback::V2_STATE_PREFIX) => {
                let tail = &addr[messages::feedback::V2_STATE_PREFIX.len()..];
                match tail {
                    "presenting" => {
                        self.presenting = msg.args.first().and_then(|a| a.bool());
                        true
                    }
                    "open" => {
                        self.open = msg.args.first().and_then(|a| a.bool());
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        };
        if changed {
            self.last_updated = Some(Utc::now());
        }
        changed
    }

    /// Stamp the state with the outgoing OSC command that just left the bridge.
    pub fn remember_outgoing_command(&mut self, address: &str) {
        let short = address.rsplit('/').next().unwrap_or(address);
        self.last_command = Some(short.to_string());
        self.last_command_time = Some(Utc::now());
    }
}

/// Broadcast channel payload sent when [`FeedbackState`] changes.
#[derive(Debug, Clone, Serialize)]
pub struct FeedbackUpdate {
    /// Full state snapshot (cloned).
    pub state: FeedbackState,
}

/// Token returned by [`start_feedback_listener`] so the caller can park the
/// background task on shutdown.
pub struct FeedbackListenerHandle {
    pub cancel: broadcast::Sender<()>,
}

/// Spawn a thread that listens for OSC feedback packets on `0.0.0.0:port`
/// and updates the shared cache.
pub fn start_feedback_listener(
    port: u16,
    state: Arc<Mutex<FeedbackState>>,
    update_tx: broadcast::Sender<FeedbackUpdate>,
) -> std::io::Result<FeedbackListenerHandle> {
    let socket = UdpSocket::bind(("0.0.0.0", port))?;
    socket.set_read_timeout(Some(std::time::Duration::from_millis(200)))?;

    let (cancel_tx, mut cancel_rx) = broadcast::channel::<()>(1);

    log::info!("Starting OSC feedback listener on UDP port {port}");

    std::thread::spawn(move || {
        let mut buf = [0u8; 65535];
        loop {
            if cancel_rx.try_recv().is_ok() {
                log::info!("Feedback listener received cancel; exiting");
                break;
            }
            let (len, _src) = match socket.recv_from(&mut buf) {
                Ok((len, src)) => (len, src),
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    continue;
                }
                Err(e) => {
                    log::warn!("Feedback UDP recv error: {e}");
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    continue;
                }
            };

            let payload = &buf[..len];
            let packet = match rosc::decoder::decode_udp(payload) {
                Ok((_remaining, packet)) => packet,
                Err(e) => {
                    log::debug!("Malformed OSC packet on feedback UDP port: {e}");
                    continue;
                }
            };

            handle_packet(&packet, &state, &update_tx);
        }
        log::info!("Feedback listener task exited cleanly");
    });

    Ok(FeedbackListenerHandle { cancel: cancel_tx })
}

fn handle_packet(
    packet: &OscPacket,
    state: &Arc<Mutex<FeedbackState>>,
    update_tx: &broadcast::Sender<FeedbackUpdate>,
) {
    match packet {
        OscPacket::Message(msg) => {
            if messages::feedback::is_known(&msg.addr) {
                let mut snapshot = state.lock().unwrap();
                let changed = snapshot.apply_message(msg);
                if changed {
                    let _ = update_tx.send(FeedbackUpdate {
                        state: snapshot.clone(),
                    });
                }
                log::debug!("Feedback: {} -> {:?}", msg.addr, msg.args);
            } else {
                log::debug!("Ignoring unknown feedback address: {}", msg.addr);
            }
        }
        OscPacket::Bundle(bundle) => {
            for inner in &bundle.content {
                handle_packet(inner, state, update_tx);
            }
        }
    }
}

trait OscTypeExt {
    fn int(&self) -> Option<i32>;
    fn string(&self) -> Option<String>;
    fn bool(&self) -> Option<bool>;
}

impl OscTypeExt for rosc::OscType {
    fn int(&self) -> Option<i32> {
        match self {
            rosc::OscType::Int(v) => Some(*v),
            rosc::OscType::Long(v) => Some(*v as i32),
            rosc::OscType::Float(v) => Some(*v as i32),
            rosc::OscType::Double(v) => Some(*v as i32),
            _ => None,
        }
    }

    fn string(&self) -> Option<String> {
        match self {
            rosc::OscType::String(s) => Some(s.clone()),
            _ => None,
        }
    }

    fn bool(&self) -> Option<bool> {
        match self {
            rosc::OscType::Bool(b) => Some(*b),
            rosc::OscType::Int(v) => Some(*v != 0),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(addr: &str, arg: rosc::OscType) -> OscMessage {
        OscMessage {
            addr: addr.to_string(),
            args: vec![arg],
        }
    }

    #[test]
    fn currentslide_updates_state() {
        let mut s = FeedbackState::default();
        assert!(s.apply_message(&msg(messages::feedback::CURRENT_SLIDE, rosc::OscType::Int(7))));
        assert_eq!(s.current_slide, Some(7));
        assert!(s.last_updated.is_some());
    }

    #[test]
    fn slidecount_updates_state() {
        let mut s = FeedbackState::default();
        s.apply_message(&msg(messages::feedback::SLIDE_COUNT, rosc::OscType::Int(42)));
        assert_eq!(s.slide_count, Some(42));
    }

    #[test]
    fn v2_state_presenting_sets_bool() {
        let mut s = FeedbackState::default();
        s.apply_message(&msg(
            "/oscpoint/state/presenting",
            rosc::OscType::Bool(true),
        ));
        assert_eq!(s.presenting, Some(true));
    }

    #[test]
    fn unknown_address_does_not_change_state() {
        let mut s = FeedbackState::default();
        let changed = s.apply_message(&msg("/random/unknown/addr", rosc::OscType::Int(1)));
        assert!(!changed);
        assert!(s.last_updated.is_none());
    }

    #[test]
    fn remember_outgoing_command_records_short_name() {
        let mut s = FeedbackState::default();
        s.remember_outgoing_command("/oscpoint/next");
        assert_eq!(s.last_command.as_deref(), Some("next"));
        assert!(s.last_command_time.is_some());
    }
}
