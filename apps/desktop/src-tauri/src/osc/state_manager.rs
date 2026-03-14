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
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use crate::adapters::{get_adapter, powerpoint::PowerPointAdapter, PresentationAdapter};
use crate::config::AdapterConfig;

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
}

impl StateManager {
    /// Create a new StateManager.
    pub fn new(
        adapter_name: String,
        presentation_name: String,
        adapter_config: AdapterConfig,
        state_change_tx: mpsc::Sender<CachedState>,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(CachedState::now())),
            adapter_name,
            presentation_name,
            adapter_config,
            state_change_tx,
            polling_handle: Mutex::new(None),
            refresh_in_progress: Arc::new(Mutex::new(false)),
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
    }

    /// Send state change notification through the channel.
    fn notify_state_change(&self, state: CachedState) {
        // try_send is non-blocking - if the channel is full, we just skip
        // This is fine because we'll send updated state soon anyway
        let _ = self.state_change_tx.try_send(state);
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
    pub fn next_slide(&self) {
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
        self.spawn_adapter_command(|adapter, name| adapter.next_slide(&name));
    }

    /// Go to the previous slide (optimistic update).
    pub fn prev_slide(&self) {
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
        self.spawn_adapter_command(|adapter, name| adapter.prev_slide(&name));
    }

    /// Jump to a specific slide (optimistic update).
    pub fn goto_slide(&self, slide: i32) {
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
        self.spawn_adapter_command(move |adapter, name| adapter.goto_slide(&name, slide));
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
    fn spawn_adapter_command<F>(&self, command: F)
    where
        F: FnOnce(&dyn crate::adapters::PresentationAdapter, String) -> Result<crate::adapters::SlideInfo, String>
            + Send
            + 'static,
    {
        let adapter_name = self.adapter_name.clone();
        let presentation_name = self.presentation_name.clone();
        let adapter_config = self.adapter_config.clone();
        let refresh_flag = self.refresh_in_progress.clone();
        let state = self.state.clone();
        let tx = self.state_change_tx.clone();

        // Spawn the blocking work on a separate thread
        tokio::task::spawn(async move {
            // Clone for use after the spawn_blocking closure consumes originals
            let adapter_name_for_refresh = adapter_name.clone();
            let presentation_name_for_refresh = presentation_name.clone();
            let adapter_config_for_refresh = adapter_config.clone();

            // Run the blocking AppleScript on a blocking thread
            let result = tokio::task::spawn_blocking(move || {
                if let Some(adapter) = get_adapter(&adapter_name, &adapter_config) {
                    command(adapter.as_ref(), presentation_name)
                } else {
                    Err("Adapter not found".to_string())
                }
            })
            .await;

            // After command completes, schedule a refresh
            // Wait 50ms to let PowerPoint settle
            tokio::time::sleep(Duration::from_millis(50)).await;

            // Trigger a refresh (reusing the logic)
            Self::do_refresh_internal(
                adapter_name_for_refresh,
                presentation_name_for_refresh,
                adapter_config_for_refresh,
                refresh_flag,
                state,
                tx,
            )
            .await;

            // Log errors for debugging
            if let Err(e) = result {
                log::warn!("Adapter command failed: {:?}", e);
            }
        });
    }

    /// Spawn a background task to set zoom level.
    fn spawn_zoom_command(&self, level: i32) {
        let refresh_flag = self.refresh_in_progress.clone();
        let state = self.state.clone();
        let tx = self.state_change_tx.clone();
        let adapter_name = self.adapter_name.clone();
        let presentation_name = self.presentation_name.clone();
        let adapter_config = self.adapter_config.clone();

        tokio::task::spawn(async move {
            // Run the blocking AppleScript
            let _ = tokio::task::spawn_blocking(move || {
                let adapter = PowerPointAdapter;
                adapter.set_notes_zoom(level)
            })
            .await;

            // Refresh after zoom completes
            tokio::time::sleep(Duration::from_millis(50)).await;
            Self::do_refresh_internal(adapter_name, presentation_name, adapter_config, refresh_flag, state, tx)
                .await;
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

        // Get presentation state
        if let Ok(pres_state) = adapter.get_presentation_state(presentation_name) {
            new_state.is_open = pres_state.is_open;
            new_state.is_presenting = pres_state.is_presenting;

            // Only fetch slide info if presenting
            if pres_state.is_presenting {
                if let Ok(slide_info) = adapter.get_slide_info(presentation_name) {
                    new_state.current_slide = slide_info.current;
                    new_state.total_slides = slide_info.total;
                }

                // Get zoom level
                if let Ok(Some(zoom)) = adapter.get_notes_zoom() {
                    new_state.zoom_level = Some(zoom);
                }
            }
        }

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

        // Spawn the polling task
        let handle = tokio::task::spawn(async move {
            let mut timer = interval(Duration::from_millis(interval_ms));

            loop {
                timer.tick().await;

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
