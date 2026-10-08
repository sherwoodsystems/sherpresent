//! # State Manager
//!
//! The single owner of presentation state: every control surface (the app UI,
//! OSC, the stage page's WebSocket and REST API) sends commands here, and one
//! poller keeps the cache in sync with the presentation app.
//!
//! ## The Problem We're Solving
//!
//! Controlling PowerPoint/Keynote via AppleScript is slow (~100-500ms per call).
//! If we wait for AppleScript to complete before sending feedback, the user
//! sees noticeable lag. AppleScript calls also serialize on one channel, so two
//! pollers asking the same question would just double the load on the app.
//!
//! ## The Solution: Optimistic Updates
//!
//! When a command comes in (e.g., "next slide"):
//! 1. **Immediately** update the cached state (assume the command will succeed)
//! 2. **Immediately** publish it (UI event, web server, OSC feedback)
//! 3. **In the background**, execute the actual AppleScript command
//! 4. Apply the slide the adapter reports back; the poller catches anything else
//!
//! ## Where State Goes
//!
//! Every change is published three ways: a `presentation-status` Tauri event
//! for the UI, `status_broadcast` for the web server and Syphon notes, and
//! `subscribe()` for OSC feedback. Notes go to `notes-cache-updated` and
//! `notes_broadcast`.
//!
//! ## Key Rust Concepts Used
//!
//! - `Arc<Self>` - methods that spawn background work take `self: &Arc<Self>`
//!   so the task can keep the manager alive
//! - `spawn_blocking` - Run blocking code (AppleScript) without blocking the async runtime
//! - `tokio::sync::broadcast` - Fan state changes out to any number of listeners

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tauri::async_runtime::{self, JoinHandle};
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast;
use tokio::time::{interval, Duration};

use super::latency::{self, CommandSource, LatencyStore};
use crate::adapters::{self, canva::CanvaAdapter, LiveStatus, PresentationAdapter, SlideInfo};
use crate::config::AdapterConfig;
use crate::util::{now_ms, LockExt};

/// Polls are skipped for this long after a command, so they don't compete
/// with it for the Apple Event channel during active use.
const COMMAND_QUIET_MS: u64 = 3000;

pub type NotesCache = HashMap<i32, String>;

// =============================================================================
// CACHED STATE
// =============================================================================

/// The cached presentation state.
///
/// This struct mirrors the `LiveStatus` from adapters, but adds a timestamp
/// for debugging and staleness detection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CachedState {
    /// Is a presentation file currently open?
    pub is_open: bool,

    /// Is a slideshow currently running?
    pub is_presenting: bool,

    /// Current slide number (1-indexed, 0 if not presenting)
    pub current_slide: i32,

    /// Total number of slides in the presentation
    pub total_slides: i32,

    /// Notes zoom level percentage (100, 150, 200, etc.)
    /// None if not available (e.g., Keynote doesn't support this)
    pub zoom_level: Option<i32>,

    /// Current build/animation step on this slide (0 = no builds fired)
    pub current_build: Option<i32>,

    /// Total click-triggered build steps on this slide (None/0 = no builds)
    pub total_builds: Option<i32>,

    /// When this state was last updated (Unix ms, not serialized)
    #[serde(skip)]
    pub last_updated_ms: u64,
}

impl CachedState {
    /// Create a new state with the current timestamp
    fn now() -> Self {
        Self {
            last_updated_ms: now_ms(),
            ..Default::default()
        }
    }
}

impl From<&CachedState> for LiveStatus {
    fn from(state: &CachedState) -> Self {
        Self {
            is_open: state.is_open,
            is_presenting: state.is_presenting,
            current_slide: state.current_slide,
            total_slides: state.total_slides,
            zoom_level: state.zoom_level,
            presenter_notes: None,
            current_build: state.current_build,
            total_builds: state.total_builds,
        }
    }
}

impl From<&LiveStatus> for CachedState {
    fn from(status: &LiveStatus) -> Self {
        Self {
            is_open: status.is_open,
            is_presenting: status.is_presenting,
            current_slide: status.current_slide,
            total_slides: status.total_slides,
            zoom_level: status.zoom_level,
            current_build: status.current_build,
            total_builds: status.total_builds,
            last_updated_ms: now_ms(),
        }
    }
}

// =============================================================================
// STATE MANAGER
// =============================================================================

/// Which presentation is being controlled. Changed from Settings.
#[derive(Debug, Clone, Default, PartialEq)]
struct Target {
    adapter: String,
    presentation: String,
    adapter_config: AdapterConfig,
}

/// The channels and shared caches state is published to.
#[derive(Clone)]
pub struct StateSinks {
    pub latency_store: Arc<LatencyStore>,
    pub status_broadcast: broadcast::Sender<LiveStatus>,
    pub notes: Arc<Mutex<NotesCache>>,
    pub notes_broadcast: broadcast::Sender<NotesCache>,
    pub canva_adapter: Arc<Mutex<Option<CanvaAdapter>>>,
}

impl Default for StateSinks {
    fn default() -> Self {
        Self {
            latency_store: Arc::new(LatencyStore::new(50)),
            status_broadcast: broadcast::channel(64).0,
            notes: Arc::default(),
            notes_broadcast: broadcast::channel(64).0,
            canva_adapter: Arc::default(),
        }
    }
}

/// Manages presentation state with optimistic updates and background polling.
pub struct StateManager {
    state: Mutex<CachedState>,
    target: Mutex<Target>,

    /// Bumped on every target change, so an in-flight poll or command for the
    /// previous presentation can't overwrite the new one's state.
    generation: AtomicU64,

    /// State changes, for OSC feedback
    changes: broadcast::Sender<CachedState>,

    sinks: StateSinks,

    /// Set once Tauri is up; events to the UI are skipped before that (and in tests).
    app_handle: OnceLock<AppHandle>,

    polling_handle: Mutex<Option<JoinHandle<()>>>,

    /// Prevents overlapping refreshes.
    refresh_in_progress: AtomicBool,

    /// Unix ms of the last slide command; polling pauses for a while after it.
    last_command_at: AtomicU64,

    /// Navigation commands sent to the adapter but not yet answered. Only the
    /// last one's answer is applied: an earlier one would undo the optimistic
    /// state of the presses after it.
    commands_in_flight: AtomicUsize,
}

impl StateManager {
    pub fn new(sinks: StateSinks) -> Self {
        Self {
            state: Mutex::new(CachedState::now()),
            target: Mutex::default(),
            generation: AtomicU64::new(0),
            changes: broadcast::channel(32).0,
            sinks,
            app_handle: OnceLock::new(),
            polling_handle: Mutex::new(None),
            refresh_in_progress: AtomicBool::new(false),
            last_command_at: AtomicU64::new(0),
            commands_in_flight: AtomicUsize::new(0),
        }
    }

    /// Connect to Tauri so state changes reach the UI as events.
    pub fn attach(&self, app: AppHandle) {
        let _ = self.app_handle.set(app);
    }

    // =========================================================================
    // STATE ACCESS
    // =========================================================================

    /// Get a copy of the current cached state (instant, no AppleScript).
    pub fn get_state(&self) -> CachedState {
        self.state.locked().clone()
    }

    /// Receive every state change (used for OSC feedback).
    pub fn subscribe(&self) -> broadcast::Receiver<CachedState> {
        self.changes.subscribe()
    }

    pub fn adapter_name(&self) -> String {
        self.target.locked().adapter.clone()
    }

    pub fn presentation_name(&self) -> String {
        self.target.locked().presentation.clone()
    }

    /// Run `f` on the named adapter, using the current adapter config.
    pub fn with_adapter<T>(
        &self,
        adapter_name: &str,
        f: impl FnOnce(&dyn PresentationAdapter) -> T,
    ) -> Result<T, String> {
        let config = self.target.locked().adapter_config.clone();
        adapters::with_adapter(adapter_name, &config, &self.sinks.canva_adapter, f)
    }

    /// Run `f` on the current adapter and presentation.
    pub fn with_current_adapter<T>(
        &self,
        f: impl FnOnce(&dyn PresentationAdapter, &str) -> T,
    ) -> Result<T, String> {
        let target = self.target.locked().clone();
        if target.presentation.is_empty() {
            return Err("No presentation selected".to_string());
        }
        adapters::with_adapter(
            &target.adapter,
            &target.adapter_config,
            &self.sinks.canva_adapter,
            |a| f(a, &target.presentation),
        )
    }

    /// Switch to a different adapter, presentation or adapter config. A no-op
    /// when nothing changed, so it's safe to call on every config save.
    pub fn set_target(
        self: &Arc<Self>,
        adapter: String,
        presentation: String,
        adapter_config: AdapterConfig,
    ) {
        let new = Target {
            adapter,
            presentation,
            adapter_config,
        };
        let presentation_changed = {
            let mut target = self.target.locked();
            if *target == new {
                return;
            }
            let changed = target.adapter != new.adapter || target.presentation != new.presentation;
            log::info!(
                "StateManager: target {}/{:?}",
                new.adapter,
                new.presentation
            );
            *target = new;
            changed
        };
        self.generation.fetch_add(1, Ordering::SeqCst);

        if presentation_changed {
            self.clear_notes();
            // Compiled scripts embed the presentation name.
            crate::applescript::clear_compiled_cache();
        }
        self.replace_state(CachedState::now());
        self.refresh_state();
    }

    /// Check if two states differ in meaningful ways.
    fn has_state_changed(old: &CachedState, new: &CachedState) -> bool {
        old.is_open != new.is_open
            || old.is_presenting != new.is_presenting
            || old.current_slide != new.current_slide
            || old.total_slides != new.total_slides
            || old.zoom_level != new.zoom_level
            || old.current_build != new.current_build
            || old.total_builds != new.total_builds
    }

    /// Store `new` and publish it if it differs. Returns the previous state.
    fn replace_state(&self, new: CachedState) -> CachedState {
        let old = std::mem::replace(&mut *self.state.locked(), new.clone());
        if Self::has_state_changed(&old, &new) {
            self.publish(&new);
        }
        old
    }

    /// Publish a state change to every listener.
    fn publish(&self, state: &CachedState) {
        let status = LiveStatus::from(state);
        // Errors only mean nobody is listening right now.
        let _ = self.changes.send(state.clone());
        let _ = self.sinks.status_broadcast.send(status.clone());
        if let Some(app) = self.app_handle.get() {
            let _ = app.emit("presentation-status", &status);
        }
    }

    /// Apply a full status read from an adapter (a poll, or Canva pushing its state).
    pub fn apply_status(self: &Arc<Self>, status: LiveStatus) {
        let new = CachedState::from(&status);
        let slide = new.current_slide;
        let old = self.replace_state(new);

        if let Some(notes) = status.presenter_notes {
            if slide > 0 && !notes.is_empty() {
                self.add_notes([(slide, notes)]);
            }
        }

        // A show just started: fetch every slide's notes at once rather than
        // waiting for each slide to be visited.
        if status.is_presenting && !old.is_presenting {
            self.fetch_all_notes_in_background();
        }
    }

    // =========================================================================
    // NOTES
    // =========================================================================

    pub fn notes(&self) -> NotesCache {
        self.sinks.notes.locked().clone()
    }

    /// Merge notes into the cache and publish if anything changed.
    pub fn add_notes(&self, notes: impl IntoIterator<Item = (i32, String)>) {
        let snapshot = {
            let mut cache = self.sinks.notes.locked();
            let mut changed = false;
            for (slide, text) in notes {
                if cache.get(&slide) != Some(&text) {
                    cache.insert(slide, text);
                    changed = true;
                }
            }
            if !changed {
                return;
            }
            cache.clone()
        };
        self.publish_notes(snapshot);
    }

    pub fn clear_notes(&self) {
        self.sinks.notes.locked().clear();
        self.publish_notes(NotesCache::new());
    }

    fn publish_notes(&self, snapshot: NotesCache) {
        if let Some(app) = self.app_handle.get() {
            let _ = app.emit("notes-cache-updated", &snapshot);
        }
        let _ = self.sinks.notes_broadcast.send(snapshot);
    }

    fn fetch_all_notes_in_background(self: &Arc<Self>) {
        let sm = self.clone();
        let generation = self.generation.load(Ordering::SeqCst);
        async_runtime::spawn_blocking(move || {
            log::info!("StateManager: presenting started, fetching all notes");
            match sm.with_current_adapter(|a, name| a.get_all_presenter_notes(name)) {
                Ok(Ok(notes)) if sm.generation.load(Ordering::SeqCst) == generation => {
                    sm.add_notes(notes)
                }
                Ok(Ok(_)) => {}
                Ok(Err(e)) | Err(e) => log::warn!("Bulk notes fetch failed: {}", e),
            }
        });
    }

    // =========================================================================
    // COMMANDS (with optimistic updates)
    // =========================================================================

    /// Advance to the next slide (or the next build on this slide).
    pub fn next_slide(self: &Arc<Self>, source: CommandSource) {
        self.navigate(
            "next".to_string(),
            source,
            |state| {
                let has_remaining_builds = matches!(
                    (state.current_build, state.total_builds),
                    (Some(current), Some(total)) if current < total
                );
                if state.current_slide >= state.total_slides && !has_remaining_builds {
                    return false;
                }
                // A build step stays on the same slide.
                if !has_remaining_builds {
                    state.current_slide += 1;
                }
                true
            },
            |adapter, name| adapter.next_slide(name),
        );
    }

    /// Go to the previous slide (or back one build on this slide).
    pub fn prev_slide(self: &Arc<Self>, source: CommandSource) {
        self.navigate(
            "prev".to_string(),
            source,
            |state| {
                let has_fired_builds = state.current_build.is_some_and(|b| b > 0);
                if state.current_slide <= 1 && !has_fired_builds {
                    return false;
                }
                if !has_fired_builds {
                    state.current_slide -= 1;
                }
                true
            },
            |adapter, name| adapter.prev_slide(name),
        );
    }

    /// Jump to a specific slide.
    pub fn goto_slide(self: &Arc<Self>, slide: i32, source: CommandSource) {
        self.navigate(
            format!("goto:{}", slide),
            source,
            |state| {
                if slide < 1 || slide > state.total_slides {
                    return false;
                }
                state.current_slide = slide;
                true
            },
            move |adapter, name| adapter.goto_slide(name, slide),
        );
    }

    /// Shared body of the navigation commands: apply `optimistic` to the cache
    /// and publish it now, then run `command` on the adapter in the background.
    fn navigate(
        self: &Arc<Self>,
        label: String,
        source: CommandSource,
        optimistic: impl FnOnce(&mut CachedState) -> bool,
        command: impl FnOnce(&dyn PresentationAdapter, &str) -> Result<SlideInfo, String>
            + Send
            + 'static,
    ) {
        let (before, after) = {
            let mut state = self.state.locked();
            if !state.is_presenting {
                return;
            }
            let before = state.current_slide;
            if !optimistic(&mut state) {
                return;
            }
            state.last_updated_ms = now_ms();
            (before, state.clone())
        };
        self.last_command_at.store(now_ms(), Ordering::SeqCst);
        self.publish(&after);

        let sm = self.clone();
        let generation = self.generation.load(Ordering::SeqCst);
        let adapter_label = self.adapter_name();
        self.commands_in_flight.fetch_add(1, Ordering::SeqCst);

        async_runtime::spawn(async move {
            let started = latency::monotonic_ms();
            let sm2 = sm.clone();
            let result =
                async_runtime::spawn_blocking(move || sm2.with_current_adapter(command)).await;

            let event = latency::make_event(
                started,
                latency::monotonic_ms(),
                label,
                source,
                adapter_label,
            );
            sm.sinks.latency_store.push(event.clone());
            if let Some(app) = sm.app_handle.get() {
                let _ = app.emit("latency-event", &event);
            }

            let last = sm.commands_in_flight.fetch_sub(1, Ordering::SeqCst) == 1;
            match result {
                Ok(Ok(Ok(info))) if last => sm.apply_slide_info(generation, before, info),
                Ok(Ok(Ok(_))) => {}
                Ok(Ok(Err(e))) | Ok(Err(e)) => log::warn!("Adapter command failed: {}", e),
                Err(e) => log::warn!("Adapter task panicked: {:?}", e),
            }
        });
    }

    /// Apply the slide position an adapter reported after a command.
    fn apply_slide_info(&self, generation: u64, before: i32, info: SlideInfo) {
        if self.generation.load(Ordering::SeqCst) != generation {
            return;
        }
        let mut new = self.get_state();
        // Some adapters still report the old slide while a transition plays.
        // Keep the optimistic number then; the next poll settles it.
        if !(info.current == before && new.current_slide != before) {
            new.current_slide = info.current;
        }
        new.total_slides = info.total;
        new.last_updated_ms = now_ms();
        self.replace_state(new);
    }

    /// Increase notes zoom level (optimistic update).
    pub fn zoom_in(self: &Arc<Self>) {
        self.zoom(adapters::get_next_zoom_level);
    }

    /// Decrease notes zoom level (optimistic update).
    pub fn zoom_out(self: &Arc<Self>) {
        self.zoom(adapters::get_prev_zoom_level);
    }

    fn zoom(self: &Arc<Self>, step: impl FnOnce(i32) -> i32) {
        let (level, new) = {
            let mut state = self.state.locked();
            if !state.is_presenting {
                return;
            }
            let level = step(state.zoom_level.unwrap_or(100));
            state.zoom_level = Some(level);
            state.last_updated_ms = now_ms();
            (level, state.clone())
        };
        self.publish(&new);

        let sm = self.clone();
        async_runtime::spawn_blocking(move || {
            match sm.with_current_adapter(|a, _| a.set_notes_zoom(level)) {
                Ok(Ok(())) => {}
                Ok(Err(e)) | Err(e) => {
                    log::warn!("Zoom command failed: {}", e);
                    // Put the real level back.
                    sm.refresh_state();
                }
            }
        });
    }

    // =========================================================================
    // STATE REFRESH
    // =========================================================================

    /// Trigger a background state refresh. Coalesces with one already running.
    pub fn refresh_state(self: &Arc<Self>) {
        let sm = self.clone();
        async_runtime::spawn(async move { sm.refresh().await });
    }

    /// Read the full status from the presentation app and apply it.
    async fn refresh(self: &Arc<Self>) {
        if self.refresh_in_progress.swap(true, Ordering::SeqCst) {
            return;
        }
        let generation = self.generation.load(Ordering::SeqCst);
        let sm = self.clone();
        let status = async_runtime::spawn_blocking(move || {
            sm.with_current_adapter(|a, name| a.get_live_status(name))
                .unwrap_or_default()
        })
        .await
        .unwrap_or_default();
        self.refresh_in_progress.store(false, Ordering::SeqCst);

        if self.generation.load(Ordering::SeqCst) == generation {
            self.apply_status(status);
        }
    }

    // =========================================================================
    // POLLING
    // =========================================================================

    /// Start background polling for external state changes (the presenter's
    /// own keyboard or clicker bypasses us). Runs for the app's lifetime.
    pub fn start_polling(self: &Arc<Self>, interval_ms: u64) {
        let mut handle = self.polling_handle.locked();
        if handle.is_some() {
            return;
        }
        let sm = self.clone();
        *handle = Some(async_runtime::spawn(async move {
            let mut timer = interval(Duration::from_millis(interval_ms));
            loop {
                timer.tick().await;
                let since_command =
                    now_ms().saturating_sub(sm.last_command_at.load(Ordering::SeqCst));
                if sm.presentation_name().is_empty() || since_command < COMMAND_QUIET_MS {
                    continue;
                }
                sm.refresh().await;
            }
        }));
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn presenting(current: i32, total: i32) -> Arc<StateManager> {
        let sm = Arc::new(StateManager::new(StateSinks::default()));
        *sm.state.locked() = CachedState {
            is_open: true,
            is_presenting: true,
            current_slide: current,
            total_slides: total,
            ..Default::default()
        };
        sm
    }

    #[test]
    fn test_default_state() {
        let state = CachedState::default();
        assert!(!state.is_open);
        assert!(!state.is_presenting);
        assert_eq!(state.current_slide, 0);
        assert_eq!(state.total_slides, 0);
        assert_eq!(state.zoom_level, None);
    }

    #[test]
    fn test_state_change_detection() {
        let old = CachedState::default();
        let mut new = CachedState::default();

        // Same state = no change
        assert!(!StateManager::has_state_changed(&old, &new));

        // Different slide = change
        new.current_slide = 5;
        assert!(StateManager::has_state_changed(&old, &new));
    }

    #[test]
    fn test_live_status_round_trip() {
        let status = LiveStatus {
            is_open: true,
            is_presenting: true,
            current_slide: 3,
            total_slides: 9,
            zoom_level: Some(150),
            presenter_notes: Some("hi".into()),
            current_build: Some(1),
            total_builds: Some(2),
        };
        let back = LiveStatus::from(&CachedState::from(&status));
        assert_eq!(
            back,
            LiveStatus {
                presenter_notes: None,
                ..status
            }
        );
    }

    #[test]
    fn test_apply_slide_info_keeps_optimistic_slide_during_transition() {
        let sm = presenting(3, 10); // optimistically moved from 2
        sm.apply_slide_info(
            0,
            2,
            SlideInfo {
                current: 2,
                total: 10,
                transition_duration: Some(1.0),
            },
        );
        assert_eq!(sm.get_state().current_slide, 3);

        sm.apply_slide_info(
            0,
            2,
            SlideInfo {
                current: 4,
                total: 10,
                transition_duration: None,
            },
        );
        assert_eq!(sm.get_state().current_slide, 4);
    }

    #[test]
    fn test_apply_slide_info_ignores_stale_generation() {
        let sm = presenting(3, 10);
        sm.generation.fetch_add(1, Ordering::SeqCst);
        sm.apply_slide_info(
            0,
            2,
            SlideInfo {
                current: 7,
                total: 10,
                transition_duration: None,
            },
        );
        assert_eq!(sm.get_state().current_slide, 3);
    }

    #[test]
    fn test_add_notes_publishes_only_changes() {
        let sm = StateManager::new(StateSinks::default());
        let mut rx = sm.sinks.notes_broadcast.subscribe();
        sm.add_notes([(1, "a".to_string())]);
        sm.add_notes([(1, "a".to_string())]);
        assert_eq!(rx.try_recv().unwrap().get(&1).unwrap(), "a");
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_publish_reaches_subscribers() {
        let sm = presenting(1, 5);
        let mut rx = sm.subscribe();
        let mut new = sm.get_state();
        new.current_slide = 2;
        sm.replace_state(new);
        assert_eq!(rx.try_recv().unwrap().current_slide, 2);
    }
}
