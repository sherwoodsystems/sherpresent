//! Caption video outputs besides the web overlay.
//!
//! Each output is a helper process fed the *same* NDJSON messages the web
//! overlay's `/api/captions/ws` socket carries — segments, replay, status and
//! [`OverlaySettings`](super::OverlaySettings) — so every output renders the
//! same lines with the same styling, and adding one (NDI, say) means a new
//! renderer/sink on the helper side plus a sibling of [`syphon`] here, with no
//! change to the caption engine.
//!
//! Outputs run independently of the caption engine: a receiver stays
//! connected, showing a transparent frame, while captions are stopped.

pub mod syphon;

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::watch;

use super::CaptionSinks;
use crate::config::{CaptionOutputsConfig, SyphonOutputConfig};

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum OutputState {
    Stopped,
    Starting,
    Running,
    Error,
}

/// One output's status, for Settings.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutputStatus {
    /// Whether this machine can run the output at all
    pub supported: bool,
    pub state: OutputState,
    /// Name receivers see
    pub name: String,
    /// Whether any receiver is currently connected
    pub has_clients: bool,
    pub message: Option<String>,
}

impl OutputStatus {
    fn stopped(supported: bool, name: &str) -> Self {
        Self {
            supported,
            state: OutputState::Stopped,
            name: name.to_string(),
            has_clients: false,
            message: None,
        }
    }
}

/// Every output's status. One field per output kind.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OutputsStatus {
    pub syphon: OutputStatus,
}

impl Default for OutputsStatus {
    fn default() -> Self {
        Self {
            syphon: OutputStatus::stopped(
                syphon::is_supported(),
                &SyphonOutputConfig::default().server_name,
            ),
        }
    }
}

/// How an output task reports status changes.
pub type StatusReporter = Arc<dyn Fn(OutputStatus) + Send + Sync>;

struct Running {
    config: SyphonOutputConfig,
    shutdown: watch::Sender<bool>,
}

/// The running outputs, reconciled against config on startup and every save.
#[derive(Default)]
pub struct CaptionOutputs {
    syphon: Option<Running>,
    status: Arc<Mutex<OutputsStatus>>,
}

impl CaptionOutputs {
    pub fn status(&self) -> OutputsStatus {
        self.status.lock().unwrap().clone()
    }

    /// Start, stop or restart outputs so they match `cfg`. Idempotent, so it's
    /// safe to call on every config save: unchanged outputs are left alone.
    pub fn reconcile(&mut self, app: &AppHandle, sinks: &CaptionSinks, cfg: &CaptionOutputsConfig) {
        let want = cfg.syphon.enabled.then(|| cfg.syphon.clone());
        let have = self.syphon.as_ref().map(|r| &r.config);
        if want.as_ref() == have {
            return;
        }

        // Changed or disabled: stop the old one first. A rename restarts,
        // since a Syphon server's name is fixed at creation.
        if let Some(old) = self.syphon.take() {
            let _ = old.shutdown.send(true);
        }

        let status = Arc::clone(&self.status);
        let app_for_report = app.clone();
        let report: StatusReporter = Arc::new(move |s: OutputStatus| {
            let snapshot = {
                let mut all = status.lock().unwrap();
                all.syphon = s;
                all.clone()
            };
            let _ = app_for_report.emit("caption-outputs-status", &snapshot);
        });

        let Some(config) = want else {
            report(OutputStatus::stopped(syphon::is_supported(), &cfg.syphon.server_name));
            return;
        };

        let path = match syphon::preflight() {
            Ok(p) => p,
            Err(message) => {
                report(OutputStatus {
                    state: OutputState::Error,
                    message: Some(message),
                    ..OutputStatus::stopped(syphon::is_supported(), &config.server_name)
                });
                return;
            }
        };

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let name = config.server_name.clone();
        let sinks = sinks.clone();
        tauri::async_runtime::spawn(async move {
            syphon::run(path, name, sinks, shutdown_rx, report).await;
        });
        self.syphon = Some(Running {
            config,
            shutdown: shutdown_tx,
        });
    }

    /// Stop everything, e.g. on app exit.
    pub fn stop_all(&mut self) {
        if let Some(r) = self.syphon.take() {
            let _ = r.shutdown.send(true);
        }
    }
}
