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
        "openai" => config.api_keys.openai.clone(),
        _ => config.api_keys.gemini.clone(),
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
        None,
    );

    let sinks_for_task = sinks.clone();
    let app_for_task = app.clone();

    let provider_shutdown = shutdown_rx.clone();
    let provider_fut = provider.run(audio_rx, event_tx, provider_shutdown);

    let task = tokio::spawn(async move {
        tokio::join!(
            provider_fut,
            consume_events(app_for_task, sinks_for_task, event_rx, provider_name),
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
                    Some((started.elapsed().as_secs(), reconnects)),
                );
            }

            ProviderEvent::Delta { source, translated } => {
                current.source.push_str(&source);
                current.translated.push_str(&translated);
                current.timestamp = now_ms();
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
        Some((started.elapsed().as_secs(), reconnects)),
    );
}

fn set_status(
    app: &AppHandle,
    sinks: &CaptionSinks,
    state: CaptionEngineState,
    provider: &str,
    counters: Option<(u64, u32)>,
) {
    let (elapsed_seconds, reconnects) = counters.unwrap_or((0, 0));
    let status = CaptionStatus {
        state,
        elapsed_seconds,
        reconnects,
        provider: provider.to_string(),
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
