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

pub mod gemini;

/// Events a provider reports back to the [`CaptionEngine`](super::CaptionEngine).
#[derive(Debug, Clone)]
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

type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

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
        other => Err(format!("Unknown caption provider '{}'", other)),
    }
}
