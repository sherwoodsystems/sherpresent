//! QLab count-out: follows a QLab workspace over OSC (TCP) and shows how long
//! its running video and audio cues have left.
//!
//! Only cues that will actually end count: a looping background
//! (`infiniteLoop`, or a `playCount` above 1) or a cue without a duration is
//! ignored, so a show full of active cues still gives a true count-out. When
//! several linear cues of one kind run at once, the one that ends last is
//! shown. With nothing running, the cue at the playhead is shown as "next".
//!
//! QLab pushes no time, so [`client`] polls `runningOrPausedCues` for what's
//! active (every 100 ms while anything is, 500 ms otherwise, and at once when
//! QLab reports a cue changed), and reads each audio or video cue's time with
//! `valuesForKeys` when it starts and every 2 s after. Between readings the
//! time runs on a local clock, published just after each second turns over,
//! so the count-out ticks on true seconds rather than when replies land.
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
    /// The step fired: `3`, or `3.1` for a slide's first build
    pub step: String,
    /// Addresses sent, as QLab received them
    pub sent: Vec<String>,
    /// Why it wasn't sent, or what QLab answered other than `ok`
    pub error: Option<String>,
}

// =============================================================================
// SLIDE TRIGGER
// =============================================================================

/// The messages to fire for one step of the show, addressed as the user
/// wrote them; the client scopes them to its workspace.
#[derive(Debug, Clone)]
pub struct SlideTrigger {
    /// [`Step::label`], for the report
    pub step: String,
    pub messages: Vec<OscMessage>,
}

/// A place in the show worth a cue: a slide, or one of its builds.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Step {
    slide: i32,
    /// Builds played on the slide (0 = none)
    build: i32,
}

impl Step {
    /// The cue number after the prefix: `3` for the slide, `3.1` for its
    /// first build, `3.2` for the second.
    fn label(&self) -> String {
        match self.build {
            0 => self.slide.to_string(),
            build => format!("{}.{}", self.slide, build),
        }
    }
}

/// What to send for each step.
#[derive(Debug, Clone, Default, PartialEq)]
struct Template {
    /// Start cue `{prefix}{step}` (`slideGo`)
    cue_prefix: Option<String>,
    /// Custom lines (`slideCommand`), sent on slide changes only
    command: String,
}

impl Template {
    fn new(cfg: &QLabConfig) -> Self {
        // An address can't hold spaces or extra path segments.
        let prefix = cfg
            .slide_cue_prefix
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '/')
            .collect();
        Self {
            cue_prefix: cfg.slide_go.then_some(prefix),
            command: cfg.slide_command.clone(),
        }
    }

    /// The messages for arriving at `step`. The cue is started with `start`,
    /// which plays it without moving the playhead, so the operator's standby
    /// stays put (`/go/S7` would jump the playhead to it); the cue number goes
    /// in the address, where QLab takes it as text. Custom lines go only with
    /// a new slide: they're written per slide (`{slide}`).
    fn messages(&self, step: Step, new_slide: bool) -> Vec<OscMessage> {
        let cue = self.cue_prefix.iter().map(|prefix| OscMessage {
            addr: format!("/cue/{}{}/start", prefix, step.label()),
            args: vec![],
        });
        let custom = if new_slide {
            slide_messages(&self.command, step.slide)
        } else {
            vec![]
        };
        cue.chain(custom).collect()
    }
}

/// The slide-change command and the running client to send it through.
#[derive(Default)]
pub struct TriggerSlot {
    template: Mutex<Template>,
    client: Mutex<Option<mpsc::UnboundedSender<SlideTrigger>>>,
}

/// A slide-change command is set and there's a QLab to send it to.
pub fn fires_on_slides(cfg: &QLabConfig) -> bool {
    let step = Step { slide: 1, build: 0 };
    !cfg.host.trim().is_empty() && !Template::new(cfg).messages(step, true).is_empty()
}

impl TriggerSlot {
    /// Send what's due for arriving at `step` to QLab. Never blocks.
    fn fire(&self, step: Step, new_slide: bool) {
        let messages = self.template.locked().messages(step, new_slide);
        if messages.is_empty() {
            return;
        }
        if let Some(client) = self.client.locked().as_ref() {
            let _ = client.send(SlideTrigger {
                step: step.label(),
                messages,
            });
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

/// Which presentation states are worth firing on: a new slide or build
/// step while a show keeps running, forwards or back. A show starting,
/// ending, or the presentation being switched isn't one.
#[derive(Default)]
struct SlideWatch {
    shown: Option<Step>,
}

impl SlideWatch {
    /// The step to fire, and whether it's on a new slide.
    fn see(&mut self, state: &CachedState) -> Option<(Step, bool)> {
        let now = (state.is_presenting && state.current_slide > 0).then(|| Step {
            slide: state.current_slide,
            build: state.current_build.unwrap_or(0).max(0),
        });
        let before = std::mem::replace(&mut self.shown, now);
        match (before, now) {
            (Some(before), Some(step)) if before != step => {
                Some((step, step.slide != before.slide))
            }
            _ => None,
        }
    }
}

/// Fire on every slide and build step the StateManager publishes, including
/// its optimistic ones, so a press on any surface reaches QLab before the
/// presentation app has even moved.
pub async fn fire_on_slide_change(
    slot: Arc<TriggerSlot>,
    mut states: broadcast::Receiver<CachedState>,
) {
    let mut watch = SlideWatch::default();
    loop {
        match states.recv().await {
            Ok(state) => {
                if let Some((step, new_slide)) = watch.see(&state) {
                    slot.fire(step, new_slide);
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

        *self.trigger.template.locked() = Template::new(cfg);

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

    fn step(slide: i32, build: i32) -> Step {
        Step { slide, build }
    }

    fn addrs(cfg: &QLabConfig, step: Step, new_slide: bool) -> Vec<String> {
        Template::new(cfg)
            .messages(step, new_slide)
            .into_iter()
            .map(|m| m.addr)
            .collect()
    }

    #[test]
    fn test_slide_go_starts_the_cue_named_by_prefix_and_step() {
        let mut cfg = QLabConfig {
            host: "10.0.0.5".into(),
            ..Default::default()
        };
        assert!(!fires_on_slides(&cfg)); // off by default
        cfg.slide_go = true;
        assert!(fires_on_slides(&cfg));
        assert_eq!(addrs(&cfg, step(7, 0), true), vec!["/cue/S7/start"]);
        assert_eq!(addrs(&cfg, step(7, 2), false), vec!["/cue/S7.2/start"]);
        cfg.slide_cue_prefix = " Slide /".into();
        assert_eq!(addrs(&cfg, step(7, 0), true), vec!["/cue/Slide7/start"]);
        cfg.slide_cue_prefix = String::new();
        assert_eq!(addrs(&cfg, step(7, 1), false), vec!["/cue/7.1/start"]);
    }

    #[test]
    fn test_custom_lines_go_with_new_slides_only() {
        let mut cfg = QLabConfig {
            host: "10.0.0.5".into(),
            slide_command: "/cue/fx{slide}/start".into(),
            ..Default::default()
        };
        assert!(fires_on_slides(&cfg));
        assert_eq!(addrs(&cfg, step(7, 0), true), vec!["/cue/fx7/start"]);
        assert!(addrs(&cfg, step(7, 1), false).is_empty());
        cfg.slide_go = true;
        assert_eq!(
            addrs(&cfg, step(8, 2), true),
            vec!["/cue/S8.2/start", "/cue/fx8/start"]
        );
    }

    fn showing(slide: i32) -> CachedState {
        building(slide, None)
    }

    fn building(slide: i32, build: Option<i32>) -> CachedState {
        CachedState {
            is_open: true,
            is_presenting: slide > 0,
            current_slide: slide,
            current_build: build,
            ..Default::default()
        }
    }

    #[test]
    fn test_slide_watch_fires_only_within_a_show() {
        let mut w = SlideWatch::default();
        assert_eq!(w.see(&showing(1)), None); // show starts
        assert_eq!(w.see(&showing(1)), None); // zoom, notes...
        assert_eq!(w.see(&showing(2)), Some((step(2, 0), true)));
        assert_eq!(w.see(&showing(1)), Some((step(1, 0), true)));
        assert_eq!(w.see(&showing(0)), None); // show ends
        assert_eq!(w.see(&showing(5)), None); // next show starts
    }

    #[test]
    fn test_slide_watch_fires_each_build_forwards_and_back() {
        let mut w = SlideWatch::default();
        w.see(&building(2, Some(0)));
        assert_eq!(w.see(&building(3, Some(0))), Some((step(3, 0), true)));
        assert_eq!(w.see(&building(3, Some(1))), Some((step(3, 1), false)));
        assert_eq!(w.see(&building(3, Some(2))), Some((step(3, 2), false)));
        assert_eq!(w.see(&building(4, Some(0))), Some((step(4, 0), true)));
        // Back onto slide 3, shown with its builds played.
        assert_eq!(w.see(&building(3, Some(2))), Some((step(3, 2), true)));
        assert_eq!(w.see(&building(3, Some(1))), Some((step(3, 1), false)));
        // Builds unknown and none played read the same: no second cue.
        assert_eq!(w.see(&building(3, Some(0))), Some((step(3, 0), false)));
        assert_eq!(w.see(&building(3, None)), None);
    }
}
