//! Caption provider abstraction.
//!
//! A provider owns one streaming session with a speech API: it consumes 16 kHz
//! mono PCM chunks and emits transcript deltas. Providers are responsible for
//! their own reconnection — for Gemini in particular, reconnecting is the
//! normal path rather than an error path (see `gemini.rs`).

use std::future::Future;
use std::pin::Pin;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::watch;

pub mod apple;
pub mod gemini;

/// Chunks of backlog to keep when a session is re-established.
///
/// Captions are a live feed: after an outage the operator needs the *current*
/// words, not a replay of what was said while we were offline. Anything older
/// than this is dropped.
pub(crate) const MAX_BACKLOG_CHUNKS: usize = 10; // 1 second

/// Events a provider reports back to the [`CaptionEngine`](super::CaptionEngine).
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderEvent {
    /// Streaming session established (first connect or after a resume).
    Connected,
    /// Incremental transcript text for the line currently being spoken.
    ///
    /// Both fields are deltas to append, not replacements. Providers that only
    /// produce one of the two (e.g. a transcription-only session) leave the
    /// other empty.
    Delta {
        /// Text in the speaker's original language.
        source: String,
        /// Text in the configured target language.
        translated: String,
    },
    /// Full replacement text for the line currently being spoken.
    ///
    /// Unlike [`Delta`](ProviderEvent::Delta), both fields are the *complete*
    /// current line rather than an append. On-device recognizers emit revisable
    /// hypotheses — "hello word" becomes "hello world" — so diffing them into
    /// appends corrupts the text. A provider must use `Delta` or `Replace`
    /// consistently for the life of a line, never both.
    Replace {
        /// Text in the speaker's original language.
        source: String,
        /// Text in the configured target language.
        translated: String,
    },
    /// The current line is complete and should be committed.
    TurnComplete,
    /// The session dropped and the provider is re-establishing it.
    Reconnecting { reason: String },
    /// Unrecoverable failure; the engine will stop.
    Fatal { message: String },
}

/// Runtime settings handed to a provider when it starts.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub api_key: String,
    /// BCP-47 target language, e.g. "fr".
    pub target_language: String,
    /// BCP-47 source language, or `None` to let the provider auto-detect.
    pub source_language: Option<String>,
}

pub(crate) type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

/// A streaming speech-to-caption backend.
///
/// Implemented as a boxed-future trait rather than `async fn` to keep the
/// trait object-safe without pulling in `async-trait`.
pub trait CaptionProvider: Send + 'static {
    /// Identifier matching `CaptionsConfig::provider`.
    fn name(&self) -> &'static str;

    /// Run until `shutdown` flips to `true` or a fatal error occurs.
    fn run(
        self: Box<Self>,
        audio_rx: UnboundedReceiver<Vec<i16>>,
        events: UnboundedSender<ProviderEvent>,
        shutdown: watch::Receiver<bool>,
    ) -> BoxFuture;
}

/// Build the provider named in config.
pub fn build(provider: &str, config: ProviderConfig) -> Result<Box<dyn CaptionProvider>, String> {
    match provider {
        "gemini" => {
            if config.api_key.trim().is_empty() {
                return Err("No Gemini API key configured. Add one in Settings.".to_string());
            }
            Ok(Box::new(gemini::GeminiProvider::new(config)))
        }
        "openai" => Err(
            "The OpenAI caption provider is not implemented yet. Select Gemini in Settings."
                .to_string(),
        ),
        #[cfg(target_os = "macos")]
        "apple" => {
            validate_apple_config(&config)?;
            Ok(Box::new(apple::AppleProvider::new(config)))
        }
        #[cfg(not(target_os = "macos"))]
        "apple" => Err(
            "The Apple on-device caption provider requires macOS 26 (Tahoe) or later. \
             Select Gemini in Settings."
                .to_string(),
        ),
        other => Err(format!("Unknown caption provider '{}'", other)),
    }
}

/// Reject an Apple config that cannot possibly start.
///
/// Kept `cfg`-free so it is testable off macOS, and checked here rather than in
/// the provider so the operator sees it as a start error in Settings instead of
/// a mid-session failure.
pub(crate) fn validate_apple_config(config: &ProviderConfig) -> Result<(), String> {
    let source = config.source_language.as_deref().unwrap_or("").trim();
    if source.is_empty() {
        // Gemini auto-detects; Apple's Speech and Translation frameworks both
        // need an explicit source locale up front.
        return Err("The Apple provider cannot auto-detect the spoken language. \
                    Set a Spoken Language (for example en-US) in Settings."
            .to_string());
    }
    if config.target_language.trim().is_empty() {
        return Err("Set a Target Language in Settings.".to_string());
    }
    Ok(())
}

/// Whether two BCP-47 tags name the same language, ignoring region.
///
/// "en-US" and "en" are the same language, so translating between them is a
/// no-op. Shared by `apple::build_args` (to skip building a
/// `TranslationSession`) and `CaptionEngine::start` (to tell the overlay
/// whether to expect translated text at all).
pub(crate) fn same_language(a: &str, b: &str) -> bool {
    fn primary(tag: &str) -> String {
        tag.trim()
            .split(['-', '_'])
            .next()
            .unwrap_or("")
            .to_lowercase()
    }
    let (a, b) = (primary(a), primary(b));
    !a.is_empty() && a == b
}

/// Take the newest [`MAX_BACKLOG_CHUNKS`] pending chunks, discarding older ones.
///
/// Returns `(kept, dropped_count)`. Keeping the tail preserves the last second
/// of speech across a session handoff; dropping the rest stops a long outage
/// from replaying minutes of stale audio into a live caption feed.
pub(crate) fn drain_backlog(audio_rx: &mut UnboundedReceiver<Vec<i16>>) -> (Vec<Vec<i16>>, usize) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::captions::audio::CHUNK_SAMPLES;

    fn apple_cfg(source: Option<&str>, target: &str) -> ProviderConfig {
        ProviderConfig {
            api_key: String::new(),
            target_language: target.to_string(),
            source_language: source.map(str::to_string),
        }
    }

    #[test]
    fn test_drain_backlog_keeps_newest_chunks() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        for i in 0..25 {
            tx.send(vec![i as i16; CHUNK_SAMPLES]).unwrap();
        }
        let (kept, dropped) = drain_backlog(&mut rx);

        assert_eq!(kept.len(), MAX_BACKLOG_CHUNKS);
        assert_eq!(dropped, 15);
        // The tail, not the head: the newest chunk must survive.
        assert_eq!(kept[0][0], 15);
        assert_eq!(kept[kept.len() - 1][0], 24);
    }

    #[test]
    fn test_drain_backlog_keeps_everything_when_short() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        for i in 0..3 {
            tx.send(vec![i as i16; CHUNK_SAMPLES]).unwrap();
        }
        let (kept, dropped) = drain_backlog(&mut rx);

        assert_eq!(kept.len(), 3);
        assert_eq!(dropped, 0);
    }

    #[test]
    fn test_validate_apple_config_requires_source_language() {
        for missing in [None, Some(""), Some("   ")] {
            let err = validate_apple_config(&apple_cfg(missing, "fr")).unwrap_err();
            assert!(err.contains("auto-detect"), "unexpected message: {}", err);
        }
    }

    #[test]
    fn test_validate_apple_config_requires_target_language() {
        let err = validate_apple_config(&apple_cfg(Some("en-US"), "  ")).unwrap_err();
        assert!(err.contains("Target Language"), "unexpected message: {}", err);
    }

    #[test]
    fn test_validate_apple_config_accepts_explicit_pair() {
        assert!(validate_apple_config(&apple_cfg(Some("en-US"), "fr")).is_ok());
    }

    #[test]
    fn test_same_language_ignores_region() {
        assert!(same_language("en-US", "en"));
        assert!(same_language("EN", "en-GB"));
        assert!(same_language("fr", "fr-CA"));
        assert!(!same_language("en", "fr"));
        assert!(!same_language("", ""));
    }
}
