//! Gemini Live API caption provider.
//!
//! Streams 16 kHz mono PCM to the `BidiGenerateContent` WebSocket and reads
//! back two transcripts per turn: `inputTranscription` (what was said) and
//! `outputTranscription` (the translation). The model also generates translated
//! *speech*, which we deliberately discard — we only want the text.
//!
//! ## Reconnection is the normal path
//!
//! A single Live API connection lives roughly 10 minutes regardless of session
//! length, and the server announces the cut with a `goAway` about 60 s ahead.
//! `contextWindowCompression` lifts the 15-minute *session* cap, and
//! `sessionResumption` lets a new socket pick up where the old one left off, so
//! a 90-minute keynote will reconnect ~9 times as a matter of course. Treating
//! a disconnect as an error would make captions fail every ten minutes.
//!
//! ## Schema handling
//!
//! Server frames are parsed as `serde_json::Value` rather than typed structs.
//! The translate model is a preview with no SLA and its response shape has
//! moved before; tolerating unknown and missing fields keeps captions flowing
//! rather than hard-failing mid-show on a field rename.

use std::time::Duration;

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;

use super::{CaptionProvider, ProviderConfig, ProviderEvent};
use crate::captions::audio::{samples_to_le_bytes, TARGET_SAMPLE_RATE};

const MODEL: &str = "models/gemini-3.5-live-translate-preview";
const ENDPOINT: &str = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";

/// Chunks of backlog to keep when a session is re-established.
///
/// Captions are a live feed: after an outage the operator needs the *current*
/// words, not a replay of what was said while we were offline. Anything older
/// than this is dropped.
const MAX_BACKLOG_CHUNKS: usize = 10; // 1 second

/// Reconnect backoff bounds.
const BACKOFF_START: Duration = Duration::from_millis(250);
const BACKOFF_MAX: Duration = Duration::from_secs(10);

pub struct GeminiProvider {
    config: ProviderConfig,
}

impl GeminiProvider {
    pub fn new(config: ProviderConfig) -> Self {
        Self { config }
    }
}

impl CaptionProvider for GeminiProvider {
    fn name(&self) -> &'static str {
        "gemini"
    }

    fn run(
        self: Box<Self>,
        audio_rx: UnboundedReceiver<Vec<i16>>,
        events: UnboundedSender<ProviderEvent>,
        shutdown: watch::Receiver<bool>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        Box::pin(run_loop(self.config, audio_rx, events, shutdown))
    }
}

/// Outer loop: connect, run a session, reconnect on `goAway` or drop.
async fn run_loop(
    config: ProviderConfig,
    mut audio_rx: UnboundedReceiver<Vec<i16>>,
    events: UnboundedSender<ProviderEvent>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut resume_handle: Option<String> = None;
    let mut backoff = BACKOFF_START;

    loop {
        if *shutdown.borrow() {
            return;
        }

        match run_session(
            &config,
            &mut audio_rx,
            &events,
            &mut shutdown,
            resume_handle.clone(),
        )
        .await
        {
            SessionOutcome::Shutdown => return,
            SessionOutcome::Reconnect { handle, reason } => {
                // A clean handoff: reconnect immediately, no backoff.
                resume_handle = handle.or(resume_handle);
                backoff = BACKOFF_START;
                let _ = events.send(ProviderEvent::Reconnecting { reason });
            }
            SessionOutcome::Failed { handle, reason } => {
                resume_handle = handle.or(resume_handle);
                let _ = events.send(ProviderEvent::Reconnecting {
                    reason: reason.clone(),
                });
                log::warn!("Gemini session failed ({}), retrying in {:?}", reason, backoff);

                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = shutdown.changed() => return,
                }
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
            SessionOutcome::Fatal { message } => {
                let _ = events.send(ProviderEvent::Fatal { message });
                return;
            }
        }
    }
}

enum SessionOutcome {
    /// Shutdown was requested.
    Shutdown,
    /// Expected end of connection; reconnect straight away.
    Reconnect {
        handle: Option<String>,
        reason: String,
    },
    /// Unexpected end; reconnect with backoff.
    Failed {
        handle: Option<String>,
        reason: String,
    },
    /// Do not retry (bad key, bad model).
    Fatal { message: String },
}

async fn run_session(
    config: &ProviderConfig,
    audio_rx: &mut UnboundedReceiver<Vec<i16>>,
    events: &UnboundedSender<ProviderEvent>,
    shutdown: &mut watch::Receiver<bool>,
    resume_handle: Option<String>,
) -> SessionOutcome {
    let url = format!("{}?key={}", ENDPOINT, config.api_key);

    let connect = tokio_tungstenite::connect_async(&url);
    let (ws, _resp) = tokio::select! {
        r = connect => match r {
            Ok(v) => v,
            Err(e) => return classify_connect_error(e),
        },
        _ = shutdown.changed() => return SessionOutcome::Shutdown,
    };

    let (mut sink, mut stream) = ws.split();

    // Setup frame. Field placement follows the Live Translate documentation
    // for this specific model.
    let setup = build_setup(config, resume_handle.as_deref());
    if let Err(e) = sink.send(Message::text(setup.to_string())).await {
        return SessionOutcome::Failed {
            handle: resume_handle,
            reason: format!("failed to send setup: {}", e),
        };
    }

    log::info!(
        "Gemini Live session opened ({} -> {}{})",
        config.source_language.as_deref().unwrap_or("auto"),
        config.target_language,
        if resume_handle.is_some() {
            ", resumed"
        } else {
            ""
        }
    );

    // Carry the last second of speech across the handoff, drop older backlog.
    let (carryover, dropped) = drain_backlog(audio_rx);
    if dropped > 0 {
        log::debug!("Dropped {} stale audio chunks on reconnect", dropped);
    }
    for chunk in &carryover {
        if let Err(e) = sink.send(Message::text(audio_frame(chunk).to_string())).await {
            return SessionOutcome::Failed {
                handle: resume_handle,
                reason: format!("failed to send carryover audio: {}", e),
            };
        }
    }

    let mut handle = resume_handle;
    let mut setup_complete = false;

    loop {
        tokio::select! {
            biased;

            _ = shutdown.changed() => {
                let _ = sink.close().await;
                return SessionOutcome::Shutdown;
            }

            incoming = stream.next() => {
                let msg = match incoming {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => return SessionOutcome::Failed {
                        handle,
                        reason: format!("websocket error: {}", e),
                    },
                    None => return SessionOutcome::Failed {
                        handle,
                        reason: "connection closed by server".to_string(),
                    },
                };

                let payload = match &msg {
                    Message::Text(t) => t.as_str().to_string(),
                    Message::Binary(b) => match std::str::from_utf8(b) {
                        Ok(s) => s.to_string(),
                        Err(_) => continue,
                    },
                    Message::Close(frame) => {
                        let reason = frame
                            .as_ref()
                            .map(|f| format!("server closed: {} {}", f.code, f.reason))
                            .unwrap_or_else(|| "server closed".to_string());
                        // An auth/model rejection closes immediately, before
                        // setupComplete. Retrying that just loops forever.
                        return if setup_complete {
                            SessionOutcome::Reconnect { handle, reason }
                        } else {
                            SessionOutcome::Fatal {
                                message: format!(
                                    "Gemini rejected the session ({}). Check the API key and that \
                                     the Live Translate preview is enabled for your project.",
                                    reason
                                ),
                            }
                        };
                    }
                    _ => continue,
                };

                let value: Value = match serde_json::from_str(&payload) {
                    Ok(v) => v,
                    Err(e) => {
                        log::debug!("Ignoring unparseable Gemini frame: {}", e);
                        continue;
                    }
                };

                if let Some(err) = value.get("error") {
                    return SessionOutcome::Fatal {
                        message: format!("Gemini API error: {}", err),
                    };
                }

                if value.get("setupComplete").is_some() {
                    setup_complete = true;
                    let _ = events.send(ProviderEvent::Connected);
                    continue;
                }

                if let Some(update) = value.get("sessionResumptionUpdate") {
                    // Only store handles the server marks resumable.
                    let resumable = update
                        .get("resumable")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    if let Some(new_handle) =
                        update.get("newHandle").and_then(Value::as_str)
                    {
                        if resumable {
                            handle = Some(new_handle.to_string());
                        }
                    }
                    continue;
                }

                if let Some(go_away) = value.get("goAway") {
                    let time_left = go_away
                        .get("timeLeft")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    log::info!("Gemini goAway received (timeLeft {}), resuming session", time_left);
                    let _ = sink.close().await;
                    return SessionOutcome::Reconnect {
                        handle,
                        reason: "scheduled session handoff".to_string(),
                    };
                }

                if let Some(server_content) = value.get("serverContent") {
                    handle_server_content(server_content, events);
                }
            }

            chunk = audio_rx.recv() => {
                let Some(chunk) = chunk else {
                    // Capture stopped.
                    let _ = sink.close().await;
                    return SessionOutcome::Shutdown;
                };

                let frame = audio_frame(&chunk);
                if let Err(e) = sink.send(Message::text(frame.to_string())).await {
                    return SessionOutcome::Failed {
                        handle,
                        reason: format!("failed to send audio: {}", e),
                    };
                }
            }
        }
    }
}

/// Extract transcript deltas and turn boundaries, discarding generated audio.
fn handle_server_content(content: &Value, events: &UnboundedSender<ProviderEvent>) {
    let source = content
        .get("inputTranscription")
        .and_then(|t| t.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("");

    let translated = content
        .get("outputTranscription")
        .and_then(|t| t.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("");

    if !source.is_empty() || !translated.is_empty() {
        let _ = events.send(ProviderEvent::Delta {
            source: source.to_string(),
            translated: translated.to_string(),
        });
    }

    // `modelTurn.parts[].inlineData` carries 24 kHz translated speech. We want
    // captions, so it is intentionally ignored.

    let turn_complete = content
        .get("turnComplete")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let generation_complete = content
        .get("generationComplete")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if turn_complete || generation_complete {
        let _ = events.send(ProviderEvent::TurnComplete);
    }
}

/// Build the `setup` frame for a new or resumed session.
fn build_setup(config: &ProviderConfig, resume_handle: Option<&str>) -> Value {
    let mut translation_config = json!({
        "targetLanguageCode": config.target_language,
        // We discard the generated audio, so there is nothing to echo.
        "echoTargetLanguage": false,
    });
    if let Some(source) = &config.source_language {
        translation_config["sourceLanguageCode"] = json!(source);
    }

    let session_resumption = match resume_handle {
        Some(h) => json!({ "handle": h }),
        None => json!({}),
    };

    json!({
        "setup": {
            "model": MODEL,
            "generationConfig": {
                "responseModalities": ["AUDIO"],
                "inputAudioTranscription": {},
                "outputAudioTranscription": {},
                "translationConfig": translation_config,
            },
            // Lifts the 15-minute audio-only session cap.
            "contextWindowCompression": { "slidingWindow": {} },
            // Lets a fresh socket continue this session after goAway.
            "sessionResumption": session_resumption,
        }
    })
}

/// Take the newest [`MAX_BACKLOG_CHUNKS`] pending chunks, discarding older ones.
///
/// Returns `(kept, dropped_count)`. Keeping the tail preserves the last second
/// of speech across a session handoff; dropping the rest stops a long outage
/// from replaying minutes of stale audio into a live caption feed.
fn drain_backlog(audio_rx: &mut UnboundedReceiver<Vec<i16>>) -> (Vec<Vec<i16>>, usize) {
    let mut pending = Vec::new();
    while let Ok(chunk) = audio_rx.try_recv() {
        pending.push(chunk);
    }
    if pending.len() <= MAX_BACKLOG_CHUNKS {
        return (pending, 0);
    }
    let dropped = pending.len() - MAX_BACKLOG_CHUNKS;
    (pending.split_off(dropped), dropped)
}

/// Encode one PCM chunk as a Live API `realtimeInput` frame.
fn audio_frame(chunk: &[i16]) -> Value {
    let b64 = base64::engine::general_purpose::STANDARD.encode(samples_to_le_bytes(chunk));
    json!({
        "realtimeInput": {
            "audio": {
                "data": b64,
                "mimeType": format!("audio/pcm;rate={}", TARGET_SAMPLE_RATE),
            }
        }
    })
}

/// Decide whether a connect failure is worth retrying.
fn classify_connect_error(e: tokio_tungstenite::tungstenite::Error) -> SessionOutcome {
    use tokio_tungstenite::tungstenite::Error as WsError;

    if let WsError::Http(resp) = &e {
        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return SessionOutcome::Fatal {
                message: format!(
                    "Gemini rejected the API key (HTTP {}). Check the key in Settings.",
                    status
                ),
            };
        }
        if status.as_u16() == 404 {
            return SessionOutcome::Fatal {
                message: format!(
                    "Gemini model '{}' is unavailable (HTTP 404). The Live Translate preview may \
                     not be enabled for this key.",
                    MODEL
                ),
            };
        }
    }

    SessionOutcome::Failed {
        handle: None,
        reason: format!("connect failed: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::captions::audio::CHUNK_SAMPLES;

    fn cfg() -> ProviderConfig {
        ProviderConfig {
            api_key: "k".to_string(),
            target_language: "fr".to_string(),
            source_language: None,
        }
    }

    #[test]
    fn test_setup_frame_shape() {
        let setup = build_setup(&cfg(), None);
        let gen = &setup["setup"]["generationConfig"];
        assert_eq!(setup["setup"]["model"], MODEL);
        assert_eq!(gen["responseModalities"][0], "AUDIO");
        assert!(gen["inputAudioTranscription"].is_object());
        assert!(gen["outputAudioTranscription"].is_object());
        assert_eq!(gen["translationConfig"]["targetLanguageCode"], "fr");
        assert_eq!(gen["translationConfig"]["echoTargetLanguage"], false);
        // Required for sessions longer than 15 minutes.
        assert!(setup["setup"]["contextWindowCompression"]["slidingWindow"].is_object());
    }

    #[test]
    fn test_setup_omits_source_language_when_auto() {
        let setup = build_setup(&cfg(), None);
        assert!(setup["setup"]["generationConfig"]["translationConfig"]
            ["sourceLanguageCode"]
            .is_null());
    }

    #[test]
    fn test_setup_includes_source_language_when_pinned() {
        let mut c = cfg();
        c.source_language = Some("en".to_string());
        let setup = build_setup(&c, None);
        assert_eq!(
            setup["setup"]["generationConfig"]["translationConfig"]["sourceLanguageCode"],
            "en"
        );
    }

    #[test]
    fn test_setup_carries_resume_handle() {
        let setup = build_setup(&cfg(), Some("abc123"));
        assert_eq!(setup["setup"]["sessionResumption"]["handle"], "abc123");

        let fresh = build_setup(&cfg(), None);
        assert!(fresh["setup"]["sessionResumption"]["handle"].is_null());
    }

    #[test]
    fn test_server_content_emits_deltas() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let content = json!({
            "inputTranscription": { "text": "hello " },
            "outputTranscription": { "text": "bonjour " },
        });
        handle_server_content(&content, &tx);

        match rx.try_recv().unwrap() {
            ProviderEvent::Delta { source, translated } => {
                assert_eq!(source, "hello ");
                assert_eq!(translated, "bonjour ");
            }
            other => panic!("expected Delta, got {:?}", other),
        }
    }

    #[test]
    fn test_server_content_ignores_generated_audio() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        // A pure audio frame carries no transcript and must produce no event.
        let content = json!({
            "modelTurn": { "parts": [{ "inlineData": {
                "mimeType": "audio/pcm;rate=24000", "data": "AAAA"
            }}]}
        });
        handle_server_content(&content, &tx);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_turn_complete_emitted() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        handle_server_content(&json!({ "turnComplete": true }), &tx);
        assert!(matches!(
            rx.try_recv().unwrap(),
            ProviderEvent::TurnComplete
        ));
    }

    #[test]
    fn test_audio_frame_is_16khz_pcm() {
        // Guards the exact mime type and encoding the API requires.
        let frame = audio_frame(&[0x0102i16]);
        assert_eq!(
            frame["realtimeInput"]["audio"]["mimeType"],
            "audio/pcm;rate=16000"
        );
        // 0x0102 little-endian is [0x02, 0x01] -> "AgE="
        assert_eq!(frame["realtimeInput"]["audio"]["data"], "AgE=");
    }

    #[test]
    fn test_audio_frame_chunk_size() {
        let frame = audio_frame(&vec![0i16; CHUNK_SAMPLES]);
        let data = frame["realtimeInput"]["audio"]["data"].as_str().unwrap();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(data)
            .unwrap();
        // 100 ms of 16 kHz mono 16-bit audio.
        assert_eq!(decoded.len(), 3200);
    }

    #[test]
    fn test_drain_backlog_keeps_newest_chunks() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<i16>>();
        for i in 0..25i16 {
            tx.send(vec![i]).unwrap();
        }
        let (kept, dropped) = drain_backlog(&mut rx);
        assert_eq!(dropped, 15);
        assert_eq!(kept.len(), MAX_BACKLOG_CHUNKS);
        // The tail is retained, not the head.
        assert_eq!(kept.first().unwrap()[0], 15);
        assert_eq!(kept.last().unwrap()[0], 24);
    }

    #[test]
    fn test_drain_backlog_keeps_everything_when_short() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<i16>>();
        tx.send(vec![7i16]).unwrap();
        let (kept, dropped) = drain_backlog(&mut rx);
        assert_eq!(dropped, 0);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0][0], 7);
    }

    #[test]
    fn test_build_rejects_empty_key() {
        let err = super::super::build(
            "gemini",
            ProviderConfig {
                api_key: "   ".to_string(),
                target_language: "fr".to_string(),
                source_language: None,
            },
        )
        // `Box<dyn CaptionProvider>` isn't Debug, so unwrap_err() won't compile.
        .err()
        .expect("empty key must be rejected before a session opens");
        assert!(err.contains("API key"));
    }
}
