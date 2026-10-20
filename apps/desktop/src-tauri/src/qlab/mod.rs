//! QLab count-out: follows a QLab workspace over OSC (TCP) and shows how long
//! its running video and audio cues have left.
//!
//! Only cues that will actually end count: a looping background
//! (`infiniteLoop`, or a `playCount` above 1) or a cue without a duration is
//! ignored, so a show full of active cues still gives a true count-out. When
//! several linear cues of one kind run at once, the one that ends last is
//! shown. With nothing running, the cue at the playhead is shown as "next".
//!
//! QLab pushes no time, so [`client`] polls: `runningOrPausedCues` for what's
//! active (every 100 ms while anything is, 500 ms otherwise, and at once when
//! QLab reports a cue changed), then `valuesForKeys` per audio or video cue.
//!
//! One [`QLab`] hub per app: the client publishes [`QLabState`] to it, and the
//! QLab tab, the `/qlab` web page and the Syphon output all read the same
//! state.

pub mod client;
mod cues;
mod slip;

use crate::util::LockExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::watch;

use crate::config::QLabConfig;

/// Everything the views show.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QLabState {
    /// A QLab host is set
    pub configured: bool,
    /// Connected to the workspace (passcode accepted)
    pub connected: bool,
    /// What's happening while not connected, for the tab and the views
    pub message: Option<String>,
    /// Display name of the workspace being followed
    pub workspace: Option<String>,
    /// Every workspace open in QLab, for the tab's picker
    pub workspaces: Vec<String>,
    pub video: Option<Countdown>,
    pub audio: Option<Countdown>,
    /// The cue at the playhead
    pub next: Option<CueLabel>,
}

/// One cue counting out.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Countdown {
    pub number: String,
    pub name: String,
    /// Seconds left
    pub remaining: f64,
    /// Seconds in all
    pub duration: f64,
    pub paused: bool,
    /// Other linear cues of the same kind running and ending sooner
    pub others: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CueLabel {
    pub number: String,
    pub name: String,
}

// =============================================================================
// HUB
// =============================================================================

/// What the client restarts on.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientKey {
    pub host: String,
    pub port: u16,
    pub passcode: String,
    pub workspace: String,
}

/// How a client publishes. Gated, so a replaced client's last words can't
/// land after its replacement's first.
#[derive(Clone)]
pub struct Publisher {
    tx: Arc<watch::Sender<QLabState>>,
    live: Arc<AtomicBool>,
}

impl Publisher {
    pub fn publish(&self, state: QLabState) {
        if self.live.load(Ordering::Relaxed) {
            self.tx.send_if_modified(|s| {
                let changed = *s != state;
                *s = state;
                changed
            });
        }
    }
}

struct Running {
    key: ClientKey,
    shutdown: watch::Sender<bool>,
    live: Arc<AtomicBool>,
}

/// The app's QLab connection and its latest state.
pub struct QLab {
    tx: Arc<watch::Sender<QLabState>>,
    /// Hidden cue numbers; the running client follows changes live
    hidden: watch::Sender<Vec<String>>,
    running: Mutex<Option<Running>>,
}

impl Default for QLab {
    fn default() -> Self {
        Self {
            tx: Arc::new(watch::channel(QLabState::default()).0),
            hidden: watch::channel(Vec::new()).0,
            running: Mutex::new(None),
        }
    }
}

impl QLab {
    pub fn subscribe(&self) -> watch::Receiver<QLabState> {
        self.tx.subscribe()
    }

    pub fn current(&self) -> QLabState {
        self.tx.borrow().clone()
    }

    /// Start, stop or restart the client to match `cfg`. Idempotent, so it's
    /// safe on every config save.
    pub fn reconcile(&self, cfg: &QLabConfig) {
        // Hiding a cue mid-show must not drop the connection.
        let hidden: Vec<String> = cfg
            .hidden_cues
            .iter()
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .collect();
        self.hidden.send_if_modified(|h| {
            let changed = *h != hidden;
            *h = hidden;
            changed
        });

        let host = cfg.host.trim();
        let want = (!host.is_empty()).then(|| ClientKey {
            host: host.to_string(),
            port: cfg.port,
            passcode: cfg.passcode.clone(),
            workspace: cfg.workspace.trim().to_string(),
        });
        let mut slot = self.running.locked();
        if want.as_ref() == slot.as_ref().map(|r| &r.key) {
            return;
        }
        if let Some(old) = slot.take() {
            old.live.store(false, Ordering::Relaxed);
            let _ = old.shutdown.send(true);
        }
        self.tx.send_replace(QLabState {
            configured: want.is_some(),
            message: want.as_ref().map(|_| "Connecting…".to_string()),
            ..Default::default()
        });
        let Some(key) = want else { return };

        let live = Arc::new(AtomicBool::new(true));
        let publisher = Publisher {
            tx: Arc::clone(&self.tx),
            live: Arc::clone(&live),
        };
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        tauri::async_runtime::spawn(client::run(
            key.clone(),
            publisher,
            self.hidden.subscribe(),
            shutdown_rx,
        ));
        *slot = Some(Running {
            key,
            shutdown: shutdown_tx,
            live,
        });
    }
}

/// Push every change to the QLab tab as `qlab-state`.
pub async fn forward_to_frontend(app: AppHandle, mut rx: watch::Receiver<QLabState>) {
    while rx.changed().await.is_ok() {
        let state = rx.borrow_and_update().clone();
        let _ = app.emit("qlab-state", &state);
    }
}
