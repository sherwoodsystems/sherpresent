//! # State Manager
//!
//! Manages cached presentation state with **optimistic updates** for fast feedback.
//!
//! ## The Problem We're Solving
//!
//! Controlling PowerPoint/Keynote via AppleScript is slow (~100-500ms per call).
//! If we wait for AppleScript to complete before sending OSC feedback, the user
//! sees noticeable lag. This feels unresponsive and unprofessional.
//!
//! ## The Solution: Optimistic Updates
//!
//! When a command comes in (e.g., "next slide"):
//! 1. **Immediately** update the cached state (assume the command will succeed)
//! 2. **Immediately** send feedback to the OSC client
//! 3. **In the background**, execute the actual AppleScript command
//! 4. **After a short delay**, verify the state matches reality
//!
//! This makes the system feel instant while still being accurate.
//!
//! ## Key Rust Concepts Used
//!
//! - `Arc<Mutex<T>>` - Shared mutable state across async tasks
//! - `tokio::task::spawn_blocking` - Run blocking code (AppleScript) without blocking the async runtime
//! - `tokio::sync::mpsc` - Channel to notify the server when state changes
//! - `tokio::task::JoinHandle` - Handle to a spawned task (for cancellation)

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use crate::adapters::{get_adapter, powerpoint::PowerPointAdapter, LiveStatus, PresentationAdapter};
use crate::config::AdapterConfig;
use super::latency::{self, CommandSource, LatencyStore};

// =============================================================================
// CACHED STATE
// =============================================================================

/// The cached presentation state.
///
/// This struct mirrors the `LiveStatus` from adapters, but adds a timestamp
/// for debugging and staleness detection.
///
/// ## Derive Macros Explained
///
/// - `Debug` - Allows printing with `{:?}` for debugging
/// - `Clone` - Allows making copies (needed for sending over channels)
/// - `Serialize/Deserialize` - JSON conversion for Tauri events
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

    /// When this state was last updated (not serialized)
    /// Using u64 milliseconds instead of Instant for serialization
    #[serde(skip)]
    pub last_updated_ms: u64,
}


impl CachedState {
    /// Create a new state with the current timestamp
    fn now() -> Self {
        Self {
            last_updated_ms: current_time_ms(),
            ..Default::default()
        }
    }
}

/// Get current time in milliseconds (for timestamps)
fn current_time_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// =============================================================================
// STATE MANAGER
// =============================================================================

/// Manages presentation state with optimistic updates and background polling.
///
/// ## Thread Safety
///
/// This struct is designed to be shared across multiple async tasks:
/// - The OSC server reads state and triggers commands
/// - The polling task updates state periodically
/// - The main Tauri thread may also access state
///
/// We use `Arc<Mutex<T>>` for the state:
/// - `Arc` (Atomic Reference Count) allows multiple owners
/// - `Mutex` ensures only one thread can access the data at a time
pub struct StateManager {
    /// The cached state, wrapped for thread-safe access.
    state: Arc<Mutex<CachedState>>,

    /// Which adapter to use ("powerpoint", "keynote", "libreoffice", "canva")
    adapter_name: String,

    /// The name of the presentation file to control
    presentation_name: String,

    /// Per-adapter network configuration
    adapter_config: AdapterConfig,

    /// Channel sender to notify when state changes.
    state_change_tx: mpsc::Sender<CachedState>,

    /// Handle to the polling task (so we can cancel it on shutdown).
    polling_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,

    /// Flag to prevent concurrent refresh operations.
    refresh_in_progress: Arc<Mutex<bool>>,

    /// Latency measurement store
    latency_store: Arc<LatencyStore>,

    /// Tauri app handle for emitting events to the frontend
    app_handle: Option<AppHandle>,

    /// Shared timestamp of last UI/OSC command (Unix ms).
    /// Polling skips cycles when a command was recent (avoids IPC contention).
    last_command_at: Arc<Mutex<u64>>,

    /// Broadcast channel for web server status updates.
    /// When present, state changes are also broadcast here so the stage view
    /// updates instantly without waiting for the separate polling loop.
    status_broadcast: Option<tokio::sync::broadcast::Sender<LiveStatus>>,
}

impl StateManager {
    /// Create a new StateManager.
    pub fn new(
        adapter_name: String,
        presentation_name: String,
        adapter_config: AdapterConfig,
        state_change_tx: mpsc::Sender<CachedState>,
        latency_store: Arc<LatencyStore>,
        app_handle: Option<AppHandle>,
        last_command_at: Arc<Mutex<u64>>,
        status_broadcast: Option<tokio::sync::broadcast::Sender<LiveStatus>>,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(CachedState::now())),
            adapter_name,
            presentation_name,
            adapter_config,
            state_change_tx,
            polling_handle: Mutex::new(None),
            refresh_in_progress: Arc::new(Mutex::new(false)),
            latency_store,
            app_handle,
            last_command_at,
            status_broadcast,
        }
    }

    // =========================================================================
    // STATE ACCESS
    // =========================================================================

    /// Get a copy of the current cached state (instant, no AppleScript).
    ///
    /// ## Why Return a Clone?
    ///
    /// We can't return a reference because the Mutex lock would need to be held.
    /// Instead, we lock briefly, clone the data, and release the lock.
    /// This is very fast since CachedState is small.
    pub fn get_state(&self) -> CachedState {
        // lock() returns a MutexGuard that auto-unlocks when dropped
        let state = self.state.lock().unwrap();
        state.clone()
    }

    /// Update the cached state and notify listeners.
    #[allow(dead_code)]
    fn set_state(&self, new_state: CachedState) {
        // Check if state actually changed
        let changed = {
            let current = self.state.lock().unwrap();
            Self::has_state_changed(&current, &new_state)
        };

        // Update the state
        {
            let mut state = self.state.lock().unwrap();
            *state = new_state.clone();
        }

        // Notify if changed
        if changed {
            self.notify_state_change(new_state);
        }
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

    /// Send state change notification through the channel.
    fn notify_state_change(&self, state: CachedState) {
        // try_send is non-blocking - if the channel is full, we just skip
        // This is fine because we'll send updated state soon anyway
        let _ = self.state_change_tx.try_send(state.clone());

        // Also broadcast to web server so stage view updates instantly
        if let Some(ref tx) = self.status_broadcast {
            let live_status = LiveStatus {
                is_open: state.is_open,
                is_presenting: state.is_presenting,
                current_slide: state.current_slide,
                total_slides: state.total_slides,
                zoom_level: state.zoom_level,
                presenter_notes: None,
                current_build: state.current_build,
                total_builds: state.total_builds,
            };
            let _ = tx.send(live_status);
        }
    }

    // =========================================================================
    // COMMANDS (with optimistic updates)
    // =========================================================================

    /// Advance to the next slide (optimistic update).
    ///
    /// ## How Optimistic Updates Work Here
    ///
    /// 1. Check if we're presenting (can't navigate if not)
    /// 2. Immediately increment current_slide in cache
    /// 3. Notify listeners (OSC feedback goes out NOW)
    /// 4. Spawn a background task to actually call PowerPoint
    /// 5. After AppleScript completes, refresh state to verify
    pub fn next_slide(&self, source: CommandSource) {
        // First, do the optimistic update
        let should_execute = {
            let mut state = self.state.lock().unwrap();

            // Can't navigate if not presenting
            if !state.is_presenting {
                return;
            }

            // Don't go past the last slide
            if state.current_slide >= state.total_slides {
                return;
            }

            // Optimistic update
            state.current_slide += 1;
            state.last_updated_ms = current_time_ms();

            true
        };

        if !should_execute {
            return;
        }

        // Notify immediately (this is the "optimistic" part)
        self.notify_state_change(self.get_state());

        // Now spawn the actual AppleScript command in the background
        self.spawn_adapter_command(
            "next".to_string(),
            source,
            |adapter, name| adapter.next_slide(&name),
        );
    }

    /// Go to the previous slide (optimistic update).
    pub fn prev_slide(&self, source: CommandSource) {
        let should_execute = {
            let mut state = self.state.lock().unwrap();

            if !state.is_presenting {
                return;
            }

            // Don't go before slide 1
            if state.current_slide <= 1 {
                return;
            }

            // Optimistic update
            state.current_slide -= 1;
            state.last_updated_ms = current_time_ms();

            true
        };

        if !should_execute {
            return;
        }

        self.notify_state_change(self.get_state());
        self.spawn_adapter_command(
            "prev".to_string(),
            source,
            |adapter, name| adapter.prev_slide(&name),
        );
    }

    /// Jump to a specific slide (optimistic update).
    pub fn goto_slide(&self, slide: i32, source: CommandSource) {
        let should_execute = {
            let mut state = self.state.lock().unwrap();

            if !state.is_presenting {
                return;
            }

            // Clamp to valid range
            if slide < 1 || slide > state.total_slides {
                return;
            }

            // Optimistic update
            state.current_slide = slide;
            state.last_updated_ms = current_time_ms();

            true
        };

        if !should_execute {
            return;
        }

        self.notify_state_change(self.get_state());
        self.spawn_adapter_command(
            format!("goto:{}", slide),
            source,
            move |adapter, name| adapter.goto_slide(&name, slide),
        );
    }

    /// Increase notes zoom level (optimistic update).
    pub fn zoom_in(&self) {
        let new_zoom = {
            let mut state = self.state.lock().unwrap();

            if !state.is_presenting {
                return;
            }

            let current = state.zoom_level.unwrap_or(100);
            let new_zoom = PowerPointAdapter::get_next_zoom_level(current);

            // Optimistic update
            state.zoom_level = Some(new_zoom);
            state.last_updated_ms = current_time_ms();

            new_zoom
        };

        self.notify_state_change(self.get_state());
        self.spawn_zoom_command(new_zoom);
    }

    /// Decrease notes zoom level (optimistic update).
    pub fn zoom_out(&self) {
        let new_zoom = {
            let mut state = self.state.lock().unwrap();

            if !state.is_presenting {
                return;
            }

            let current = state.zoom_level.unwrap_or(100);
            let new_zoom = PowerPointAdapter::get_prev_zoom_level(current);

            // Optimistic update
            state.zoom_level = Some(new_zoom);
            state.last_updated_ms = current_time_ms();

            new_zoom
        };

        self.notify_state_change(self.get_state());
        self.spawn_zoom_command(new_zoom);
    }

    // =========================================================================
    // BACKGROUND TASKS
    // =========================================================================

    /// Spawn a background task to execute an adapter command.
    ///
    /// ## Why spawn_blocking?
    ///
    /// The adapters use `std::process::Command` to run AppleScript, which
    /// blocks the thread. In async Rust, blocking the thread blocks the
    /// entire async runtime, starving other tasks.
    ///
    /// `spawn_blocking` runs the closure on a dedicated thread pool for
    /// blocking operations, keeping the async runtime free.
    fn spawn_adapter_command<F>(&self, command_label: String, source: CommandSource, command: F)
    where
        F: FnOnce(&dyn crate::adapters::PresentationAdapter, String) -> Result<crate::adapters::SlideInfo, String>
            + Send
            + 'static,
    {
        let adapter_name = self.adapter_name.clone();
        let presentation_name = self.presentation_name.clone();
        let adapter_config = self.adapter_config.clone();
        let state = self.state.clone();
        let tx = self.state_change_tx.clone();
        let latency_store = self.latency_store.clone();
        let app_handle = self.app_handle.clone();
        let adapter_label = self.adapter_name.clone();

        let before_ms = latency::monotonic_ms();

        // Spawn the blocking work on a separate thread
        tokio::task::spawn(async move {
            // Run the blocking AppleScript on a blocking thread
            let result = tokio::task::spawn_blocking(move || {
                if let Some(adapter) = get_adapter(&adapter_name, &adapter_config) {
                    command(adapter.as_ref(), presentation_name)
                } else {
                    Err("Adapter not found".to_string())
                }
            })
            .await;

            // Capture latency after adapter completes
            let after_ms = latency::monotonic_ms();
            let event = latency::make_event(before_ms, after_ms, command_label, source, adapter_label);
            latency_store.push(event.clone());
            if let Some(ref handle) = app_handle {
                let _ = handle.emit("latency-event", &event);
            }

            // Use the command result to update state directly instead of a full refresh.
            // The adapter's next_slide/prev_slide/goto_slide already return accurate SlideInfo.
            // The 2s polling cycle catches any desync from external changes.
            match result {
                Ok(Ok(slide_info)) => {
                    let changed = {
                        let mut st = state.lock().unwrap();
                        let old_current = st.current_slide;
                        let old_total = st.total_slides;
                        st.current_slide = slide_info.current;
                        st.total_slides = slide_info.total;
                        st.last_updated_ms = current_time_ms();
                        old_current != slide_info.current || old_total != slide_info.total
                    };
                    if changed {
                        let new_state = state.lock().unwrap().clone();
                        let _ = tx.try_send(new_state);
                    }
                }
                Ok(Err(e)) => {
                    log::warn!("Adapter command failed: {}", e);
                }
                Err(e) => {
                    log::warn!("Adapter task panicked: {:?}", e);
                }
            }
        });
    }

    /// Spawn a background task to set zoom level.
    fn spawn_zoom_command(&self, level: i32) {
        let state = self.state.clone();
        let tx = self.state_change_tx.clone();

        tokio::task::spawn(async move {
            // Run the blocking AppleScript
            let result = tokio::task::spawn_blocking(move || {
                let adapter = PowerPointAdapter;
                adapter.set_notes_zoom(level)
            })
            .await;

            // Update state directly with the requested zoom level on success
            match result {
                Ok(Ok(())) => {
                    {
                        let mut st = state.lock().unwrap();
                        st.zoom_level = Some(level);
                        st.last_updated_ms = current_time_ms();
                    }
                    let new_state = state.lock().unwrap().clone();
                    let _ = tx.try_send(new_state);
                }
                Ok(Err(e)) => log::warn!("Zoom command failed: {}", e),
                Err(e) => log::warn!("Zoom task panicked: {:?}", e),
            }
        });
    }

    // =========================================================================
    // STATE REFRESH
    // =========================================================================

    /// Trigger a background state refresh.
    ///
    /// This coalesces multiple refresh requests - if a refresh is already
    /// in progress, we skip this one. The ongoing refresh will pick up
    /// any changes anyway.
    pub fn refresh_state(&self) {
        // Check if refresh is already in progress
        {
            let in_progress = self.refresh_in_progress.lock().unwrap();
            if *in_progress {
                return;
            }
        }

        let adapter_name = self.adapter_name.clone();
        let presentation_name = self.presentation_name.clone();
        let adapter_config = self.adapter_config.clone();
        let refresh_flag = self.refresh_in_progress.clone();
        let state = self.state.clone();
        let tx = self.state_change_tx.clone();

        tokio::task::spawn(async move {
            Self::do_refresh_internal(adapter_name, presentation_name, adapter_config, refresh_flag, state, tx)
                .await;
        });
    }

    /// Internal refresh implementation (used by multiple callers).
    async fn do_refresh_internal(
        adapter_name: String,
        presentation_name: String,
        adapter_config: AdapterConfig,
        refresh_flag: Arc<Mutex<bool>>,
        state: Arc<Mutex<CachedState>>,
        tx: mpsc::Sender<CachedState>,
    ) {
        // Set flag to prevent concurrent refreshes
        {
            let mut in_progress = refresh_flag.lock().unwrap();
            if *in_progress {
                return;
            }
            *in_progress = true;
        }

        // Fetch state from presentation software (blocking)
        let new_state = tokio::task::spawn_blocking(move || {
            Self::fetch_all_state(&adapter_name, &presentation_name, &adapter_config)
        })
        .await
        .unwrap_or_else(|_| CachedState::now());

        // Clear the in-progress flag
        {
            let mut in_progress = refresh_flag.lock().unwrap();
            *in_progress = false;
        }

        // Check if state changed
        let changed = {
            let current = state.lock().unwrap();
            Self::has_state_changed(&current, &new_state)
        };

        // Update state
        {
            let mut current = state.lock().unwrap();
            *current = new_state.clone();
        }

        // Notify if changed
        if changed {
            let _ = tx.try_send(new_state);
        }
    }

    /// Force a synchronous state refresh (for initial load).
    ///
    /// ## When to Use
    ///
    /// Call this when starting the OSC server to ensure we have valid
    /// initial state before accepting commands.
    pub async fn force_refresh(&self) -> CachedState {
        let adapter_name = self.adapter_name.clone();
        let presentation_name = self.presentation_name.clone();
        let adapter_config = self.adapter_config.clone();

        let new_state = tokio::task::spawn_blocking(move || {
            Self::fetch_all_state(&adapter_name, &presentation_name, &adapter_config)
        })
        .await
        .unwrap_or_else(|_| CachedState::now());

        // Update state
        {
            let mut state = self.state.lock().unwrap();
            *state = new_state.clone();
        }

        new_state
    }

    /// Fetch complete state from the presentation software.
    ///
    /// This is the actual AppleScript work - it's blocking and slow.
    fn fetch_all_state(adapter_name: &str, presentation_name: &str, adapter_config: &AdapterConfig) -> CachedState {
        let mut new_state = CachedState::now();

        let Some(adapter) = get_adapter(adapter_name, adapter_config) else {
            return new_state;
        };

        let status = adapter.get_live_status(presentation_name);
        new_state.is_open = status.is_open;
        new_state.is_presenting = status.is_presenting;
        new_state.current_slide = status.current_slide;
        new_state.total_slides = status.total_slides;
        new_state.zoom_level = status.zoom_level;
        new_state.current_build = status.current_build;
        new_state.total_builds = status.total_builds;

        new_state
    }

    // =========================================================================
    // POLLING
    // =========================================================================

    /// Start background polling for external state changes.
    ///
    /// ## Why Poll?
    ///
    /// The user might change slides using their keyboard or clicker,
    /// bypassing our OSC commands. Polling every 2 seconds catches
    /// these external changes so our OSC clients stay in sync.
    ///
    /// ## Parameters
    ///
    /// - `interval_ms` - Polling interval in milliseconds (default: 2000)
    pub fn start_polling(&self, interval_ms: u64) {
        // Check if already polling
        {
            let handle = self.polling_handle.lock().unwrap();
            if handle.is_some() {
                return;
            }
        }

        let adapter_name = self.adapter_name.clone();
        let presentation_name = self.presentation_name.clone();
        let adapter_config = self.adapter_config.clone();
        let refresh_flag = self.refresh_in_progress.clone();
        let state = self.state.clone();
        let tx = self.state_change_tx.clone();
        let last_command_at = self.last_command_at.clone();

        // Spawn the polling task
        let handle = tokio::task::spawn(async move {
            let mut timer = interval(Duration::from_millis(interval_ms));

            loop {
                timer.tick().await;

                // Skip this poll cycle if a slide command was executed within the last 3s.
                // This avoids competing for the Apple Event IPC channel during active use.
                {
                    let last_cmd = *last_command_at.lock().unwrap();
                    if last_cmd > 0 {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64;
                        if now.saturating_sub(last_cmd) < 3000 {
                            continue;
                        }
                    }
                }

                Self::do_refresh_internal(
                    adapter_name.clone(),
                    presentation_name.clone(),
                    adapter_config.clone(),
                    refresh_flag.clone(),
                    state.clone(),
                    tx.clone(),
                )
                .await;
            }
        });

        // Store the handle so we can cancel later
        {
            let mut polling = self.polling_handle.lock().unwrap();
            *polling = Some(handle);
        }
    }

    /// Stop background polling.
    pub fn stop_polling(&self) {
        let handle = {
            let mut polling = self.polling_handle.lock().unwrap();
            polling.take()
        };

        if let Some(h) = handle {
            // abort() cancels the task immediately
            h.abort();
        }
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

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
}
