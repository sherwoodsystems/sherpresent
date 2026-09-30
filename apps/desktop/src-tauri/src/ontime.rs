//! Minimal Ontime client: the running timer, for the notes video output.
//!
//! The stage view talks to Ontime from the browser; a native output has no
//! browser, so this does the same from Rust. It reads exactly what the stage
//! page's script reads (`runtime-data` messages: `timer` and `eventNow`), so
//! both show the same thing from the same Ontime version.

use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;

/// Same retry delay as the stage page.
const RECONNECT_DELAY: Duration = Duration::from_secs(2);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// What the notes output shows of Ontime.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct TimerState {
    pub connected: bool,
    /// Milliseconds remaining; negative is overtime
    pub current: Option<f64>,
    /// `play`, `pause`, `stop`, `armed`, `roll`
    pub playback: Option<String>,
    /// Title of the running event
    pub title: String,
}

/// Follow Ontime at `host:port` until `shutdown` flips, reconnecting on any
/// failure. Every change is published to `tx`.
pub async fn run(
    host: String,
    port: u16,
    tx: watch::Sender<TimerState>,
    mut shutdown: watch::Receiver<bool>,
) {
    let url = format!("ws://{}:{}/ws", host, port);
    loop {
        tokio::select! {
            _ = follow(&url, &tx) => {}
            _ = shutdown.changed() => {}
        }
        tx.send_if_modified(|t| std::mem::replace(&mut t.connected, false));
        if stopping(&shutdown) {
            return;
        }
        tokio::select! {
            _ = tokio::time::sleep(RECONNECT_DELAY) => {}
            _ = shutdown.changed() => {}
        }
        if stopping(&shutdown) {
            return;
        }
    }
}

/// Asked to stop, or the owner is gone (a dropped sender would otherwise make
/// `changed()` return immediately forever).
fn stopping(shutdown: &watch::Receiver<bool>) -> bool {
    *shutdown.borrow() || shutdown.has_changed().is_err()
}

/// One connection, until it drops.
async fn follow(url: &str, tx: &watch::Sender<TimerState>) {
    let mut ws =
        match tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(url)).await {
            Ok(Ok((ws, _))) => ws,
            Ok(Err(e)) => {
                log::debug!("Ontime connect to {} failed: {}", url, e);
                return;
            }
            Err(_) => {
                log::debug!("Ontime connect to {} timed out", url);
                return;
            }
        };
    log::info!("Connected to Ontime at {}", url);
    tx.send_if_modified(|t| !std::mem::replace(&mut t.connected, true));

    while let Some(Ok(msg)) = ws.next().await {
        let Message::Text(text) = msg else { continue };
        let Ok(v) = serde_json::from_str::<Value>(text.as_str()) else {
            continue;
        };
        tx.send_if_modified(|t| apply(t, &v));
    }
    log::info!("Disconnected from Ontime at {}", url);
}

/// Apply one Ontime message. Returns whether anything changed.
fn apply(t: &mut TimerState, v: &Value) -> bool {
    if v.get("tag").and_then(Value::as_str) != Some("runtime-data") {
        return false;
    }
    let Some(p) = v.get("payload") else {
        return false;
    };
    let mut next = t.clone();
    if let Some(timer) = p.get("timer").filter(|t| t.is_object()) {
        next.current = timer.get("current").and_then(Value::as_f64);
        next.playback = timer
            .get("playback")
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    // Like the stage page: a message without an event keeps the last title.
    if let Some(event) = p.get("eventNow").filter(|e| e.is_object()) {
        next.title = event
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
    }
    let changed = next != *t;
    *t = next;
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_reads_timer_and_event() {
        let mut t = TimerState {
            connected: true,
            ..Default::default()
        };
        let msg = serde_json::json!({
            "tag": "runtime-data",
            "payload": {
                "timer": { "current": 42000, "playback": "play", "duration": 60000 },
                "eventNow": { "title": "Keynote" }
            }
        });
        assert!(apply(&mut t, &msg));
        assert_eq!(t.current, Some(42000.0));
        assert_eq!(t.playback.as_deref(), Some("play"));
        assert_eq!(t.title, "Keynote");
        assert!(!apply(&mut t, &msg), "same message is no change");
    }

    #[test]
    fn test_apply_keeps_title_and_ignores_other_tags() {
        let mut t = TimerState {
            title: "Keynote".into(),
            ..Default::default()
        };
        let no_event = serde_json::json!({
            "tag": "runtime-data",
            "payload": { "timer": { "current": -5000, "playback": "play" }, "eventNow": null }
        });
        assert!(apply(&mut t, &no_event));
        assert_eq!(t.title, "Keynote");
        assert_eq!(t.current, Some(-5000.0));

        let other = serde_json::json!({ "tag": "client", "payload": {} });
        assert!(!apply(&mut t, &other));
    }
}
