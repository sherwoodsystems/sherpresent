//! Live caption capture, translation, and fan-out.
//!
//! Wires the audio capture thread to a streaming caption provider and
//! publishes the results three ways, mirroring how notes and status already
//! reach their consumers:
//!
//! - a `broadcast` channel consumed by the LAN overlay page
//! - a replay buffer so a browser that connects late sees current captions
//! - Tauri events for the in-app monitor

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, watch};

use crate::config::CaptionsConfig;

pub mod audio;
pub mod output;
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
    /// Opaque black box behind each row (closed-caption look)
    pub background: bool,
    /// Seconds of silence before the lines clear; 0 = never
    pub clear_after: f32,
}

impl OverlaySettings {
    pub fn from_config(c: &CaptionsConfig) -> Self {
        Self {
            font_size: c.font_size.clamp(12, 240),
            max_lines: c.max_lines.clamp(1, 6),
            chroma_color: sanitize_chroma(&c.chroma_color),
            safe_area: finite_clamp(c.safe_area, 0.0, 40.0, 5.0),
            width: finite_clamp(c.width, 20.0, 100.0, 80.0),
            shadow: c.shadow,
            background: c.background,
            clear_after: finite_clamp(c.clear_after, 0.0, 120.0, 8.0),
        }
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
/// Only `#rgb` / `#rrggbb` / `transparent` are accepted; anything else falls
/// back to broadcast green. This is a stylesheet injection guard, since the
/// value reaches the page from both config and an untrusted query string.
pub(crate) fn sanitize_chroma(raw: &str) -> String {
    let s = raw.trim();
    if s.eq_ignore_ascii_case("transparent") {
        return "transparent".to_string();
    }

    let hex = s.strip_prefix('#').unwrap_or(s);
    if matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        format!("#{}", hex)
    } else {
        "#00B140".to_string()
    }
}

/// A message published to the overlay page.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CaptionUpdate {
    Segment { segment: CaptionSegment },
    Status { status: CaptionStatus },
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

impl CaptionSinks {
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

/// Start capture and streaming translation.
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

    let provider_name = provider.name().to_string();

    // Auto-detect (`source_language: None`) always means "translate": the
    // provider doesn't know yet whether the detected language will match the
    // target, so the overlay must not assume captions-only mode.
    let translate_enabled = match config.source_language.as_deref() {
        Some(src) => !provider::same_language(src, &config.target_language),
        None => true,
    };

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
        provider_name,
        config.source_language.as_deref().unwrap_or("auto"),
        config.target_language,
    );

    set_status(
        &app,
        &sinks,
        CaptionEngineState::Starting,
        &provider_name,
        translate_enabled,
        None,
    );

    let sinks_for_task = sinks.clone();
    let app_for_task = app.clone();

    let provider_shutdown = shutdown_rx.clone();
    let provider_fut = provider.run(audio_rx, event_tx, provider_shutdown);

    let task = tokio::spawn(async move {
        tokio::join!(
            provider_fut,
            consume_events(app_for_task, sinks_for_task, event_rx, provider_name, translate_enabled),
        );
    });

    Ok(CaptionEngine {
        shutdown_tx,
        audio,
        task,
    })
}

/// Accumulate provider deltas into lines and publish them.
async fn consume_events(
    app: AppHandle,
    sinks: CaptionSinks,
    mut events: mpsc::UnboundedReceiver<ProviderEvent>,
    provider_name: String,
    translate_enabled: bool,
) {
    let mut next_id: u64 = 1;
    let mut current = CaptionSegment {
        id: next_id,
        source: String::new(),
        translated: String::new(),
        is_final: false,
        timestamp: now_ms(),
    };
    let mut reconnects: u32 = 0;
    let started = std::time::Instant::now();

    while let Some(event) = events.recv().await {
        match event {
            ProviderEvent::Connected => {
                set_status(
                    &app,
                    &sinks,
                    CaptionEngineState::Running,
                    &provider_name,
                    translate_enabled,
                    Some((started.elapsed().as_secs(), reconnects)),
                );
            }

            ProviderEvent::Delta { source, translated } => {
                apply_delta(&mut current, &source, &translated);
                sinks.publish(
                    &app,
                    CaptionUpdate::Segment {
                        segment: current.clone(),
                    },
                );
            }

            ProviderEvent::Replace { source, translated } => {
                apply_replace(&mut current, source, translated);
                sinks.publish(
                    &app,
                    CaptionUpdate::Segment {
                        segment: current.clone(),
                    },
                );
            }

            ProviderEvent::TurnComplete => {
                // An empty turn (silence, or an audio-only frame) is not a line.
                if current.source.trim().is_empty() && current.translated.trim().is_empty() {
                    continue;
                }

                current.is_final = true;
                current.timestamp = now_ms();

                {
                    let mut buf = sinks.buffer.lock().unwrap();
                    buf.push_back(current.clone());
                    while buf.len() > BUFFER_CAPACITY {
                        buf.pop_front();
                    }
                }

                sinks.publish(
                    &app,
                    CaptionUpdate::Segment {
                        segment: current.clone(),
                    },
                );

                next_id += 1;
                current = CaptionSegment {
                    id: next_id,
                    source: String::new(),
                    translated: String::new(),
                    is_final: false,
                    timestamp: now_ms(),
                };
            }

            ProviderEvent::Reconnecting { reason } => {
                reconnects += 1;
                log::info!("Caption provider reconnecting ({})", reason);
                set_status(
                    &app,
                    &sinks,
                    CaptionEngineState::Reconnecting,
                    &provider_name,
                    translate_enabled,
                    Some((started.elapsed().as_secs(), reconnects)),
                );
            }

            ProviderEvent::Fatal { message } => {
                log::error!("Caption provider failed: {}", message);
                set_status(
                    &app,
                    &sinks,
                    CaptionEngineState::Error(message),
                    &provider_name,
                    translate_enabled,
                    Some((started.elapsed().as_secs(), reconnects)),
                );
                return;
            }
        }
    }

    set_status(
        &app,
        &sinks,
        CaptionEngineState::Stopped,
        &provider_name,
        translate_enabled,
        Some((started.elapsed().as_secs(), reconnects)),
    );
}

/// Append incremental text to the line in progress.
fn apply_delta(seg: &mut CaptionSegment, source: &str, translated: &str) {
    seg.source.push_str(source);
    seg.translated.push_str(translated);
    seg.timestamp = now_ms();
}

/// Overwrite the line in progress with a revised hypothesis.
fn apply_replace(seg: &mut CaptionSegment, source: String, translated: String) {
    seg.source = source;
    seg.translated = translated;
    seg.timestamp = now_ms();
}

fn set_status(
    app: &AppHandle,
    sinks: &CaptionSinks,
    state: CaptionEngineState,
    provider: &str,
    translate_enabled: bool,
    counters: Option<(u64, u32)>,
) {
    let (elapsed_seconds, reconnects) = counters.unwrap_or((0, 0));
    let status = CaptionStatus {
        state,
        elapsed_seconds,
        reconnects,
        provider: provider.to_string(),
        translate_enabled,
    };

    {
        let mut current = sinks.status.lock().unwrap();
        *current = status.clone();
    }

    sinks.publish(app, CaptionUpdate::Status { status });
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
            "clearAfter",
        ] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
    }

    #[test]
    fn test_sanitize_chroma() {
        assert_eq!(sanitize_chroma("TRANSPARENT"), "transparent");
        assert_eq!(sanitize_chroma("0f0"), "#0f0");
        assert_eq!(sanitize_chroma("#00b140"), "#00b140");
        assert_eq!(sanitize_chroma("url(x)"), "#00B140");
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

    fn blank_segment() -> CaptionSegment {
        CaptionSegment {
            id: 1,
            source: String::new(),
            translated: String::new(),
            is_final: false,
            timestamp: 0,
        }
    }

    #[test]
    fn test_apply_delta_appends() {
        let mut seg = blank_segment();
        apply_delta(&mut seg, "hello", "bonjour");
        apply_delta(&mut seg, " world", " le monde");
        assert_eq!(seg.source, "hello world");
        assert_eq!(seg.translated, "bonjour le monde");
    }

    #[test]
    fn test_apply_replace_overwrites_a_revised_hypothesis() {
        let mut seg = blank_segment();
        // On-device recognizers revise, they don't only extend. Appending these
        // would yield "hello wordhello world".
        apply_replace(&mut seg, "hello word".into(), "bonjour mot".into());
        apply_replace(&mut seg, "hello world".into(), "bonjour le monde".into());
        assert_eq!(seg.source, "hello world");
        assert_eq!(seg.translated, "bonjour le monde");
    }

    #[test]
    fn test_apply_replace_can_shorten_the_line() {
        let mut seg = blank_segment();
        apply_replace(&mut seg, "a longer guess".into(), String::new());
        apply_replace(&mut seg, "short".into(), String::new());
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
