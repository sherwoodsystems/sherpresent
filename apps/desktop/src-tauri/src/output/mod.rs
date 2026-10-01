//! Native video outputs: helper processes publishing frames to receivers on
//! this Mac (Syphon). Each output is its own helper and Syphon server:
//!
//! - **captions** (two of them, each showing the translation, the original or
//!   both): fed the same NDJSON the web overlay's `/api/captions/ws`
//!   socket carries — segments, replay, status and
//!   [`OverlaySettings`](crate::captions::OverlaySettings) — so it renders the
//!   same lines with the same styling.
//! - **notes**: the current slide's notes and the Ontime timer, fed the stage
//!   view's `/api/ws` messages plus [`crate::ontime`].
//! - **slideshow**: PowerPoint's slide show window, captured by the helper
//!   itself (ScreenCaptureKit) while a show runs. Fed nothing.
//!
//! What each is fed lives in [`feed`]; adding a sink (NDI, say) means a new
//! sink on the helper side plus a sibling of [`syphon`] here.
//!
//! Outputs run independently of the caption engine and the presentation: a
//! receiver stays connected between talks.

pub mod feed;
pub mod syphon;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::watch;

use crate::captions::CaptionSinks;
use crate::config::{
    AppConfig, CaptionOutputsConfig, CaptionSyphonConfig, SyphonOutputConfig, WebServerConfig,
};
use crate::ontime::{self, TimerState};
use feed::NotesSources;

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

/// Every output's status. One field per output.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OutputsStatus {
    pub captions: OutputStatus,
    pub captions2: OutputStatus,
    pub notes: OutputStatus,
    pub slideshow: OutputStatus,
}

impl Default for OutputsStatus {
    fn default() -> Self {
        let supported = syphon::is_supported();
        let outputs = CaptionOutputsConfig::default();
        Self {
            captions: OutputStatus::stopped(supported, &outputs.syphon.syphon.server_name),
            captions2: OutputStatus::stopped(supported, &outputs.syphon2.syphon.server_name),
            notes: OutputStatus::stopped(supported, &WebServerConfig::default().syphon.server_name),
            slideshow: OutputStatus::stopped(
                supported,
                &WebServerConfig::default().slideshow_syphon.server_name,
            ),
        }
    }
}

/// How an output task reports status changes.
pub type StatusReporter = Arc<dyn Fn(OutputStatus) + Send + Sync>;

/// Everything the outputs are fed from.
#[derive(Clone)]
pub struct OutputSources {
    pub captions: CaptionSinks,
    pub notes: NotesSources,
}

/// A running output, and the settings it was started with.
struct Running<K> {
    key: K,
    shutdown: watch::Sender<bool>,
    /// Cleared when replaced, so the old task's final "stopped" report can't
    /// land after its replacement's "running".
    live: Arc<AtomicBool>,
}

/// What the notes output restarts on: its Syphon settings and where Ontime is.
#[derive(Clone, PartialEq)]
struct NotesKey {
    syphon: SyphonOutputConfig,
    ontime: Option<(String, u16)>,
}

/// The running outputs, reconciled against config on startup and every save.
#[derive(Default)]
pub struct Outputs {
    captions: Option<Running<CaptionSyphonConfig>>,
    captions2: Option<Running<CaptionSyphonConfig>>,
    notes: Option<Running<NotesKey>>,
    slideshow: Option<Running<SyphonOutputConfig>>,
    status: Arc<Mutex<OutputsStatus>>,
}

impl Outputs {
    pub fn status(&self) -> OutputsStatus {
        self.status.lock().unwrap().clone()
    }

    /// Start, stop or restart outputs so they match `cfg`. Idempotent, so it's
    /// safe to call on every config save: unchanged outputs are left alone.
    pub fn reconcile(&mut self, app: &AppHandle, sources: &OutputSources, cfg: &AppConfig) {
        let outputs = &cfg.captions.outputs;
        let report = self.reporter(app, |all| &mut all.captions);
        reconcile_captions(
            &mut self.captions,
            &outputs.syphon,
            &sources.captions,
            report,
        );
        let report = self.reporter(app, |all| &mut all.captions2);
        reconcile_captions(
            &mut self.captions2,
            &outputs.syphon2,
            &sources.captions,
            report,
        );

        let ws = &cfg.web_server;
        let host = ws.ontime_host.trim();
        let notes = sources.notes.clone();
        let report = self.reporter(app, |all| &mut all.notes);
        reconcile_slot(
            &mut self.notes,
            ws.syphon.enabled.then(|| NotesKey {
                syphon: ws.syphon.clone(),
                ontime: (!host.is_empty()).then(|| (host.to_string(), ws.ontime_port)),
            }),
            &ws.syphon.server_name,
            report,
            |key, path, shutdown, report| {
                // Ontime is followed only while the notes output runs.
                let timer = key.ontime.clone().map(|(host, port)| {
                    let (tx, rx) = watch::channel(TimerState::default());
                    tauri::async_runtime::spawn(ontime::run(host, port, tx, shutdown.clone()));
                    rx
                });
                let feed = feed::notes(notes, timer);
                let name = key.syphon.server_name.clone();
                tauri::async_runtime::spawn(syphon::run(
                    path, "notes", None, name, feed, shutdown, report,
                ));
            },
        );

        let slideshow = &ws.slideshow_syphon;
        let report = self.reporter(app, |all| &mut all.slideshow);
        reconcile_slot(
            &mut self.slideshow,
            slideshow.enabled.then(|| slideshow.clone()),
            &slideshow.server_name,
            report,
            |key, path, shutdown, report| {
                let name = key.server_name.clone();
                tauri::async_runtime::spawn(syphon::run(
                    path,
                    "slideshow",
                    None,
                    name,
                    feed::none(),
                    shutdown,
                    report,
                ));
            },
        );
    }

    /// Reports into one field of the shared status, and pushes the whole
    /// snapshot to the frontend.
    fn reporter(
        &self,
        app: &AppHandle,
        field: fn(&mut OutputsStatus) -> &mut OutputStatus,
    ) -> StatusReporter {
        let status = Arc::clone(&self.status);
        let app = app.clone();
        Arc::new(move |s: OutputStatus| {
            let snapshot = {
                let mut all = status.lock().unwrap();
                *field(&mut all) = s;
                all.clone()
            };
            let _ = app.emit("outputs-status", &snapshot);
        })
    }
}

/// Bring one captions output in line with its config.
fn reconcile_captions(
    slot: &mut Option<Running<CaptionSyphonConfig>>,
    want: &CaptionSyphonConfig,
    captions: &CaptionSinks,
    report: StatusReporter,
) {
    let captions = captions.clone();
    reconcile_slot(
        slot,
        want.syphon.enabled.then(|| want.clone()),
        &want.syphon.server_name,
        report,
        |key, path, shutdown, report| {
            let feed = feed::captions(captions);
            let name = key.syphon.server_name.clone();
            tauri::async_runtime::spawn(syphon::run(
                path,
                "captions",
                Some(key.text),
                name,
                feed,
                shutdown,
                report,
            ));
        },
    );
}

/// Bring one output in line with `want` (`None` = disabled). A changed key
/// restarts it: a Syphon server's name is fixed at creation.
fn reconcile_slot<K: PartialEq>(
    slot: &mut Option<Running<K>>,
    want: Option<K>,
    name: &str,
    report: StatusReporter,
    start: impl FnOnce(&K, PathBuf, watch::Receiver<bool>, StatusReporter),
) {
    if want.as_ref() == slot.as_ref().map(|r| &r.key) {
        return;
    }
    if let Some(old) = slot.take() {
        old.live.store(false, Ordering::Relaxed);
        let _ = old.shutdown.send(true);
    }

    let Some(key) = want else {
        report(OutputStatus::stopped(syphon::is_supported(), name));
        return;
    };
    let path = match syphon::preflight() {
        Ok(p) => p,
        Err(message) => {
            report(OutputStatus {
                state: OutputState::Error,
                message: Some(message),
                ..OutputStatus::stopped(syphon::is_supported(), name)
            });
            return;
        }
    };

    let live = Arc::new(AtomicBool::new(true));
    let gated: StatusReporter = {
        let live = Arc::clone(&live);
        Arc::new(move |s| {
            if live.load(Ordering::Relaxed) {
                report(s)
            }
        })
    };
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    start(&key, path, shutdown_rx, gated);
    *slot = Some(Running {
        key,
        shutdown: shutdown_tx,
        live,
    });
}
