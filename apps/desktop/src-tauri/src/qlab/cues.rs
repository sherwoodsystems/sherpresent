//! Which cue each count-out shows. Fed QLab's `runningOrPausedCues` and
//! per-cue `valuesForKeys` replies; ignores loops, cues without a duration
//! and hidden cue numbers, and picks the cue that ends last per kind, sticking
//! with it until another ends clearly later.
//!
//! Time left comes from a [`Clock`] per cue, not straight from the replies:
//! one reading starts it, it runs on our own clock, and a reading every
//! [`SYNC_EVERY`] corrects it only if it has drifted [`RESYNC_DRIFT`]. Shown
//! straight, readings land whenever a reply does and the seconds stutter.
//! A cue is read again at once when it starts or stops elapsing (paused,
//! resumed), seen by comparing QLab's `runningCues` with what we last read.
//!
//! Logs (target `sherpresent_lib::qlab::cues`) at `info` whenever the active
//! list, a cue's playback shape or a shown cue changes, with the reason a cue
//! doesn't count; every reply at `trace`.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde_json::Value;
use tokio::time::Instant;

use super::{Countdown, CueLabel};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Kind {
    Video,
    Audio,
}

impl Kind {
    fn from_type(t: &str) -> Option<Kind> {
        match t.to_ascii_lowercase().as_str() {
            "video" => Some(Kind::Video),
            "audio" => Some(Kind::Audio),
            _ => None,
        }
    }
}

/// The keys asked of each active audio or video cue.
pub(super) const CUE_KEYS: &[&str] = &[
    "uniqueID",
    "number",
    "listName",
    "type",
    "duration",
    "percentActionElapsed",
    "isPaused",
    "infiniteLoop",
    "playCount",
    // QLab lists loaded or held cues as active without playing them.
    "isRunning",
    // For the log only.
    "holdLastFrame",
];

/// At or past this share of the action a cue has finished, even if QLab
/// still lists it (a video holding its last frame stays active).
const FINISHED: f64 = 0.999;

/// How often each active cue's time is read from QLab to check its clock.
pub(super) const SYNC_EVERY: Duration = Duration::from_secs(2);

/// After asking for a cue's values, how long a `runningCues` reply that
/// disagrees with them waits before asking again.
const RECHECK_AFTER: Duration = Duration::from_millis(300);

/// A clock this far from QLab's reading is reset to it; anything closer is
/// reply latency, and correcting it would make the seconds stutter.
const RESYNC_DRIFT: f64 = 0.25;

/// Longest time left a clock holds (115 days): keeps an absurd duration from
/// overflowing an `Instant`.
const MAX_LEFT: f64 = 1e7;

/// A cue's count-down, run locally between readings.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Clock {
    Running { ends_at: Instant },
    Paused { left: f64 },
}

impl Clock {
    fn start(p: &Playback, now: Instant) -> Clock {
        let left = p.remaining().min(MAX_LEFT);
        if p.paused {
            Clock::Paused { left }
        } else {
            Clock::Running {
                ends_at: now + Duration::from_secs_f64(left),
            }
        }
    }

    fn left(&self, now: Instant) -> f64 {
        match *self {
            Clock::Running { ends_at } => ends_at.saturating_duration_since(now).as_secs_f64(),
            Clock::Paused { left } => left,
        }
    }
}

/// An active audio or video cue.
#[derive(Debug, Clone, PartialEq)]
struct ActiveCue {
    kind: Kind,
    label: CueLabel,
    /// The last `valuesForKeys` reply; `None` until the first
    reading: Option<Reading>,
    /// When its values were last asked for; `None` to ask at the next poll
    asked: Option<Instant>,
}

impl ActiveCue {
    /// Take a `valuesForKeys` reply received at `now`.
    fn read(&mut self, p: Playback, now: Instant) {
        let clock = match &self.reading {
            // Still on time: keep it, so the seconds keep a steady beat.
            Some(r) if r.on_time(&p, now) => r.clock,
            Some(r) => {
                if let (Clock::Running { .. }, false) = (r.clock, p.paused) {
                    log::debug!(
                        "QLab cue {:?}: clock {:+.2}s off, reset",
                        self.label,
                        r.clock.left(now) - p.remaining()
                    );
                }
                Clock::start(&p, now)
            }
            None => Clock::start(&p, now),
        };
        self.reading = Some(Reading { playback: p, clock });
    }
}

/// What QLab last said about a cue's playback, and the clock it set.
#[derive(Debug, Clone, PartialEq)]
struct Reading {
    playback: Playback,
    clock: Clock,
}

impl Reading {
    /// The clock agrees with a new reading `p` closely enough to keep.
    fn on_time(&self, p: &Playback, now: Instant) -> bool {
        matches!(self.clock, Clock::Running { .. })
            && !p.paused
            && self.playback.duration == p.duration
            && (self.clock.left(now) - p.remaining()).abs() <= RESYNC_DRIFT
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Playback {
    duration: f64,
    /// 0 to 1 of the action
    elapsed: f64,
    paused: bool,
    looping: bool,
    /// Playing now (false when paused, or when QLab merely has it loaded)
    running: bool,
    /// Log only
    hold_last_frame: bool,
}

impl Playback {
    fn remaining(&self) -> f64 {
        (self.duration * (1.0 - self.elapsed.clamp(0.0, 1.0))).max(0.0)
    }

    /// Why this cue can't be counted down, if it can't.
    fn skip_reason(&self) -> Option<&'static str> {
        if self.looping {
            Some("looping")
        } else if self.duration <= 0.0 {
            Some("no duration")
        } else if self.elapsed >= FINISHED {
            Some("finished (holding last frame?)")
        } else if !self.running && !self.paused {
            Some("listed active but not playing (loaded?)")
        } else {
            None
        }
    }

    /// What's worth a log line when it changes: the cue's status, never the
    /// time, which moves on every reply.
    fn shape(&self) -> (bool, bool, bool, bool, Option<&'static str>) {
        (
            self.paused,
            self.looping,
            self.running,
            self.hold_last_frame,
            self.skip_reason(),
        )
    }
}

pub(super) fn str_field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(super) fn label(v: &Value) -> CueLabel {
    CueLabel {
        number: str_field(v, "number"),
        name: str_field(v, "listName"),
    }
}

/// The audio and video cues in a `runningOrPausedCues` reply, by unique ID,
/// including those inside groups (nested under `cues`).
fn active_media(data: &Value) -> Vec<(String, Kind, CueLabel)> {
    fn walk(v: &Value, out: &mut Vec<(String, Kind, CueLabel)>) {
        let Some(cues) = v.as_array() else { return };
        for cue in cues {
            let id = str_field(cue, "uniqueID");
            let kind = cue
                .get("type")
                .and_then(Value::as_str)
                .and_then(Kind::from_type);
            if let (false, Some(kind)) = (id.is_empty(), kind) {
                out.push((id, kind, label(cue)));
            }
            if let Some(children) = cue.get("cues") {
                walk(children, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(data, &mut out);
    out
}

/// A cue's `valuesForKeys` reply. Missing keys read as "not looping", "not
/// paused": older QLab versions don't know every key.
fn playback(data: &Value) -> Option<Playback> {
    let duration = data.get("duration").and_then(Value::as_f64)?;
    let play_count = data.get("playCount").and_then(Value::as_f64).unwrap_or(1.0);
    let infinite = data
        .get("infiniteLoop")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some(Playback {
        duration,
        elapsed: data
            .get("percentActionElapsed")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        paused: data
            .get("isPaused")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        looping: infinite || play_count > 1.0,
        running: data
            .get("isRunning")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        hold_last_frame: data
            .get("holdLastFrame")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// One line naming every active audio/video cue, in cue-number order.
fn summarize(cues: &HashMap<String, ActiveCue>) -> String {
    if cues.is_empty() {
        return "none".to_string();
    }
    let mut parts: Vec<String> = cues
        .iter()
        .map(|(id, c)| {
            format!(
                "{:?} {} \"{}\" [{}]",
                c.kind, c.label.number, c.label.name, id
            )
        })
        .collect();
    parts.sort();
    parts.join(", ")
}

/// How much later another cue must end before the count-out switches to it.
/// QLab answers each cue's query separately, so two cues ending together
/// (one film on two outputs, say) leapfrog each other by a few hundredths
/// on every reply; without this the view would swap between them ten times
/// a second.
const SWITCH_MARGIN: f64 = 1.0;

/// How long after a second turns over to publish it: never early.
const TICK_LATE: Duration = Duration::from_millis(2);

/// The active cues being followed, by unique ID, and which one per kind is
/// shown.
#[derive(Debug, Default)]
pub(super) struct ActiveCues {
    cues: HashMap<String, ActiveCue>,
    /// Cue numbers never counted (`QLabConfig::hidden_cues`)
    hidden: HashSet<String>,
    shown_video: Option<String>,
    shown_audio: Option<String>,
    /// The active list as last logged
    last_summary: String,
}

impl ActiveCues {
    /// Replace the set with a `runningOrPausedCues` reply, keeping what's
    /// known about cues still running. Returns the IDs to ask about: new
    /// cues, and those not read for [`SYNC_EVERY`].
    pub(super) fn set_running(&mut self, data: &Value, now: Instant) -> Vec<String> {
        let media = active_media(data);
        let mut next = HashMap::with_capacity(media.len());
        for (id, kind, label) in media {
            // QLab can list a cue twice (inside its group and on its own);
            // the first copy already carried over what we know.
            if next.contains_key(&id) {
                continue;
            }
            let known = self.cues.remove(&id);
            // A blank label in the list keeps what the cue's own reply said.
            let label = match &known {
                Some(k) if label.number.is_empty() && label.name.is_empty() => k.label.clone(),
                _ => label,
            };
            let (reading, asked) = known.map(|c| (c.reading, c.asked)).unwrap_or_default();
            next.insert(
                id,
                ActiveCue {
                    kind,
                    label,
                    reading,
                    asked,
                },
            );
        }
        let summary = summarize(&next);
        if summary != self.last_summary {
            log::info!("QLab active audio/video cues: {}", summary);
            self.last_summary = summary;
        }
        self.cues = next;
        self.reselect(now);
        self.due(now)
    }

    /// Compare a `runningCues` reply (cues whose time is elapsing) with what
    /// each cue last read: one that has paused or resumed since is read
    /// again. Returns the IDs to ask about.
    pub(super) fn set_elapsing(&mut self, data: &Value, now: Instant) -> Vec<String> {
        let elapsing: HashSet<String> = active_media(data).into_iter().map(|(id, ..)| id).collect();
        for (id, cue) in &mut self.cues {
            // Only clocks matter: a loaded or finished cue isn't elapsing
            // either way.
            let Some(r) = cue
                .reading
                .as_ref()
                .filter(|r| r.playback.skip_reason().is_none())
            else {
                continue;
            };
            let was = r.playback.running && !r.playback.paused;
            // Just asked: the answer may not reflect the change yet.
            let just_asked = cue.asked.is_some_and(|t| now < t + RECHECK_AFTER);
            if was != elapsing.contains(id) && !just_asked {
                log::debug!(
                    "QLab cue {:?} {}",
                    cue.label,
                    if was { "paused" } else { "resumed" }
                );
                cue.asked = None;
            }
        }
        self.due(now)
    }

    /// QLab says the cue changed: read it at the next poll rather than
    /// waiting out [`SYNC_EVERY`].
    pub(super) fn resync(&mut self, id: &str) {
        if let Some(cue) = self.cues.get_mut(id) {
            cue.asked = None;
        }
    }

    /// The cues to ask for values now: never asked, marked for a resync, or
    /// not asked for [`SYNC_EVERY`]. Marks them asked.
    fn due(&mut self, now: Instant) -> Vec<String> {
        self.cues
            .iter_mut()
            .filter(|(_, c)| c.asked.is_none_or(|t| now >= t + SYNC_EVERY))
            .map(|(id, c)| {
                c.asked = Some(now);
                id.clone()
            })
            .collect()
    }

    /// Apply a cue's `valuesForKeys` reply. A late reply for a cue that has
    /// since stopped is dropped rather than bringing it back.
    pub(super) fn set_values(&mut self, data: &Value, now: Instant) {
        let id = str_field(data, "uniqueID");
        if let Some(cue) = self.cues.get_mut(&id) {
            if let Some(p) = playback(data) {
                log::trace!("QLab cue {:?}: {:?}", cue.label, p);
                if cue.reading.as_ref().map(|r| r.playback.shape()) != Some(p.shape()) {
                    log::info!(
                        "QLab {:?} cue {} \"{}\": {:.1}s of {:.1}s left, paused={} running={} looping={} holdLastFrame={} -> {}",
                        cue.kind,
                        cue.label.number,
                        cue.label.name,
                        p.remaining(),
                        p.duration,
                        p.paused,
                        p.running,
                        p.looping,
                        p.hold_last_frame,
                        p.skip_reason().map_or("counts".to_string(), |r| format!("not counted: {}", r)),
                    );
                }
                cue.read(p, now);
            } else {
                log::info!("QLab cue {:?} reply has no duration: {}", cue.label, data);
            }
            let fresh = label(data);
            if !fresh.number.is_empty() || !fresh.name.is_empty() {
                cue.label = fresh;
            }
        }
        self.reselect(now);
    }

    pub(super) fn set_hidden(&mut self, hidden: &[String], now: Instant) {
        self.hidden = hidden.iter().map(|n| n.trim().to_string()).collect();
        self.reselect(now);
    }

    pub(super) fn is_empty(&self) -> bool {
        self.cues.is_empty()
    }

    /// The cues of `kind` that count, with their time left: linear, still
    /// playing (by QLab and by the clock), not hidden.
    fn linear(&self, kind: Kind, now: Instant) -> impl Iterator<Item = (&String, &ActiveCue, f64)> {
        self.cues.iter().filter_map(move |(id, c)| {
            let r = c.reading.as_ref()?;
            let left = r.clock.left(now);
            let counts = r.playback.skip_reason().is_none() && left > 0.0;
            (c.kind == kind && counts && !self.is_hidden(c)).then_some((id, c, left))
        })
    }

    fn is_hidden(&self, cue: &ActiveCue) -> bool {
        self.hidden.contains(cue.label.number.trim())
    }

    fn shown_mut(&mut self, kind: Kind) -> &mut Option<String> {
        match kind {
            Kind::Video => &mut self.shown_video,
            Kind::Audio => &mut self.shown_audio,
        }
    }

    /// Pick the cue each kind shows: the one that ends last, but the current
    /// one stays until another ends [`SWITCH_MARGIN`] later or it stops.
    pub(super) fn reselect(&mut self, now: Instant) {
        for kind in [Kind::Video, Kind::Audio] {
            // Ties go to the lower ID so the pick never depends on map order.
            let best = self
                .linear(kind, now)
                .max_by(|a, b| a.2.total_cmp(&b.2).then_with(|| b.0.cmp(a.0)))
                .map(|(id, _, left)| (id.clone(), left));
            let current = self.shown_mut(kind).clone();
            let current_left = current
                .as_ref()
                .and_then(|id| self.linear(kind, now).find(|(i, ..)| *i == id))
                .map(|(.., left)| left);
            let next = match (best, current_left) {
                (Some((_, best_left)), Some(left)) if best_left < left + SWITCH_MARGIN => {
                    current.clone()
                }
                (best, _) => best.map(|(id, _)| id),
            };
            if next != current {
                match next.as_ref().and_then(|id| self.cues.get(id)) {
                    Some(c) => log::info!(
                        "QLab {:?} count-out now shows {} \"{}\"",
                        kind,
                        c.label.number,
                        c.label.name
                    ),
                    None => log::info!("QLab {:?} count-out now shows nothing", kind),
                }
            }
            *self.shown_mut(kind) = next;
        }
    }

    fn shown(&self, kind: Kind, now: Instant) -> Option<(&ActiveCue, f64)> {
        let shown = match kind {
            Kind::Video => self.shown_video.as_ref(),
            Kind::Audio => self.shown_audio.as_ref(),
        }?;
        self.linear(kind, now)
            .find(|(id, ..)| *id == shown)
            .map(|(_, cue, left)| (cue, left))
    }

    /// What `kind` shows at `now`, its time left read off the cue's clock.
    pub(super) fn countdown(&self, kind: Kind, now: Instant) -> Option<Countdown> {
        let (cue, left) = self.shown(kind, now)?;
        let p = &cue.reading.as_ref()?.playback;
        Some(Countdown {
            number: cue.label.number.clone(),
            name: cue.label.name.clone(),
            remaining: left,
            duration: p.duration,
            paused: p.paused,
            others: self.linear(kind, now).count().saturating_sub(1) as u32,
        })
    }

    /// When a shown count-down's seconds next change, just after the moment,
    /// so the views tick on the second. `None` while nothing counts down.
    pub(super) fn next_second(&self, now: Instant) -> Option<Instant> {
        [Kind::Video, Kind::Audio]
            .into_iter()
            .filter_map(|kind| self.shown(kind, now))
            .filter(|(cue, _)| {
                matches!(
                    cue.reading,
                    Some(Reading {
                        clock: Clock::Running { .. },
                        ..
                    })
                )
            })
            .map(|(_, left)| {
                // The views round up from 1 ms before (`formatTime`), so the
                // number changes as `left - 0.001` passes a whole second.
                let shown = left - 0.001;
                now + Duration::from_secs_f64((shown - shown.floor()).max(0.0)) + TICK_LATE
            })
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::OnceLock;

    /// One moment for a whole test, so clocks read exactly what was set.
    fn t0() -> Instant {
        static T0: OnceLock<Instant> = OnceLock::new();
        *T0.get_or_init(Instant::now)
    }

    fn secs(s: f64) -> Instant {
        t0() + Duration::from_secs_f64(s)
    }

    fn values(id: &str, duration: f64, elapsed: f64) -> Value {
        json!({
            "uniqueID": id, "number": id, "listName": format!("Cue {}", id),
            "duration": duration, "percentActionElapsed": elapsed,
            "isPaused": false, "infiniteLoop": false, "playCount": 1
        })
    }

    #[test]
    fn test_active_media_includes_group_children_and_skips_others() {
        let data = json!([
            { "uniqueID": "g", "type": "Group", "number": "1", "listName": "Scene", "cues": [
                { "uniqueID": "v", "type": "Video", "number": "1.1", "listName": "Film" },
                { "uniqueID": "l", "type": "Light", "number": "1.2", "listName": "Wash" }
            ]},
            { "uniqueID": "a", "type": "Audio", "number": "2", "listName": "Walk-in" }
        ]);
        let ids: Vec<_> = active_media(&data)
            .into_iter()
            .map(|(id, kind, _)| (id, kind))
            .collect();
        assert_eq!(
            ids,
            vec![("v".into(), Kind::Video), ("a".into(), Kind::Audio)]
        );
    }

    #[test]
    fn test_loops_are_ignored_and_last_to_end_wins() {
        let mut cues = ActiveCues::default();
        cues.set_running(
            &json!([
                { "uniqueID": "bg", "type": "Video", "number": "1", "listName": "Background" },
                { "uniqueID": "short", "type": "Video", "number": "2", "listName": "Sting" },
                { "uniqueID": "long", "type": "Video", "number": "3", "listName": "Film" },
                { "uniqueID": "twice", "type": "Video", "number": "4", "listName": "Twice" },
            ]),
            t0(),
        );
        let mut bg = values("bg", 600.0, 0.0);
        bg["infiniteLoop"] = json!(true);
        let mut twice = values("twice", 300.0, 0.0);
        twice["playCount"] = json!(2);
        for v in [
            bg,
            twice,
            values("short", 10.0, 0.5),
            values("long", 100.0, 0.25),
        ] {
            cues.set_values(&v, t0());
        }

        let c = cues.countdown(Kind::Video, t0()).unwrap();
        assert_eq!(c.number, "long");
        assert_eq!(c.remaining, 75.0);
        assert_eq!(c.others, 1, "the sting, not the loops");
        assert!(cues.countdown(Kind::Audio, t0()).is_none());
    }

    #[test]
    fn test_only_loops_running_means_no_countdown() {
        let mut cues = ActiveCues::default();
        cues.set_running(&json!([{ "uniqueID": "bg", "type": "Audio" }]), t0());
        let mut bg = values("bg", 60.0, 0.1);
        bg["infiniteLoop"] = json!(true);
        cues.set_values(&bg, t0());
        assert!(cues.countdown(Kind::Audio, t0()).is_none());
    }

    #[test]
    fn test_stopped_cue_reply_is_dropped_and_playback_kept_across_polls() {
        let mut cues = ActiveCues::default();
        let running = json!([{ "uniqueID": "a", "type": "Audio" }]);
        cues.set_running(&running, t0());
        cues.set_values(&values("a", 20.0, 0.5), t0());
        // The next poll still lists it: what we know survives until the reply.
        cues.set_running(&running, t0());
        assert_eq!(cues.countdown(Kind::Audio, t0()).unwrap().remaining, 10.0);

        cues.set_running(&json!([]), t0());
        cues.set_values(&values("a", 20.0, 0.6), t0());
        assert!(cues.is_empty(), "late reply must not resurrect the cue");
    }

    #[test]
    fn test_near_ties_do_not_swap_the_shown_cue() {
        // One film on two outputs: their times leapfrog as replies arrive.
        let mut cues = ActiveCues::default();
        cues.set_running(
            &json!([
                { "uniqueID": "a", "type": "Video" },
                { "uniqueID": "b", "type": "Video" },
            ]),
            t0(),
        );
        cues.set_values(&values("a", 100.0, 0.500), t0());
        cues.set_values(&values("b", 100.0, 0.501), t0());
        let first = cues.countdown(Kind::Video, t0()).unwrap().number;
        for i in 0..20 {
            let (lead, lag) = if i % 2 == 0 { ("a", "b") } else { ("b", "a") };
            cues.set_values(&values(lead, 100.0, 0.502 + i as f64 * 0.001), t0());
            cues.set_values(&values(lag, 100.0, 0.503 + i as f64 * 0.001), t0());
            assert_eq!(cues.countdown(Kind::Video, t0()).unwrap().number, first);
        }
    }

    #[test]
    fn test_switches_when_another_cue_ends_clearly_later_or_shown_one_stops() {
        let mut cues = ActiveCues::default();
        cues.set_running(&json!([{ "uniqueID": "a", "type": "Video" }]), t0());
        cues.set_values(&values("a", 30.0, 0.0), t0());
        cues.set_running(
            &json!([
                { "uniqueID": "a", "type": "Video" },
                { "uniqueID": "b", "type": "Video" },
            ]),
            t0(),
        );
        cues.set_values(&values("b", 120.0, 0.0), t0());
        assert_eq!(cues.countdown(Kind::Video, t0()).unwrap().number, "b");

        cues.set_running(&json!([{ "uniqueID": "a", "type": "Video" }]), t0());
        assert_eq!(cues.countdown(Kind::Video, t0()).unwrap().number, "a");
    }

    #[test]
    fn test_hidden_cue_numbers_are_never_counted() {
        let mut cues = ActiveCues::default();
        cues.set_running(
            &json!([
                { "uniqueID": "a", "type": "Video" },
                { "uniqueID": "b", "type": "Video" },
            ]),
            t0(),
        );
        cues.set_values(&values("a", 300.0, 0.0), t0());
        cues.set_values(&values("b", 60.0, 0.0), t0());
        cues.set_hidden(&[" a ".to_string()], t0());
        let c = cues.countdown(Kind::Video, t0()).unwrap();
        assert_eq!((c.number.as_str(), c.others), ("b", 0));

        cues.set_hidden(&["a".to_string(), "b".to_string()], t0());
        assert!(cues.countdown(Kind::Video, t0()).is_none());
        cues.set_hidden(&[], t0());
        assert_eq!(cues.countdown(Kind::Video, t0()).unwrap().number, "a");
    }

    #[test]
    fn test_finished_cue_holding_its_last_frame_is_not_counted() {
        let mut cues = ActiveCues::default();
        cues.set_running(&json!([{ "uniqueID": "v", "type": "Video" }]), t0());
        let mut held = values("v", 30.0, 1.0);
        held["holdLastFrame"] = json!(true);
        cues.set_values(&held, t0());
        assert!(cues.countdown(Kind::Video, t0()).is_none());
    }

    #[test]
    fn test_cue_listed_active_but_not_playing_is_not_counted() {
        let mut cues = ActiveCues::default();
        cues.set_running(&json!([{ "uniqueID": "v", "type": "Video" }]), t0());
        let mut loaded = values("v", 3.4, 0.0);
        loaded["isRunning"] = json!(false);
        cues.set_values(&loaded, t0());
        assert!(cues.countdown(Kind::Video, t0()).is_none());

        // Paused still counts (it shows PAUSED), and so does playing.
        loaded["isPaused"] = json!(true);
        cues.set_values(&loaded, t0());
        assert!(cues.countdown(Kind::Video, t0()).is_some());
    }

    #[test]
    fn test_cue_listed_twice_keeps_its_playback_between_polls() {
        // A cue inside a group, also listed on its own: every poll must keep
        // its countdown rather than blanking until the cue's next reply.
        let listed_twice = json!([
            { "uniqueID": "g", "type": "Group", "cues": [{ "uniqueID": "v", "type": "Video" }] },
            { "uniqueID": "v", "type": "Video" },
        ]);
        let mut cues = ActiveCues::default();
        cues.set_running(&listed_twice, t0());
        cues.set_values(&values("v", 100.0, 0.4), t0());
        for _ in 0..3 {
            cues.set_running(&listed_twice, t0());
            assert_eq!(cues.countdown(Kind::Video, t0()).unwrap().remaining, 60.0);
        }
    }

    fn one_audio(duration: f64, elapsed: f64) -> ActiveCues {
        let mut cues = ActiveCues::default();
        cues.set_running(&json!([{ "uniqueID": "a", "type": "Audio" }]), t0());
        cues.set_values(&values("a", duration, elapsed), t0());
        cues
    }

    fn left(cues: &ActiveCues, at: Instant) -> Option<f64> {
        cues.countdown(Kind::Audio, at).map(|c| c.remaining)
    }

    fn close(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-6)
    }

    #[test]
    fn test_clock_counts_down_between_readings() {
        let cues = one_audio(20.0, 0.5);
        assert!(close(left(&cues, secs(0.0)), 10.0));
        assert!(close(left(&cues, secs(1.5)), 8.5));
        assert!(close(left(&cues, secs(9.75)), 0.25));
    }

    #[test]
    fn test_reading_close_to_the_clock_leaves_it_alone() {
        let mut cues = one_audio(20.0, 0.5);
        // 2 s later QLab says 8.1 s left (reply latency): keep the beat.
        cues.set_values(&values("a", 20.0, 0.595), secs(2.0));
        assert!(close(left(&cues, secs(2.0)), 8.0));
        // Then 7 s: someone scrubbed the cue. Follow QLab.
        cues.set_values(&values("a", 20.0, 0.65), secs(2.0));
        assert!(close(left(&cues, secs(2.0)), 7.0));
    }

    #[test]
    fn test_pause_stops_the_clock_and_resume_restarts_it() {
        let mut cues = one_audio(20.0, 0.5);
        let mut paused = values("a", 20.0, 0.55);
        paused["isPaused"] = json!(true);
        cues.set_values(&paused, secs(1.0));
        assert!(close(left(&cues, secs(1.0)), 9.0));
        assert!(close(left(&cues, secs(30.0)), 9.0));
        assert!(cues.next_second(secs(30.0)).is_none(), "nothing to tick");

        cues.set_values(&values("a", 20.0, 0.55), secs(30.0));
        assert!(close(left(&cues, secs(31.0)), 8.0));
    }

    #[test]
    fn test_cue_drops_out_when_its_clock_runs_out() {
        let mut cues = one_audio(10.0, 0.0);
        cues.reselect(secs(10.0));
        assert!(cues.countdown(Kind::Audio, secs(10.0)).is_none());
    }

    #[test]
    fn test_values_asked_every_sync_period_or_when_qlab_says_it_changed() {
        let running = json!([{ "uniqueID": "a", "type": "Audio" }]);
        let mut cues = ActiveCues::default();
        assert_eq!(cues.set_running(&running, secs(0.0)), vec!["a"]);
        assert!(cues.set_running(&running, secs(1.0)).is_empty());
        assert_eq!(cues.set_running(&running, secs(2.0)), vec!["a"]);
        cues.resync("a");
        assert_eq!(cues.set_running(&running, secs(2.1)), vec!["a"]);
    }

    #[test]
    fn test_next_second_is_just_after_the_shown_number_changes() {
        let cues = one_audio(100.0, 0.7995); // 20.05 s left
        let at = cues.next_second(t0()).unwrap();
        let ms = (at - t0()).as_secs_f64() * 1000.0;
        assert!((50.0..=53.0).contains(&ms), "{}", ms);
        // Shown as the views round it (`formatTime`): 21 before, 20 by then.
        let shown = |l: f64| (l - 0.001).ceil() as i64;
        assert_eq!(shown(left(&cues, t0()).unwrap()), 21);
        assert_eq!(shown(left(&cues, at).unwrap()), 20);
    }

    #[test]
    fn test_cue_leaving_or_rejoining_running_cues_is_read_again() {
        let audio = json!([{ "uniqueID": "a", "type": "Audio" }]);
        let nothing = json!([]);
        let mut cues = one_audio(20.0, 0.5);
        // Still elapsing, or too soon after the last ask: nothing to do.
        assert!(cues.set_elapsing(&audio, secs(0.5)).is_empty());
        assert!(cues.set_elapsing(&nothing, secs(0.1)).is_empty());
        // Paused: it left `runningCues`.
        assert_eq!(cues.set_elapsing(&nothing, secs(0.5)), vec!["a"]);
        let mut paused = values("a", 20.0, 0.525);
        paused["isPaused"] = json!(true);
        cues.set_values(&paused, secs(0.6));
        assert!(cues.set_elapsing(&nothing, secs(1.0)).is_empty());
        // Resumed.
        assert_eq!(cues.set_elapsing(&audio, secs(1.0)), vec!["a"]);
    }

    #[test]
    fn test_cues_that_do_not_count_are_not_rechecked() {
        let mut cues = ActiveCues::default();
        cues.set_running(&json!([{ "uniqueID": "v", "type": "Video" }]), t0());
        let mut loaded = values("v", 3.4, 0.0);
        loaded["isRunning"] = json!(false);
        cues.set_values(&loaded, t0());
        let mut held = values("v", 30.0, 1.0);
        held["isRunning"] = json!(true);
        for reading in [loaded, held] {
            cues.set_values(&reading, t0());
            assert!(cues.set_elapsing(&json!([]), secs(1.0)).is_empty());
        }
    }
}
