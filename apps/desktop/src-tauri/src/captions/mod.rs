//! Live caption capture, translation, and fan-out.
//!
//! Wires the audio capture thread to a streaming caption provider and
//! publishes the results three ways, mirroring how notes and status already
//! reach their consumers:
//!
//! - a `broadcast` channel consumed by the LAN overlay page
//! - a replay buffer so a browser that connects late sees current captions
//! - Tauri events for the in-app monitor

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::{broadcast, mpsc, watch};

use crate::config::CaptionsConfig;

pub mod audio;
pub mod provider;

use provider::{ProviderConfig, ProviderEvent};

/// How many finalized segments the replay buffer keeps.
///
/// Enough to repopulate a reconnecting overlay; captions are ephemeral, so
/// there is no reason to hold a full transcript here.
const BUFFER_CAPACITY: usize = 8;

/// One caption line.
///
/// While a line is being spoken it is republished repeatedly with `is_final`
/// false and the same `id`, so consumers replace rather than append. On turn
/// completion it is published once more with `is_final` true.
#[derive(Debug, Clone, Serialize)]
pub struct CaptionSegment {
    pub id: u64,
    /// Transcript in the speaker's original language.
    pub source: String,
    /// Translation in the configured target language.
    pub translated: String,
    #[serde(rename = "final")]
    pub is_final: bool,
    pub timestamp: u64,
}

/// Connection state of the engine.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum CaptionEngineState {
    Stopped,
    Starting,
    Running,
    Reconnecting,
    Error(String),
}

/// Status snapshot for the UI.
#[derive(Debug, Clone, Serialize)]
pub struct CaptionStatus {
    pub state: CaptionEngineState,
    /// Seconds of audio streamed this session; drives the cost readout.
    #[serde(rename = "elapsedSeconds")]
    pub elapsed_seconds: u64,
    pub reconnects: u32,
    pub provider: String,
    /// Whether this session translates at all (source/target differ). The
    /// overlay uses this to decide whether a blank `translated` field means
    /// "translation not started yet" (hold the previous line) versus
    /// "captions-only mode" (show source text immediately).
    #[serde(rename = "translateEnabled")]
    pub translate_enabled: bool,
}

/// Everything the `/captions` overlay page renders with, pushed live over its
/// socket so a Settings change restyles open overlays without a reload.
///
/// Clamped and sanitized here, once, because it reaches a stylesheet: the page
/// drops these straight into CSS custom properties.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlaySettings {
    /// px at a 1920-wide frame; the page scales it with the frame
    pub font_size: u16,
    /// Visual rows of text on screen, not caption segments: one long sentence
    /// can wrap to several rows, and those count.
    pub max_lines: u8,
    /// `#rrggbb` / `#rgb` / `transparent`
    pub chroma_color: String,
    /// % of frame height
    pub safe_area: f32,
    /// % of frame width
    pub width: f32,
    pub shadow: bool,
    /// Opaque box behind each row (closed-caption look)
    pub background: bool,
    /// `#rrggbb` / `#rgb` fill of that box
    pub box_color: String,
    /// Seconds of silence before the lines clear; 0 = never
    pub clear_after: f32,
}

impl OverlaySettings {
    pub fn from_config(c: &CaptionsConfig) -> Self {
        Self {
            font_size: c.font_size,
            max_lines: c.max_lines,
            chroma_color: c.chroma_color.clone(),
            safe_area: c.safe_area,
            width: c.width,
            shadow: c.shadow,
            background: c.background,
            box_color: c.box_color.clone(),
            clear_after: c.clear_after,
        }
        .clamped()
    }

    /// The one place every range lives. Applied to config values and to URL
    /// overrides alike, so neither path can drift from the other.
    fn clamped(self) -> Self {
        use crate::config::{
            default_caption_clear_after, default_caption_safe_area, default_caption_width,
            DEFAULT_CAPTION_BOX_COLOR, DEFAULT_CHROMA_COLOR,
        };
        let chroma_color = if self.chroma_color.trim().eq_ignore_ascii_case("transparent") {
            "transparent".to_string()
        } else {
            sanitize_hex_color(&self.chroma_color, DEFAULT_CHROMA_COLOR)
        };
        Self {
            font_size: self.font_size.clamp(12, 240),
            max_lines: self.max_lines.clamp(1, 6),
            chroma_color,
            // No `transparent`: the box is opaque by design.
            box_color: sanitize_hex_color(&self.box_color, DEFAULT_CAPTION_BOX_COLOR),
            safe_area: finite_clamp(self.safe_area, 0.0, 40.0, default_caption_safe_area()),
            width: finite_clamp(self.width, 20.0, 100.0, default_caption_width()),
            clear_after: finite_clamp(self.clear_after, 0.0, 120.0, default_caption_clear_after()),
            ..self
        }
    }

    /// These settings with an overlay URL's query-string pins applied
    /// (`size`, `lines`, `safe`, `width`, `bg`, `shadow`, `box`, `boxcolor`).
    ///
    /// Not `clear_after`: the silence timeout is one app-wide timer
    /// ([`run_silence_clear`]) that blanks every overlay and output together,
    /// so a per-page value couldn't be honoured.
    ///
    /// Applied server-side to the page template *and* to every `settings`
    /// message on that page's socket, so a pinned value simply never changes
    /// there. Unparseable values are ignored rather than pinning anything.
    pub fn with_overrides(&self, query: &HashMap<String, String>) -> Self {
        let num = |key: &str| query.get(key).and_then(|v| v.trim().parse::<f32>().ok());
        let flag = |key: &str| query.get(key).map(|v| v == "1");
        let mut s = self.clone();
        // `as` saturates (and maps NaN to 0), and `clamped` does the rest.
        if let Some(v) = num("size") {
            s.font_size = v.round() as u16;
        }
        if let Some(v) = num("lines") {
            s.max_lines = v.round() as u8;
        }
        if let Some(v) = num("safe") {
            s.safe_area = v;
        }
        if let Some(v) = num("width") {
            s.width = v;
        }
        if let Some(v) = query.get("bg") {
            s.chroma_color = v.clone();
        }
        if let Some(v) = flag("shadow") {
            s.shadow = v;
        }
        if let Some(v) = flag("box") {
            s.background = v;
        }
        if let Some(v) = query.get("boxcolor") {
            s.box_color = v.clone();
        }
        s.clamped()
    }
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self::from_config(&CaptionsConfig::default())
    }
}

/// `f32::clamp` passes NaN through, and NaN in a CSS `calc()` voids the rule.
pub(crate) fn finite_clamp(v: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v.clamp(min, max)
    } else {
        fallback
    }
}

/// Normalize a colour into something safe to drop into a CSS declaration.
///
/// Only `#rgb` / `#rrggbb` (with or without the `#`) is accepted; anything
/// else becomes `fallback`. This is a stylesheet injection guard, since the
/// value reaches the page from both config and an untrusted query string.
pub(crate) fn sanitize_hex_color(raw: &str, fallback: &str) -> String {
    let s = raw.trim();
    let hex = s.strip_prefix('#').unwrap_or(s);
    if matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        format!("#{}", hex)
    } else {
        fallback.to_string()
    }
}

/// A message published to the overlay page.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CaptionUpdate {
    Segment {
        segment: CaptionSegment,
    },
    Status {
        status: CaptionStatus,
    },
    /// The silence timeout fired: blank the screen. See [`run_silence_clear`].
    Clear,
}

impl CaptionUpdate {
    /// The wire form every overlay socket and native output receives.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// The `settings` message every overlay and output receives.
pub fn settings_message(settings: &OverlaySettings) -> String {
    serde_json::json!({ "type": "settings", "settings": settings }).to_string()
}

/// Shared handles the engine publishes through.
#[derive(Clone)]
pub struct CaptionSinks {
    pub broadcast: tokio::sync::broadcast::Sender<CaptionUpdate>,
    pub buffer: Arc<Mutex<VecDeque<CaptionSegment>>>,
    pub status: Arc<Mutex<CaptionStatus>>,
    /// Live overlay styling. A `watch` channel rather than a plain value so the
    /// web server can both read it fresh on every page load and push updates
    /// to already-open overlay tabs without a restart.
    pub overlay: tokio::sync::watch::Sender<OverlaySettings>,
}

impl Default for CaptionSinks {
    fn default() -> Self {
        Self {
            broadcast: broadcast::channel(64).0,
            buffer: Arc::default(),
            status: Arc::default(),
            overlay: watch::channel(OverlaySettings::default()).0,
        }
    }
}

impl CaptionSinks {
    /// Restyle every open overlay and output to match `config`. A no-op when
    /// nothing visible changed, so it's cheap to call on every save and on
    /// every slider tick.
    pub fn set_overlay(&self, config: &CaptionsConfig) {
        let next = OverlaySettings::from_config(config);
        self.overlay.send_if_modified(|cur| {
            let changed = *cur != next;
            if changed {
                *cur = next;
            }
            changed
        });
    }

    fn publish(&self, app: &AppHandle, update: CaptionUpdate) {
        // A send error just means no overlay browser is connected.
        let _ = self.broadcast.send(update.clone());

        match &update {
            CaptionUpdate::Segment { segment } => {
                let _ = app.emit("caption-segment", segment);
            }
            CaptionUpdate::Status { status } => {
                let _ = app.emit("caption-status", status);
            }
            CaptionUpdate::Clear => {}
        }
    }

    /// The `replay` message: whatever is on screen right now.
    pub fn replay_message(&self) -> String {
        let segments: Vec<_> = self.buffer.lock().unwrap().iter().cloned().collect();
        serde_json::json!({ "type": "replay", "segments": segments }).to_string()
    }

    /// What a newly connected overlay or output needs before live updates:
    /// styling, engine status, and the current lines. One definition, so the
    /// web overlay and every native output start from the same state.
    pub fn opening_messages(&self, settings: &OverlaySettings) -> Vec<String> {
        let status = self.status.lock().unwrap().clone();
        vec![
            settings_message(settings),
            CaptionUpdate::Status { status }.to_json(),
            self.replay_message(),
        ]
    }
}

/// Blank every overlay and output after `clear_after` seconds without new
/// speech, so a pause doesn't leave the last sentence hanging on air.
///
/// Owned here rather than by each renderer: it also empties the replay
/// buffer, so an overlay that reconnects (or a helper that respawns) after
/// the timeout doesn't bring the stale lines back.
pub async fn run_silence_clear(sinks: CaptionSinks) {
    let mut updates = sinks.broadcast.subscribe();
    let mut overlay = sinks.overlay.subscribe();
    let mut on_screen = false;
    let mut deadline: Option<tokio::time::Instant> = None;

    let arm = |overlay: &watch::Receiver<OverlaySettings>| {
        let secs = overlay.borrow().clear_after;
        (secs > 0.0).then(|| tokio::time::Instant::now() + std::time::Duration::from_secs_f32(secs))
    };

    loop {
        let expiry = async move {
            match deadline {
                Some(d) => tokio::time::sleep_until(d).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            update = updates.recv() => match update {
                Ok(CaptionUpdate::Segment { .. }) | Err(broadcast::error::RecvError::Lagged(_)) => {
                    on_screen = true;
                    deadline = arm(&overlay);
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Closed) => return,
            },
            // A new timeout applies from now to whatever is showing.
            Ok(()) = overlay.changed() => {
                if on_screen {
                    deadline = arm(&overlay);
                }
            }
            () = expiry => {
                on_screen = false;
                deadline = None;
                sinks.buffer.lock().unwrap().clear();
                let _ = sinks.broadcast.send(CaptionUpdate::Clear);
            }
        }
    }
}

/// Handle to a running caption pipeline.
pub struct CaptionEngine {
    shutdown_tx: watch::Sender<bool>,
    audio: audio::AudioCaptureHandle,
    task: tokio::task::JoinHandle<()>,
}

impl CaptionEngine {
    /// Stop capture and close the provider session.
    pub async fn stop(self) {
        let _ = self.shutdown_tx.send(true);
        // Stopping capture closes the audio channel, which also unblocks the
        // provider if it is mid-select.
        self.audio.stop();
        let _ = self.task.await;
        log::info!("Caption engine stopped");
    }
}

/// Start capture and streaming translation (or plain captions, when the
/// source and target are the same language).
pub fn start(
    app: AppHandle,
    config: &CaptionsConfig,
    sinks: CaptionSinks,
) -> Result<CaptionEngine, String> {
    let api_key = match config.provider.as_str() {
        "gemini" => config.api_keys.gemini.clone(),
        "openai" => config.api_keys.openai.clone(),
        // On-device providers need no key, and an unknown provider name is
        // rejected by `provider::build` below. Falling back to the Gemini key
        // here would both hand it to a provider with no business holding it and
        // mask a typo'd provider name behind a working-looking key.
        _ => String::new(),
    };

    let provider = provider::build(
        &config.provider,
        ProviderConfig {
            api_key,
            target_language: config.target_language.clone(),
            source_language: config.source_language.clone(),
        },
    )?;

    let (audio_tx, audio_rx) = mpsc::unbounded_channel::<Vec<i16>>();
    let (event_tx, event_rx) = mpsc::unbounded_channel::<ProviderEvent>();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Start capture first: a bad device should fail before we open a billable
    // provider session.
    let audio = audio::start(config.input_device.clone(), audio_tx)?;

    log::info!(
        "Captions starting: '{}' @ {} Hz -> {} ({} -> {})",
        audio.device_name,
        audio.device_sample_rate,
        provider.name(),
        config.source_language.as_deref().unwrap_or("auto"),
        config.target_language,
    );

    let status = StatusReporter::new(app, sinks, provider.name(), config.translates());
    status.set(CaptionEngineState::Starting);

    let provider_fut = provider.run(audio_rx, event_tx, shutdown_rx);
    let task = tokio::spawn(async move {
        tokio::join!(provider_fut, consume_events(status, event_rx));
    });

    Ok(CaptionEngine {
        shutdown_tx,
        audio,
        task,
    })
}

/// Publishes engine status, carrying the per-session fields every update
/// repeats.
struct StatusReporter {
    app: AppHandle,
    sinks: CaptionSinks,
    provider: String,
    translate_enabled: bool,
    started: std::time::Instant,
    reconnects: u32,
}

impl StatusReporter {
    fn new(app: AppHandle, sinks: CaptionSinks, provider: &str, translate_enabled: bool) -> Self {
        Self {
            app,
            sinks,
            provider: provider.to_string(),
            translate_enabled,
            started: std::time::Instant::now(),
            reconnects: 0,
        }
    }

    fn set(&self, state: CaptionEngineState) {
        let status = CaptionStatus {
            state,
            elapsed_seconds: self.started.elapsed().as_secs(),
            reconnects: self.reconnects,
            provider: self.provider.clone(),
            translate_enabled: self.translate_enabled,
        };
        *self.sinks.status.lock().unwrap() = status.clone();
        self.publish(CaptionUpdate::Status { status });
    }

    fn publish(&self, update: CaptionUpdate) {
        self.sinks.publish(&self.app, update);
    }
}

/// Accumulate provider deltas into lines and publish them.
async fn consume_events(
    mut status: StatusReporter,
    mut events: mpsc::UnboundedReceiver<ProviderEvent>,
) {
    let mut current = CaptionSegment::blank(1);

    while let Some(event) = events.recv().await {
        match event {
            ProviderEvent::Connected => status.set(CaptionEngineState::Running),

            ProviderEvent::Delta { source, translated } => {
                current.apply_delta(&source, &translated);
                publish_segment(&status, &mut current);
            }

            ProviderEvent::Replace { source, translated } => {
                current.apply_replace(source, translated);
                publish_segment(&status, &mut current);
            }

            ProviderEvent::TurnComplete => {
                // An empty turn (silence, or an audio-only frame) is not a line.
                if current.source.trim().is_empty() && current.translated.trim().is_empty() {
                    continue;
                }

                current.is_final = true;
                current.timestamp = now_ms();
                {
                    let mut buf = status.sinks.buffer.lock().unwrap();
                    buf.push_back(current.clone());
                    while buf.len() > BUFFER_CAPACITY {
                        buf.pop_front();
                    }
                }
                publish_segment(&status, &mut current);
                current = CaptionSegment::blank(current.id + 1);
            }

            ProviderEvent::Reconnecting { reason } => {
                status.reconnects += 1;
                log::info!("Caption provider reconnecting ({})", reason);
                status.set(CaptionEngineState::Reconnecting);
            }

            ProviderEvent::Fatal { message } => {
                log::error!("Caption provider failed: {}", message);
                status.set(CaptionEngineState::Error(message));
                return;
            }
        }
    }

    status.set(CaptionEngineState::Stopped);
}

/// Publish the line in progress. In captions-only mode anything a provider
/// put in `translated` is a same-language re-rendering, not the speaker's
/// words, so it's dropped here once rather than in every renderer.
fn publish_segment(status: &StatusReporter, seg: &mut CaptionSegment) {
    if !status.translate_enabled {
        seg.translated.clear();
    }
    status.publish(CaptionUpdate::Segment {
        segment: seg.clone(),
    });
}

impl CaptionSegment {
    fn blank(id: u64) -> Self {
        Self {
            id,
            source: String::new(),
            translated: String::new(),
            is_final: false,
            timestamp: now_ms(),
        }
    }

    /// Append incremental text to the line in progress.
    fn apply_delta(&mut self, source: &str, translated: &str) {
        self.source.push_str(source);
        self.translated.push_str(translated);
        self.timestamp = now_ms();
    }

    /// Overwrite the line in progress with a revised hypothesis.
    fn apply_replace(&mut self, source: String, translated: String) {
        self.source = source;
        self.translated = translated;
        self.timestamp = now_ms();
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Default for CaptionStatus {
    fn default() -> Self {
        Self {
            state: CaptionEngineState::Stopped,
            elapsed_seconds: 0,
            reconnects: 0,
            provider: "gemini".to_string(),
            translate_enabled: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overlay_settings_clamp_and_sanitize() {
        let cfg = CaptionsConfig {
            font_size: 9999,
            max_lines: 0,
            chroma_color: "red; } body { display:none".into(),
            safe_area: f32::NAN,
            width: 5.0,
            ..CaptionsConfig::default()
        };
        let s = OverlaySettings::from_config(&cfg);
        assert_eq!(s.font_size, 240);
        assert_eq!(s.max_lines, 1);
        assert_eq!(s.chroma_color, "#00B140");
        assert_eq!(s.safe_area, 5.0);
        assert_eq!(s.width, 20.0);
    }

    #[test]
    fn test_overlay_settings_serialize_camel_case() {
        let json = serde_json::to_value(OverlaySettings::default()).unwrap();
        for key in [
            "fontSize",
            "maxLines",
            "chromaColor",
            "safeArea",
            "width",
            "shadow",
            "background",
            "boxColor",
            "clearAfter",
        ] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
    }

    #[test]
    fn test_overrides_pin_clamp_and_ignore_garbage() {
        let q: HashMap<String, String> = [
            ("size", "999"),
            ("lines", "abc"),
            ("clear", "999"),
            ("bg", "transparent"),
            ("box", "1"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let base = OverlaySettings::default();
        let s = base.with_overrides(&q);
        assert_eq!(s.font_size, 240, "clamped like config values");
        assert_eq!(
            s.max_lines, base.max_lines,
            "unparseable value pins nothing"
        );
        assert_eq!(
            s.clear_after, base.clear_after,
            "silence timeout is app-wide, not pinnable"
        );
        assert_eq!(s.chroma_color, "transparent");
        assert!(s.background);
        assert_eq!(s.shadow, base.shadow);
    }

    fn final_segment(id: u64) -> CaptionSegment {
        CaptionSegment {
            source: "hi".into(),
            is_final: true,
            ..CaptionSegment::blank(id)
        }
    }

    fn segment(id: u64) -> CaptionUpdate {
        CaptionUpdate::Segment {
            segment: final_segment(id),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_silence_clear_blanks_and_empties_replay() {
        let sinks = CaptionSinks::default();
        let mut rx = sinks.broadcast.subscribe();
        tokio::spawn(run_silence_clear(sinks.clone()));
        tokio::task::yield_now().await;

        sinks.buffer.lock().unwrap().push_back(final_segment(1));
        sinks.broadcast.send(segment(1)).unwrap();
        assert!(matches!(
            rx.recv().await.unwrap(),
            CaptionUpdate::Segment { .. }
        ));

        // Nothing before the 8s default...
        tokio::time::sleep(std::time::Duration::from_secs(7)).await;
        assert!(rx.try_recv().is_err());
        // ...then a clear, and the replay buffer is empty.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        assert!(matches!(rx.recv().await.unwrap(), CaptionUpdate::Clear));
        assert!(sinks.buffer.lock().unwrap().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn test_silence_clear_disabled_at_zero() {
        let sinks = CaptionSinks::default();
        sinks.overlay.send_modify(|s| s.clear_after = 0.0);
        let mut rx = sinks.broadcast.subscribe();
        tokio::spawn(run_silence_clear(sinks.clone()));
        tokio::task::yield_now().await;

        sinks.broadcast.send(segment(1)).unwrap();
        rx.recv().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(600)).await;
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_sanitize_hex_color() {
        assert_eq!(sanitize_hex_color("0f0", "#000"), "#0f0");
        assert_eq!(sanitize_hex_color(" #00b140 ", "#000"), "#00b140");
        assert_eq!(sanitize_hex_color("url(x)", "#000"), "#000");
        assert_eq!(sanitize_hex_color("transparent", "#000"), "#000");
    }

    #[test]
    fn test_chroma_accepts_transparent_but_box_does_not() {
        let q: HashMap<String, String> = [("bg", "TRANSPARENT"), ("boxcolor", "transparent")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let s = OverlaySettings::default().with_overrides(&q);
        assert_eq!(s.chroma_color, "transparent");
        assert_eq!(s.box_color, "#000000");
    }

    #[test]
    fn test_segment_serializes_final_not_is_final() {
        let seg = CaptionSegment {
            id: 1,
            source: "hi".into(),
            translated: "salut".into(),
            is_final: true,
            timestamp: 0,
        };
        let json = serde_json::to_string(&seg).unwrap();
        // `final` is a Rust keyword but the wire name the frontend expects.
        assert!(json.contains("\"final\":true"));
        assert!(!json.contains("is_final"));
    }

    #[test]
    fn test_update_is_tagged_for_the_overlay_ws() {
        let update = CaptionUpdate::Status {
            status: CaptionStatus::default(),
        };
        let json = serde_json::to_string(&update).unwrap();
        assert!(json.contains("\"type\":\"status\""));
    }

    #[test]
    fn test_apply_delta_appends() {
        let mut seg = CaptionSegment::blank(1);
        seg.apply_delta("hello", "bonjour");
        seg.apply_delta(" world", " le monde");
        assert_eq!(seg.source, "hello world");
        assert_eq!(seg.translated, "bonjour le monde");
    }

    #[test]
    fn test_apply_replace_overwrites_a_revised_hypothesis() {
        let mut seg = CaptionSegment::blank(1);
        // On-device recognizers revise, they don't only extend. Appending these
        // would yield "hello wordhello world".
        seg.apply_replace("hello word".into(), "bonjour mot".into());
        seg.apply_replace("hello world".into(), "bonjour le monde".into());
        assert_eq!(seg.source, "hello world");
        assert_eq!(seg.translated, "bonjour le monde");
    }

    #[test]
    fn test_apply_replace_can_shorten_the_line() {
        let mut seg = CaptionSegment::blank(1);
        seg.apply_replace("a longer guess".into(), String::new());
        seg.apply_replace("short".into(), String::new());
        assert_eq!(seg.source, "short");
    }

    #[test]
    fn test_engine_state_serialization() {
        assert_eq!(
            serde_json::to_string(&CaptionEngineState::Reconnecting).unwrap(),
            "\"reconnecting\""
        );
        // Error carries its message as `{"error": "..."}`.
        let err = serde_json::to_string(&CaptionEngineState::Error("bad key".into())).unwrap();
        assert_eq!(err, "{\"error\":\"bad key\"}");
    }
}
