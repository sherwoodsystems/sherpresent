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
//!
//! The same connection also fires the configured slide-change command
//! ([`fire_on_slide_change`]): it's already open and past the passcode, so a
//! slide change costs one TCP write.

pub mod client;
mod cues;
mod slip;

use crate::util::LockExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rosc::{OscMessage, OscType};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::{broadcast, mpsc, watch};

use crate::config::QLabConfig;
use crate::osc::state_manager::CachedState;

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
    /// The last slide-change command fired
    pub slide_trigger: Option<TriggerReport>,
}

/// How the last slide-change command went, for the QLab tab.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TriggerReport {
    pub slide: i32,
    /// Addresses sent, as QLab received them
    pub sent: Vec<String>,
    /// Why it wasn't sent, or what QLab answered other than `ok`
    pub error: Option<String>,
}

// =============================================================================
// SLIDE TRIGGER
// =============================================================================

/// The messages to fire for one slide change, addressed as the user wrote
/// them; the client scopes them to its workspace.
#[derive(Debug, Clone)]
pub struct SlideTrigger {
    pub slide: i32,
    pub messages: Vec<OscMessage>,
}

/// The slide-change command and the running client to send it through.
#[derive(Default)]
pub struct TriggerSlot {
    command: Mutex<String>,
    client: Mutex<Option<mpsc::UnboundedSender<SlideTrigger>>>,
}

/// A slide-change command is set and there's a QLab to send it to.
pub fn fires_on_slides(cfg: &QLabConfig) -> bool {
    !cfg.host.trim().is_empty() && !slide_messages(&slide_template(cfg), 0).is_empty()
}

/// Everything sent on a slide change, as one command for [`slide_messages`]:
/// the GO of the matching cue, then any custom lines. The cue number goes in
/// the address (`/go/S7`), where QLab takes it as text; as an argument it
/// would have to be sent as a string, not a number.
fn slide_template(cfg: &QLabConfig) -> String {
    let mut lines = Vec::new();
    if cfg.slide_go {
        // An address can't hold spaces or extra path segments.
        let prefix: String = cfg
            .slide_cue_prefix
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '/')
            .collect();
        lines.push(format!("/go/{}{{slide}}", prefix));
    }
    lines.push(cfg.slide_command.clone());
    lines.join("\n")
}

impl TriggerSlot {
    /// Send the command for `slide` to QLab. Never blocks.
    pub fn fire(&self, slide: i32) {
        let messages = slide_messages(&self.command.locked(), slide);
        if messages.is_empty() {
            return;
        }
        if let Some(client) = self.client.locked().as_ref() {
            let _ = client.send(SlideTrigger { slide, messages });
        }
    }
}

/// Parse the slide-change command: one message per line, address first, then
/// arguments (whole numbers are ints, other numbers floats, the rest strings;
/// quote one to force a string, as QLab wants for cue numbers: `/go "53"`).
/// `{slide}` becomes the slide number anywhere in a line. Lines that don't
/// start with `/` (blank, `#` comments) are skipped.
pub fn slide_messages(command: &str, slide: i32) -> Vec<OscMessage> {
    let slide = slide.to_string();
    command
        .lines()
        .map(|line| line.trim().replace("{slide}", &slide))
        .filter(|line| line.starts_with('/'))
        .map(|line| {
            let mut parts = tokens(&line).into_iter();
            let addr = parts.next().map(|(t, _)| t).unwrap_or_default();
            let args = parts
                .map(|(arg, quoted)| {
                    if quoted {
                        OscType::String(arg)
                    } else if let Ok(i) = arg.parse() {
                        OscType::Int(i)
                    } else if let Ok(f) = arg.parse() {
                        OscType::Float(f)
                    } else {
                        OscType::String(arg)
                    }
                })
                .collect();
            OscMessage { addr, args }
        })
        .collect()
}

/// Split on whitespace, keeping `"double quoted"` runs whole. Each token comes
/// with whether it was quoted.
fn tokens(line: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '"' {
            chars.next();
            let text: String = chars.by_ref().take_while(|&c| c != '"').collect();
            out.push((text, true));
        } else {
            let mut text = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                text.push(c);
                chars.next();
            }
            out.push((text, false));
        }
    }
    out
}

/// Which presentation states are slide changes worth firing on: a new slide
/// while a show keeps running. A show starting, ending, or the presentation
/// being switched isn't one.
#[derive(Default)]
struct SlideWatch {
    shown: Option<i32>,
}

impl SlideWatch {
    fn see(&mut self, state: &CachedState) -> Option<i32> {
        let now = Some(state.current_slide).filter(|&s| state.is_presenting && s > 0);
        let before = std::mem::replace(&mut self.shown, now);
        match (before, now) {
            (Some(before), Some(slide)) if before != slide => Some(slide),
            _ => None,
        }
    }
}

/// Fire the slide-change command on every slide change the StateManager
/// publishes, including its optimistic ones, so a press on any surface
/// reaches QLab before the presentation app has even moved.
pub async fn fire_on_slide_change(
    slot: Arc<TriggerSlot>,
    mut states: broadcast::Receiver<CachedState>,
) {
    let mut watch = SlideWatch::default();
    loop {
        match states.recv().await {
            Ok(state) => {
                if let Some(slide) = watch.see(&state) {
                    slot.fire(slide);
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
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
    trigger: Arc<TriggerSlot>,
}

impl Default for QLab {
    fn default() -> Self {
        Self {
            tx: Arc::new(watch::channel(QLabState::default()).0),
            hidden: watch::channel(Vec::new()).0,
            running: Mutex::new(None),
            trigger: Arc::default(),
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

    pub fn slide_trigger(&self) -> Arc<TriggerSlot> {
        Arc::clone(&self.trigger)
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

        *self.trigger.command.locked() = slide_template(cfg);

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
        *self.trigger.client.locked() = None;
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
        let (trigger_tx, trigger_rx) = mpsc::unbounded_channel();
        *self.trigger.client.locked() = Some(trigger_tx);
        tauri::async_runtime::spawn(client::run(
            key.clone(),
            publisher,
            self.hidden.subscribe(),
            trigger_rx,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slide_messages_parse_lines_and_args() {
        let msgs = slide_messages(
            "/cue/{slide}/start\n\n# note\n/cue/1/sliderLevel 0 -6.5 x",
            7,
        );
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].addr, "/cue/7/start");
        assert!(msgs[0].args.is_empty());
        assert_eq!(
            msgs[1].args,
            vec![
                OscType::Int(0),
                OscType::Float(-6.5),
                OscType::String("x".into())
            ]
        );
        assert!(slide_messages("  \n go", 1).is_empty());
    }

    #[test]
    fn test_quoted_args_stay_strings() {
        let msgs = slide_messages(r#"/go "53" "two words" 4"#, 1);
        assert_eq!(
            msgs[0].args,
            vec![
                OscType::String("53".into()),
                OscType::String("two words".into()),
                OscType::Int(4)
            ]
        );
    }

    #[test]
    fn test_slide_go_uses_the_cue_prefix() {
        let mut cfg = QLabConfig {
            host: "10.0.0.5".into(),
            ..Default::default()
        };
        assert!(!fires_on_slides(&cfg)); // off by default
        cfg.slide_go = true;
        assert!(fires_on_slides(&cfg));
        let addrs = |cfg: &QLabConfig| -> Vec<String> {
            slide_messages(&slide_template(cfg), 7)
                .into_iter()
                .map(|m| m.addr)
                .collect()
        };
        assert_eq!(addrs(&cfg), vec!["/go/S7"]);
        cfg.slide_cue_prefix = " Slide /".into();
        cfg.slide_command = "/cue/fx{slide}/start".into();
        assert_eq!(addrs(&cfg), vec!["/go/Slide7", "/cue/fx7/start"]);
        cfg.slide_cue_prefix = String::new();
        assert_eq!(addrs(&cfg)[0], "/go/7");
    }

    fn showing(slide: i32) -> CachedState {
        CachedState {
            is_open: true,
            is_presenting: slide > 0,
            current_slide: slide,
            ..Default::default()
        }
    }

    #[test]
    fn test_slide_watch_fires_only_within_a_show() {
        let mut w = SlideWatch::default();
        assert_eq!(w.see(&showing(1)), None); // show starts
        assert_eq!(w.see(&showing(1)), None); // zoom, notes...
        assert_eq!(w.see(&showing(2)), Some(2));
        assert_eq!(w.see(&showing(1)), Some(1));
        assert_eq!(w.see(&showing(0)), None); // show ends
        assert_eq!(w.see(&showing(5)), None); // next show starts
    }
}
