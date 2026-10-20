//! The QLab OSC client: one TCP connection (SLIP-framed OSC, port 53000 by
//! default), reconnecting on any failure.
//!
//! [`Session`] is the protocol, kept free of I/O so the tests can drive it
//! with QLab's replies; [`run`] is the socket around it.
//!
//! 1. `/workspaces`, then pick the configured workspace (or the first open).
//! 2. `/workspace/{id}/connect {passcode}`; `ok` (QLab 5 adds `:permissions`)
//!    or `badpass`.
//! 3. `/workspace/{id}/updates 1`, so QLab says when a cue changes or the
//!    playhead moves, and from then on poll `runningOrPausedCues`, each
//!    active audio or video cue's `valuesForKeys`, and the playhead cue's.
//!
//! Every reply is `/reply/{address} {json}` with `status` and `data`.
//!
//! Slide-change triggers arrive on a channel and go out the moment they do,
//! scoped to the connected workspace (`/go` → `/workspace/{id}/go`).

use std::collections::VecDeque;
use std::time::Duration;

use rosc::{OscMessage, OscPacket, OscType};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};
use tokio::time::{timeout, Instant};

use super::cues::{self, ActiveCues, Kind, CUE_KEYS};
use super::{slip, ClientKey, CueLabel, Publisher, QLabState, SlideTrigger, TriggerReport};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY: Duration = Duration::from_secs(2);
/// QLab delays progressively after each wrong passcode; don't hammer it.
const BADPASS_RETRY: Duration = Duration::from_secs(10);
/// Poll rate while anything is active: tenths are what a count-out needs.
const POLL_ACTIVE: Duration = Duration::from_millis(100);
/// Idle: a cue starting also triggers an immediate poll via `/updates`.
const POLL_IDLE: Duration = Duration::from_millis(500);
const PLAYHEAD_EVERY: Duration = Duration::from_secs(1);
/// Asking again for a workspace that isn't open yet.
const WORKSPACES_EVERY: Duration = Duration::from_secs(2);
/// Polls are answered within milliseconds; this long without a word means
/// the connection is dead even if TCP hasn't noticed.
const SILENCE_LIMIT: Duration = Duration::from_secs(5);
/// Trigger addresses awaiting QLab's reply. QLab answers every TCP message,
/// so this only caps a misbehaving server.
const PENDING_TRIGGERS: usize = 32;

/// Follow QLab until `shutdown` flips.
pub async fn run(
    key: ClientKey,
    out: Publisher,
    mut hidden: watch::Receiver<Vec<String>>,
    mut triggers: mpsc::UnboundedReceiver<SlideTrigger>,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        let (message, delay) = tokio::select! {
            end = connect_and_follow(&key, &out, &mut hidden, &mut triggers) => end,
            _ = shutdown.changed() => return,
        };
        log::info!("QLab {}:{}: {}", key.host, key.port, message);
        out.publish(QLabState {
            configured: true,
            message: Some(message),
            ..Default::default()
        });
        if stopping(&shutdown) {
            return;
        }
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = shutdown.changed() => {}
        }
        if stopping(&shutdown) {
            return;
        }
    }
}

/// Asked to stop, or the owner is gone.
fn stopping(shutdown: &watch::Receiver<bool>) -> bool {
    *shutdown.borrow() || shutdown.has_changed().is_err()
}

/// One connection, until it ends. Returns why, and how long to wait before
/// the next attempt.
async fn connect_and_follow(
    key: &ClientKey,
    out: &Publisher,
    hidden: &mut watch::Receiver<Vec<String>>,
    triggers: &mut mpsc::UnboundedReceiver<SlideTrigger>,
) -> (String, Duration) {
    let addr = format!("{}:{}", key.host, key.port);
    let stream = match timeout(CONNECT_TIMEOUT, TcpStream::connect(&addr)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return (format!("Can't reach QLab at {}: {}", addr, e), RETRY),
        Err(_) => return (format!("Timed out connecting to QLab at {}", addr), RETRY),
    };
    let _ = stream.set_nodelay(true);
    let (mut rd, mut wr) = stream.into_split();
    log::info!("Connected to QLab at {}", addr);

    let mut session = Session::new(key.clone(), Instant::now());
    session.set_hidden(&hidden.borrow_and_update());
    let mut decoder = slip::Decoder::default();
    let mut buf = vec![0u8; 64 * 1024];
    let mut outgoing = session.start();
    // Slide changes from while we were away are stale: firing them now would
    // play cues the show has moved past.
    while triggers.try_recv().is_ok() {}

    loop {
        for msg in outgoing.drain(..) {
            if let Err(e) = send(&mut wr, msg).await {
                return (format!("Lost connection to QLab: {}", e), RETRY);
            }
        }
        out.publish(session.state());
        if let Some(end) = session.ended.take() {
            return end;
        }

        tokio::select! {
            read = rd.read(&mut buf) => {
                let n = match read {
                    Ok(0) => return ("QLab closed the connection".to_string(), RETRY),
                    Ok(n) => n,
                    Err(e) => return (format!("Lost connection to QLab: {}", e), RETRY),
                };
                let now = Instant::now();
                for packet in decoder.feed(&buf[..n]) {
                    match rosc::decoder::decode_udp(&packet) {
                        Ok((_, packet)) => each_message(packet, &mut |m| {
                            outgoing.extend(session.handle(&m, now))
                        }),
                        Err(e) => log::debug!("QLab sent an undecodable packet: {:?}", e),
                    }
                }
            }
            _ = tokio::time::sleep_until(session.wake_at()) => {
                outgoing.extend(session.tick(Instant::now()));
            }
            Some(trigger) = triggers.recv() => {
                outgoing.extend(session.fire(trigger));
            }
            Ok(()) = hidden.changed() => {
                session.set_hidden(&hidden.borrow_and_update());
            }
        }
    }
}

fn each_message(packet: OscPacket, f: &mut impl FnMut(OscMessage)) {
    match packet {
        OscPacket::Message(m) => f(m),
        OscPacket::Bundle(b) => b.content.into_iter().for_each(|p| each_message(p, f)),
    }
}

async fn send(wr: &mut tokio::net::tcp::OwnedWriteHalf, msg: OscMessage) -> Result<(), String> {
    let bytes = rosc::encoder::encode(&OscPacket::Message(msg)).map_err(|e| format!("{:?}", e))?;
    wr.write_all(&slip::encode(&bytes))
        .await
        .map_err(|e| e.to_string())
}

fn osc(addr: impl Into<String>, args: Vec<OscType>) -> OscMessage {
    OscMessage {
        addr: addr.into(),
        args,
    }
}

// =============================================================================
// SESSION
// =============================================================================

#[derive(Debug, Clone, PartialEq)]
enum Phase {
    /// Waiting for `/workspaces`, or for the wanted workspace to be opened
    Discovering,
    /// Passcode sent
    Connecting {
        id: String,
        name: String,
    },
    Connected {
        id: String,
        name: String,
    },
}

/// One connection's protocol state.
pub struct Session {
    key: ClientKey,
    phase: Phase,
    /// Workspace display names, as last listed
    workspaces: Vec<String>,
    cues: ActiveCues,
    next: Option<CueLabel>,
    /// Why we aren't connected yet
    message: Option<String>,
    last_heard: Instant,
    next_poll: Instant,
    next_playhead: Instant,
    next_workspaces: Instant,
    slide_trigger: Option<TriggerReport>,
    /// Addresses of fired triggers QLab hasn't answered yet
    pending_triggers: VecDeque<String>,
    /// Set when the connection should end: why, and the retry delay
    pub ended: Option<(String, Duration)>,
}

impl Session {
    pub fn new(key: ClientKey, now: Instant) -> Self {
        Self {
            key,
            phase: Phase::Discovering,
            workspaces: Vec::new(),
            cues: ActiveCues::default(),
            next: None,
            message: Some("Looking for a QLab workspace…".to_string()),
            last_heard: now,
            next_poll: now,
            next_playhead: now,
            next_workspaces: now + WORKSPACES_EVERY,
            slide_trigger: None,
            pending_triggers: VecDeque::new(),
            ended: None,
        }
    }

    pub fn set_hidden(&mut self, hidden: &[String]) {
        self.cues.set_hidden(hidden);
    }

    pub fn start(&self) -> Vec<OscMessage> {
        vec![osc("/workspaces", vec![])]
    }

    pub fn state(&self) -> QLabState {
        let (connected, workspace) = match &self.phase {
            Phase::Connected { name, .. } => (true, Some(name.clone())),
            Phase::Connecting { name, .. } => (false, Some(name.clone())),
            Phase::Discovering => (false, None),
        };
        QLabState {
            configured: true,
            connected,
            message: if connected {
                None
            } else {
                self.message.clone()
            },
            workspace,
            workspaces: self.workspaces.clone(),
            video: connected
                .then(|| self.cues.countdown(Kind::Video))
                .flatten(),
            audio: connected
                .then(|| self.cues.countdown(Kind::Audio))
                .flatten(),
            next: connected.then(|| self.next.clone()).flatten(),
            slide_trigger: self.slide_trigger.clone(),
        }
    }

    /// The messages to send for a slide change, scoped to the workspace.
    /// Nothing while not connected: QLab would refuse them.
    pub fn fire(&mut self, trigger: SlideTrigger) -> Vec<OscMessage> {
        let Phase::Connected { id, .. } = &self.phase else {
            log::info!(
                "QLab trigger for slide {} skipped: not connected",
                trigger.slide
            );
            self.slide_trigger = Some(TriggerReport {
                slide: trigger.slide,
                sent: vec![],
                error: Some("Not connected to QLab".to_string()),
            });
            return vec![];
        };
        let messages: Vec<OscMessage> = trigger
            .messages
            .into_iter()
            .map(|m| osc(in_workspace(id, &m.addr), m.args))
            .collect();
        let sent: Vec<String> = messages.iter().map(|m| m.addr.clone()).collect();
        log::info!("QLab trigger for slide {}: {:?}", trigger.slide, sent);
        self.pending_triggers.extend(sent.iter().cloned());
        while self.pending_triggers.len() > PENDING_TRIGGERS {
            self.pending_triggers.pop_front();
        }
        self.slide_trigger = Some(TriggerReport {
            slide: trigger.slide,
            sent,
            error: None,
        });
        messages
    }

    /// QLab's answer to a fired trigger. A refusal is reported, not treated
    /// as the follow connection failing: the passcode may allow viewing but
    /// not control.
    fn trigger_reply(&mut self, addr: &str, status: &str, data: &Value) -> bool {
        let Some(i) = self.pending_triggers.iter().position(|a| a == addr) else {
            return false;
        };
        self.pending_triggers.remove(i);
        if status != "ok" {
            let why = match status {
                "denied" => "denied: the passcode doesn't allow control".to_string(),
                _ if data.is_null() => status.to_string(),
                _ => format!("{} ({})", status, data),
            };
            log::warn!("QLab refused trigger {}: {}", addr, why);
            if let Some(report) = &mut self.slide_trigger {
                if report.sent.iter().any(|a| a == addr) {
                    report.error = Some(format!("{}: {}", addr, why));
                }
            }
        }
        true
    }

    /// When [`tick`](Self::tick) next has something to do.
    pub fn wake_at(&self) -> Instant {
        let silence = self.last_heard + SILENCE_LIMIT;
        match self.phase {
            Phase::Connected { .. } => self.next_poll.min(self.next_playhead).min(silence),
            Phase::Discovering => self.next_workspaces.min(silence),
            Phase::Connecting { .. } => silence,
        }
    }

    pub fn tick(&mut self, now: Instant) -> Vec<OscMessage> {
        if now >= self.last_heard + SILENCE_LIMIT {
            self.ended = Some(("QLab stopped answering".to_string(), RETRY));
            return vec![];
        }
        let mut out = Vec::new();
        match &self.phase {
            Phase::Connected { id, .. } => {
                if now >= self.next_poll {
                    out.push(osc(
                        format!("/workspace/{}/runningOrPausedCues", id),
                        vec![],
                    ));
                    let every = if self.cues.is_empty() {
                        POLL_IDLE
                    } else {
                        POLL_ACTIVE
                    };
                    self.next_poll = now + every;
                }
                if now >= self.next_playhead {
                    out.push(osc(
                        format!("/workspace/{}/cue/playhead/valuesForKeys", id),
                        vec![keys_arg(&["uniqueID", "number", "listName"])],
                    ));
                    self.next_playhead = now + PLAYHEAD_EVERY;
                }
            }
            Phase::Discovering => {
                if now >= self.next_workspaces {
                    out.push(osc("/workspaces", vec![]));
                    self.next_workspaces = now + WORKSPACES_EVERY;
                }
            }
            Phase::Connecting { .. } => {}
        }
        out
    }

    /// Apply one message from QLab. Returns what to send in response.
    pub fn handle(&mut self, msg: &OscMessage, now: Instant) -> Vec<OscMessage> {
        self.last_heard = now;
        if let Some(addr) = msg.addr.strip_prefix("/reply") {
            let Some(OscType::String(json)) = msg.args.first() else {
                return vec![];
            };
            let Ok(reply) = serde_json::from_str::<Value>(json) else {
                return vec![];
            };
            self.reply(addr, &reply, now)
        } else if msg.addr.starts_with("/update/") {
            self.update(&msg.addr, now);
            vec![]
        } else {
            vec![]
        }
    }

    fn reply(&mut self, addr: &str, reply: &Value, now: Instant) -> Vec<OscMessage> {
        let status = reply.get("status").and_then(Value::as_str).unwrap_or("ok");
        let data = reply.get("data").unwrap_or(&Value::Null);

        if self.trigger_reply(addr, status, data) {
            return vec![];
        }
        if status == "denied" {
            self.ended = Some((
                "QLab refused the connection: check the OSC passcode".to_string(),
                BADPASS_RETRY,
            ));
            return vec![];
        }

        if addr == "/workspaces" {
            return self.workspaces_reply(data, now);
        }
        if addr.ends_with("/connect") {
            return self.connect_reply(data, now);
        }
        let Phase::Connected { id, .. } = &self.phase else {
            return vec![];
        };

        if addr.ends_with("/runningOrPausedCues") {
            // Anything but a list means nothing can be confirmed running:
            // clear rather than keep showing the last answer.
            let empty = Value::Array(vec![]);
            let data = if status == "ok" {
                data
            } else {
                log::info!("QLab runningOrPausedCues replied {}: {}", status, data);
                &empty
            };
            let keys = keys_arg(CUE_KEYS);
            return self
                .cues
                .set_running(data)
                .into_iter()
                .map(|cue| {
                    osc(
                        format!("/workspace/{}/cue_id/{}/valuesForKeys", id, cue),
                        vec![keys.clone()],
                    )
                })
                .collect();
        }
        if addr.contains("/cue/playhead/") {
            // An error here just means no cue is at the playhead.
            self.next = (status == "ok")
                .then(|| cues::label(data))
                .filter(|l| !l.number.is_empty() || !l.name.is_empty());
            return vec![];
        }
        if addr.contains("/cue_id/") && addr.ends_with("/valuesForKeys") && status == "ok" {
            self.cues.set_values(data);
        }
        vec![]
    }

    fn workspaces_reply(&mut self, data: &Value, now: Instant) -> Vec<OscMessage> {
        let list: Vec<(String, String)> = data
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|w| {
                        (
                            cues::str_field(w, "uniqueID"),
                            cues::str_field(w, "displayName"),
                        )
                    })
                    .filter(|(id, _)| !id.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        self.workspaces = list.iter().map(|(_, name)| name.clone()).collect();
        if !matches!(self.phase, Phase::Discovering) {
            return vec![];
        }

        let want = self.key.workspace.as_str();
        let found = if want.is_empty() {
            list.first()
        } else {
            list.iter()
                .find(|(id, name)| id == want || same_workspace(name, want))
        };
        let Some((id, name)) = found.cloned() else {
            self.message = Some(if list.is_empty() {
                "No workspace open in QLab".to_string()
            } else {
                format!("Workspace \"{}\" isn't open in QLab", want)
            });
            self.next_workspaces = now + WORKSPACES_EVERY;
            return vec![];
        };

        self.message = Some(format!("Connecting to \"{}\"…", name));
        let args = if self.key.passcode.is_empty() {
            vec![]
        } else {
            vec![OscType::String(self.key.passcode.clone())]
        };
        let addr = format!("/workspace/{}/connect", id);
        self.phase = Phase::Connecting { id, name };
        vec![osc(addr, args)]
    }

    fn connect_reply(&mut self, data: &Value, now: Instant) -> Vec<OscMessage> {
        let Phase::Connecting { id, name } = self.phase.clone() else {
            return vec![];
        };
        let answer = data.as_str().unwrap_or_default();
        if answer == "ok" || answer.starts_with("ok:") {
            log::info!("QLab connected to workspace \"{}\" ({})", name, answer);
            self.phase = Phase::Connected {
                id: id.clone(),
                name,
            };
            self.message = None;
            self.next_poll = now;
            self.next_playhead = now;
            return vec![osc(
                format!("/workspace/{}/updates", id),
                vec![OscType::Int(1)],
            )];
        }
        if answer == "badpass" {
            self.ended = Some((
                "Wrong OSC passcode for this workspace".to_string(),
                BADPASS_RETRY,
            ));
            return vec![];
        }
        // `error`: the workspace closed in between. Look again.
        self.phase = Phase::Discovering;
        self.message = Some(format!("Workspace \"{}\" closed", name));
        self.next_workspaces = now;
        vec![]
    }

    /// `/update/workspace/{id}/...`: something changed in QLab.
    fn update(&mut self, addr: &str, now: Instant) {
        let Phase::Connected { id, .. } = &self.phase else {
            return;
        };
        let Some(rest) = addr.strip_prefix(&format!("/update/workspace/{}", id)) else {
            return;
        };
        if rest == "/disconnect" {
            self.ended = Some(("QLab closed the workspace".to_string(), RETRY));
        } else if rest.ends_with("/playbackPosition") {
            self.next_playhead = now;
        } else if rest.starts_with("/cue_id/") {
            // A cue started, stopped or changed: look now, not at the next poll.
            self.next_poll = now;
        }
    }
}

/// `/go` → `/workspace/{id}/go`, so a trigger reaches the followed workspace
/// rather than every one open. Addresses already naming a workspace are kept.
fn in_workspace(id: &str, addr: &str) -> String {
    if addr.starts_with("/workspace/") {
        addr.to_string()
    } else {
        format!("/workspace/{}{}", id, addr)
    }
}

/// `valuesForKeys` takes the keys as one JSON array string.
fn keys_arg(keys: &[&str]) -> OscType {
    OscType::String(serde_json::to_string(keys).unwrap_or_default())
}

/// `Show` matches a workspace displayed as `Show.qlab5`.
fn same_workspace(display_name: &str, want: &str) -> bool {
    let strip = |s: &str| {
        let s = s.trim();
        s.rsplit_once(".qlab")
            .map(|(base, _)| base.to_string())
            .unwrap_or_else(|| s.to_string())
            .to_lowercase()
    };
    strip(display_name) == strip(want)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn key(passcode: &str, workspace: &str) -> ClientKey {
        ClientKey {
            host: "127.0.0.1".into(),
            port: 53000,
            passcode: passcode.into(),
            workspace: workspace.into(),
        }
    }

    fn reply(addr: &str, status: &str, data: Value) -> OscMessage {
        let body = json!({ "address": addr, "status": status, "data": data });
        osc(
            format!("/reply{}", addr),
            vec![OscType::String(body.to_string())],
        )
    }

    fn addrs(msgs: &[OscMessage]) -> Vec<&str> {
        msgs.iter().map(|m| m.addr.as_str()).collect()
    }

    /// A session connected to workspace `W`.
    fn connected(now: Instant) -> Session {
        let mut s = Session::new(key("1234", ""), now);
        s.handle(
            &reply(
                "/workspaces",
                "ok",
                json!([{ "uniqueID": "W", "displayName": "Show.qlab5" }]),
            ),
            now,
        );
        s.handle(
            &reply("/workspace/W/connect", "ok", json!("ok:view|edit|control")),
            now,
        );
        s
    }

    #[test]
    fn test_connects_with_passcode_and_subscribes() {
        let now = Instant::now();
        let mut s = Session::new(key("1234", ""), now);
        assert_eq!(addrs(&s.start()), vec!["/workspaces"]);

        let out = s.handle(
            &reply(
                "/workspaces",
                "ok",
                json!([{ "uniqueID": "W", "displayName": "Show.qlab5" }]),
            ),
            now,
        );
        assert_eq!(addrs(&out), vec!["/workspace/W/connect"]);
        assert_eq!(out[0].args, vec![OscType::String("1234".into())]);
        assert!(!s.state().connected);

        let out = s.handle(
            &reply("/workspace/W/connect", "ok", json!("ok:view|edit|control")),
            now,
        );
        assert_eq!(addrs(&out), vec!["/workspace/W/updates"]);
        let state = s.state();
        assert!(state.connected);
        assert_eq!(state.workspace.as_deref(), Some("Show.qlab5"));
        assert_eq!(state.message, None);
    }

    #[test]
    fn test_bad_passcode_ends_with_long_retry() {
        let now = Instant::now();
        let mut s = Session::new(key("0000", ""), now);
        s.handle(
            &reply(
                "/workspaces",
                "ok",
                json!([{ "uniqueID": "W", "displayName": "Show" }]),
            ),
            now,
        );
        s.handle(&reply("/workspace/W/connect", "ok", json!("badpass")), now);
        let (_, delay) = s.ended.clone().unwrap();
        assert_eq!(delay, BADPASS_RETRY);
    }

    #[test]
    fn test_picks_named_workspace_and_waits_for_it() {
        let now = Instant::now();
        let mut s = Session::new(key("", "Main"), now);
        let out = s.handle(
            &reply(
                "/workspaces",
                "ok",
                json!([{ "uniqueID": "X", "displayName": "Other.qlab5" }]),
            ),
            now,
        );
        assert!(out.is_empty());
        assert!(s.state().message.unwrap().contains("isn't open"));
        assert_eq!(s.state().workspaces, vec!["Other.qlab5"]);

        let out = s.handle(
            &reply(
                "/workspaces",
                "ok",
                json!([{ "uniqueID": "X", "displayName": "Other.qlab5" }, { "uniqueID": "M", "displayName": "Main.qlab5" }]),
            ),
            now,
        );
        assert_eq!(addrs(&out), vec!["/workspace/M/connect"]);
        assert!(out[0].args.is_empty(), "no passcode configured");
    }

    #[test]
    fn test_polls_then_asks_each_media_cue() {
        let now = Instant::now();
        let mut s = connected(now);
        let out = s.tick(now);
        assert_eq!(
            addrs(&out),
            vec![
                "/workspace/W/runningOrPausedCues",
                "/workspace/W/cue/playhead/valuesForKeys"
            ]
        );

        let out = s.handle(
            &reply(
                "/workspace/W/runningOrPausedCues",
                "ok",
                json!([
                    { "uniqueID": "v", "type": "Video", "number": "5", "listName": "Film" },
                    { "uniqueID": "f", "type": "Fade", "number": "6", "listName": "Fade" }
                ]),
            ),
            now,
        );
        assert_eq!(addrs(&out), vec!["/workspace/W/cue_id/v/valuesForKeys"]);

        s.handle(
            &reply(
                "/workspace/W/cue_id/v/valuesForKeys",
                "ok",
                json!({ "uniqueID": "v", "number": "5", "listName": "Film", "duration": 40.0,
                        "percentActionElapsed": 0.5, "isPaused": false, "infiniteLoop": false, "playCount": 1 }),
            ),
            now,
        );
        let video = s.state().video.unwrap();
        assert_eq!((video.number.as_str(), video.remaining), ("5", 20.0));

        // Anything active: polls come at the fast rate from the next one on.
        let next = s.next_poll;
        s.tick(next);
        assert_eq!(s.next_poll, next + POLL_ACTIVE);
    }

    #[test]
    fn test_error_reply_clears_active_cues() {
        let now = Instant::now();
        let mut s = connected(now);
        s.handle(
            &reply(
                "/workspace/W/runningOrPausedCues",
                "ok",
                json!([{ "uniqueID": "v", "type": "Video", "number": "5", "listName": "Film" }]),
            ),
            now,
        );
        s.handle(
            &reply(
                "/workspace/W/cue_id/v/valuesForKeys",
                "ok",
                json!({ "uniqueID": "v", "duration": 40.0, "percentActionElapsed": 0.5 }),
            ),
            now,
        );
        assert!(s.state().video.is_some());

        // QLab 5.3+ answers some queries with an error when nothing is active.
        s.handle(
            &reply("/workspace/W/runningOrPausedCues", "error", Value::Null),
            now,
        );
        assert!(s.state().video.is_none(), "a stale cue must not linger");
    }

    #[test]
    fn test_playhead_reply_and_error_clear_next() {
        let now = Instant::now();
        let mut s = connected(now);
        s.handle(
            &reply(
                "/workspace/W/cue/playhead/valuesForKeys",
                "ok",
                json!({ "uniqueID": "n", "number": "7", "listName": "Interval" }),
            ),
            now,
        );
        assert_eq!(s.state().next.unwrap().name, "Interval");
        s.handle(
            &reply(
                "/workspace/W/cue/playhead/valuesForKeys",
                "error",
                Value::Null,
            ),
            now,
        );
        assert!(s.state().next.is_none());
    }

    #[test]
    fn test_cue_update_triggers_immediate_poll() {
        let now = Instant::now();
        let mut s = connected(now);
        s.tick(now);
        let later = now + Duration::from_millis(50);
        s.handle(&osc("/update/workspace/W/cue_id/v", vec![]), later);
        assert_eq!(s.wake_at(), later);
    }

    #[test]
    fn test_silence_ends_session() {
        let now = Instant::now();
        let mut s = connected(now);
        s.tick(now + SILENCE_LIMIT);
        assert!(s.ended.is_some());
    }

    #[test]
    fn test_denied_ends_session() {
        let now = Instant::now();
        let mut s = connected(now);
        s.handle(
            &reply("/workspace/W/runningOrPausedCues", "denied", Value::Null),
            now,
        );
        assert!(s.ended.is_some());
    }

    /// The real socket loop against a fake QLab speaking SLIP-framed OSC.
    #[tokio::test]
    async fn test_follows_a_fake_qlab_over_tcp() {
        use std::sync::atomic::AtomicBool;
        use std::sync::Arc;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut decoder = slip::Decoder::default();
            let mut buf = [0u8; 4096];
            loop {
                let n = sock.read(&mut buf).await.unwrap();
                if n == 0 {
                    return;
                }
                for packet in decoder.feed(&buf[..n]) {
                    let Ok((_, OscPacket::Message(m))) = rosc::decoder::decode_udp(&packet) else {
                        continue;
                    };
                    let data = match m.addr.as_str() {
                        "/workspaces" => json!([{ "uniqueID": "W", "displayName": "Show" }]),
                        "/workspace/W/connect" if m.args == vec![OscType::String("pw".into())] => {
                            json!("ok")
                        }
                        a if a.ends_with("/runningOrPausedCues") => json!([
                            { "uniqueID": "v", "type": "Video", "number": "1", "listName": "Film" }
                        ]),
                        a if a.ends_with("/cue_id/v/valuesForKeys") => json!({
                            "uniqueID": "v", "number": "1", "listName": "Film", "duration": 60.0,
                            "percentActionElapsed": 0.25, "isPaused": false,
                            "infiniteLoop": false, "playCount": 1
                        }),
                        _ => continue,
                    };
                    let body = json!({ "address": m.addr, "status": "ok", "data": data });
                    let reply = osc(
                        format!("/reply{}", m.addr),
                        vec![OscType::String(body.to_string())],
                    );
                    let bytes = rosc::encoder::encode(&OscPacket::Message(reply)).unwrap();
                    sock.write_all(&slip::encode(&bytes)).await.unwrap();
                }
            }
        });

        let tx = Arc::new(watch::channel(QLabState::default()).0);
        let mut rx = tx.subscribe();
        let out = Publisher {
            tx,
            live: Arc::new(AtomicBool::new(true)),
        };
        let (stop, stop_rx) = watch::channel(false);
        let key = ClientKey {
            port,
            ..key("pw", "")
        };
        let (_hidden, hidden_rx) = watch::channel(Vec::new());
        let (_triggers, triggers_rx) = mpsc::unbounded_channel();
        tokio::spawn(run(key, out, hidden_rx, triggers_rx, stop_rx));

        let state = timeout(Duration::from_secs(5), rx.wait_for(|s| s.video.is_some()))
            .await
            .expect("no countdown within 5s")
            .unwrap()
            .clone();
        assert!(state.connected);
        assert_eq!(state.workspace.as_deref(), Some("Show"));
        let video = state.video.unwrap();
        assert_eq!((video.name.as_str(), video.remaining), ("Film", 45.0));
        stop.send(true).unwrap();
    }

    fn trigger(slide: i32, command: &str) -> SlideTrigger {
        SlideTrigger {
            slide,
            messages: super::super::slide_messages(command, slide),
        }
    }

    #[test]
    fn test_trigger_is_scoped_to_the_workspace() {
        let now = Instant::now();
        let mut s = connected(now);
        let out = s.fire(trigger(4, "/go\n/cue/S{slide}/start\n/workspace/X/go"));
        assert_eq!(
            addrs(&out),
            vec![
                "/workspace/W/go",
                "/workspace/W/cue/S4/start",
                "/workspace/X/go"
            ]
        );
        let report = s.state().slide_trigger.unwrap();
        assert_eq!((report.slide, report.error), (4, None));
    }

    #[test]
    fn test_trigger_skipped_until_connected() {
        let now = Instant::now();
        let mut s = Session::new(key("", ""), now);
        assert!(s.fire(trigger(2, "/go")).is_empty());
        assert!(s.state().slide_trigger.unwrap().error.is_some());
    }

    #[test]
    fn test_refused_trigger_is_reported_and_keeps_the_connection() {
        let now = Instant::now();
        let mut s = connected(now);
        s.fire(trigger(3, "/go"));
        s.handle(&reply("/workspace/W/go", "denied", Value::Null), now);
        assert!(s.ended.is_none());
        let error = s.state().slide_trigger.unwrap().error.unwrap();
        assert!(error.contains("denied"), "{}", error);
    }

    #[test]
    fn test_same_workspace_ignores_extension_and_case() {
        assert!(same_workspace("Main Show.qlab5", "main show"));
        assert!(same_workspace("Main Show", "Main Show.qlab5"));
        assert!(!same_workspace("Main Show 2.qlab5", "Main Show"));
    }
}
