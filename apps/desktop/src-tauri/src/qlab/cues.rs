//! Which cue each count-out shows. Fed QLab's `runningOrPausedCues` and
//! per-cue `valuesForKeys` replies; ignores loops, cues without a duration
//! and hidden cue numbers, and picks the cue that ends last per kind, sticking
//! with it until another ends clearly later.
//!
//! Logs (target `sherpresent_lib::qlab::cues`) at `info` whenever the active
//! list, a cue's playback shape or a shown cue changes, with the reason a cue
//! doesn't count; every reply at `trace`.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

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

/// An active audio or video cue, and what QLab last said about its playback
/// (`None` until its first `valuesForKeys` reply).
#[derive(Debug, Clone, PartialEq)]
struct ActiveCue {
    kind: Kind,
    label: CueLabel,
    playback: Option<Playback>,
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
    /// known about cues still running. Returns the IDs to ask about.
    pub(super) fn set_running(&mut self, data: &Value) -> Vec<String> {
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
            let playback = known.and_then(|c| c.playback);
            next.insert(
                id,
                ActiveCue {
                    kind,
                    label,
                    playback,
                },
            );
        }
        let summary = summarize(&next);
        if summary != self.last_summary {
            log::info!("QLab active audio/video cues: {}", summary);
            self.last_summary = summary;
        }
        self.cues = next;
        self.reselect();
        self.cues.keys().cloned().collect()
    }

    /// Apply a cue's `valuesForKeys` reply. A late reply for a cue that has
    /// since stopped is dropped rather than bringing it back.
    pub(super) fn set_values(&mut self, data: &Value) {
        let id = str_field(data, "uniqueID");
        if let Some(cue) = self.cues.get_mut(&id) {
            if let Some(p) = playback(data) {
                log::trace!("QLab cue {:?}: {:?}", cue.label, p);
                if cue.playback.as_ref().map(Playback::shape) != Some(p.shape()) {
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
                cue.playback = Some(p);
            } else {
                log::info!("QLab cue {:?} reply has no duration: {}", cue.label, data);
            }
            let fresh = label(data);
            if !fresh.number.is_empty() || !fresh.name.is_empty() {
                cue.label = fresh;
            }
        }
        self.reselect();
    }

    pub(super) fn set_hidden(&mut self, hidden: &[String]) {
        self.hidden = hidden.iter().map(|n| n.trim().to_string()).collect();
        self.reselect();
    }

    pub(super) fn is_empty(&self) -> bool {
        self.cues.is_empty()
    }

    /// The cues of `kind` that count: linear, still playing, not hidden.
    fn linear(&self, kind: Kind) -> impl Iterator<Item = (&String, &ActiveCue, &Playback)> {
        self.cues.iter().filter_map(move |(id, c)| {
            let p = c.playback.as_ref()?;
            (c.kind == kind && p.skip_reason().is_none() && !self.is_hidden(c))
                .then_some((id, c, p))
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
    fn reselect(&mut self) {
        for kind in [Kind::Video, Kind::Audio] {
            // Ties go to the lower ID so the pick never depends on map order.
            let best = self
                .linear(kind)
                .max_by(|a, b| {
                    a.2.remaining()
                        .total_cmp(&b.2.remaining())
                        .then_with(|| b.0.cmp(a.0))
                })
                .map(|(id, _, p)| (id.clone(), p.remaining()));
            let current = self.shown_mut(kind).clone();
            let current_left = current
                .as_ref()
                .and_then(|id| self.linear(kind).find(|(i, ..)| *i == id))
                .map(|(.., p)| p.remaining());
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

    pub(super) fn countdown(&self, kind: Kind) -> Option<Countdown> {
        let shown = match kind {
            Kind::Video => self.shown_video.as_ref(),
            Kind::Audio => self.shown_audio.as_ref(),
        }?;
        let (_, cue, p) = self.linear(kind).find(|(id, ..)| *id == shown)?;
        Some(Countdown {
            number: cue.label.number.clone(),
            name: cue.label.name.clone(),
            remaining: p.remaining(),
            duration: p.duration,
            paused: p.paused,
            others: self.linear(kind).count().saturating_sub(1) as u32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        cues.set_running(&json!([
            { "uniqueID": "bg", "type": "Video", "number": "1", "listName": "Background" },
            { "uniqueID": "short", "type": "Video", "number": "2", "listName": "Sting" },
            { "uniqueID": "long", "type": "Video", "number": "3", "listName": "Film" },
            { "uniqueID": "twice", "type": "Video", "number": "4", "listName": "Twice" },
        ]));
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
            cues.set_values(&v);
        }

        let c = cues.countdown(Kind::Video).unwrap();
        assert_eq!(c.number, "long");
        assert_eq!(c.remaining, 75.0);
        assert_eq!(c.others, 1, "the sting, not the loops");
        assert!(cues.countdown(Kind::Audio).is_none());
    }

    #[test]
    fn test_only_loops_running_means_no_countdown() {
        let mut cues = ActiveCues::default();
        cues.set_running(&json!([{ "uniqueID": "bg", "type": "Audio" }]));
        let mut bg = values("bg", 60.0, 0.1);
        bg["infiniteLoop"] = json!(true);
        cues.set_values(&bg);
        assert!(cues.countdown(Kind::Audio).is_none());
    }

    #[test]
    fn test_stopped_cue_reply_is_dropped_and_playback_kept_across_polls() {
        let mut cues = ActiveCues::default();
        let running = json!([{ "uniqueID": "a", "type": "Audio" }]);
        cues.set_running(&running);
        cues.set_values(&values("a", 20.0, 0.5));
        // The next poll still lists it: what we know survives until the reply.
        cues.set_running(&running);
        assert_eq!(cues.countdown(Kind::Audio).unwrap().remaining, 10.0);

        cues.set_running(&json!([]));
        cues.set_values(&values("a", 20.0, 0.6));
        assert!(cues.is_empty(), "late reply must not resurrect the cue");
    }

    #[test]
    fn test_near_ties_do_not_swap_the_shown_cue() {
        // One film on two outputs: their times leapfrog as replies arrive.
        let mut cues = ActiveCues::default();
        cues.set_running(&json!([
            { "uniqueID": "a", "type": "Video" },
            { "uniqueID": "b", "type": "Video" },
        ]));
        cues.set_values(&values("a", 100.0, 0.500));
        cues.set_values(&values("b", 100.0, 0.501));
        let first = cues.countdown(Kind::Video).unwrap().number;
        for i in 0..20 {
            let (lead, lag) = if i % 2 == 0 { ("a", "b") } else { ("b", "a") };
            cues.set_values(&values(lead, 100.0, 0.502 + i as f64 * 0.001));
            cues.set_values(&values(lag, 100.0, 0.503 + i as f64 * 0.001));
            assert_eq!(cues.countdown(Kind::Video).unwrap().number, first);
        }
    }

    #[test]
    fn test_switches_when_another_cue_ends_clearly_later_or_shown_one_stops() {
        let mut cues = ActiveCues::default();
        cues.set_running(&json!([{ "uniqueID": "a", "type": "Video" }]));
        cues.set_values(&values("a", 30.0, 0.0));
        cues.set_running(&json!([
            { "uniqueID": "a", "type": "Video" },
            { "uniqueID": "b", "type": "Video" },
        ]));
        cues.set_values(&values("b", 120.0, 0.0));
        assert_eq!(cues.countdown(Kind::Video).unwrap().number, "b");

        cues.set_running(&json!([{ "uniqueID": "a", "type": "Video" }]));
        assert_eq!(cues.countdown(Kind::Video).unwrap().number, "a");
    }

    #[test]
    fn test_hidden_cue_numbers_are_never_counted() {
        let mut cues = ActiveCues::default();
        cues.set_running(&json!([
            { "uniqueID": "a", "type": "Video" },
            { "uniqueID": "b", "type": "Video" },
        ]));
        cues.set_values(&values("a", 300.0, 0.0));
        cues.set_values(&values("b", 60.0, 0.0));
        cues.set_hidden(&[" a ".to_string()]);
        let c = cues.countdown(Kind::Video).unwrap();
        assert_eq!((c.number.as_str(), c.others), ("b", 0));

        cues.set_hidden(&["a".to_string(), "b".to_string()]);
        assert!(cues.countdown(Kind::Video).is_none());
        cues.set_hidden(&[]);
        assert_eq!(cues.countdown(Kind::Video).unwrap().number, "a");
    }

    #[test]
    fn test_finished_cue_holding_its_last_frame_is_not_counted() {
        let mut cues = ActiveCues::default();
        cues.set_running(&json!([{ "uniqueID": "v", "type": "Video" }]));
        let mut held = values("v", 30.0, 1.0);
        held["holdLastFrame"] = json!(true);
        cues.set_values(&held);
        assert!(cues.countdown(Kind::Video).is_none());
    }

    #[test]
    fn test_cue_listed_active_but_not_playing_is_not_counted() {
        let mut cues = ActiveCues::default();
        cues.set_running(&json!([{ "uniqueID": "v", "type": "Video" }]));
        let mut loaded = values("v", 3.4, 0.0);
        loaded["isRunning"] = json!(false);
        cues.set_values(&loaded);
        assert!(cues.countdown(Kind::Video).is_none());

        // Paused still counts (it shows PAUSED), and so does playing.
        loaded["isPaused"] = json!(true);
        cues.set_values(&loaded);
        assert!(cues.countdown(Kind::Video).is_some());
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
        cues.set_running(&listed_twice);
        cues.set_values(&values("v", 100.0, 0.4));
        for _ in 0..3 {
            cues.set_running(&listed_twice);
            assert_eq!(cues.countdown(Kind::Video).unwrap().remaining, 60.0);
        }
    }
}
