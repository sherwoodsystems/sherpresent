use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// How a slide command was initiated.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandSource {
    Osc,
    Ws,
    Ui,
}

/// A single latency measurement for an adapter command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyEvent {
    /// Monotonic timestamp when the command was received (ms since app start)
    pub command_received_ms: u64,
    /// Monotonic timestamp when the adapter finished (ms since app start)
    pub adapter_complete_ms: u64,
    /// Computed latency in ms
    pub latency_ms: u64,
    /// Human-readable command label (e.g. "next", "prev", "goto:5")
    pub command: String,
    /// Where the command originated
    pub source: CommandSource,
    /// Which adapter handled it (e.g. "powerpoint", "keynote")
    pub adapter: String,
    /// Wall-clock timestamp for display (Unix ms)
    pub wall_clock_ms: u64,
}

/// Ring buffer that stores the most recent latency events.
pub struct LatencyStore {
    events: Mutex<VecDeque<LatencyEvent>>,
    capacity: usize,
}

impl LatencyStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            events: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
        }
    }

    pub fn push(&self, event: LatencyEvent) {
        let mut events = self.events.lock().unwrap();
        if events.len() >= self.capacity {
            events.pop_front();
        }
        events.push_back(event);
    }

    pub fn get_all(&self) -> Vec<LatencyEvent> {
        let events = self.events.lock().unwrap();
        events.iter().cloned().collect()
    }

    pub fn clear(&self) {
        let mut events = self.events.lock().unwrap();
        events.clear();
    }
}

/// Returns milliseconds since the first call (monotonic, for duration measurement).
pub fn monotonic_ms() -> u64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    let epoch = EPOCH.get_or_init(Instant::now);
    epoch.elapsed().as_millis() as u64
}

/// Returns current wall-clock time in Unix milliseconds (for display).
fn wall_clock_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Build a LatencyEvent from before/after timestamps.
pub fn make_event(
    before_ms: u64,
    after_ms: u64,
    command: String,
    source: CommandSource,
    adapter: String,
) -> LatencyEvent {
    LatencyEvent {
        command_received_ms: before_ms,
        adapter_complete_ms: after_ms,
        latency_ms: after_ms.saturating_sub(before_ms),
        command,
        source,
        adapter,
        wall_clock_ms: wall_clock_ms(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latency_store_ring_buffer() {
        let store = LatencyStore::new(3);
        for i in 0..5 {
            store.push(LatencyEvent {
                command_received_ms: i,
                adapter_complete_ms: i + 10,
                latency_ms: 10,
                command: format!("cmd{}", i),
                source: CommandSource::Ui,
                adapter: "test".to_string(),
                wall_clock_ms: 0,
            });
        }
        let events = store.get_all();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].command, "cmd2");
        assert_eq!(events[2].command, "cmd4");
    }

    #[test]
    fn test_latency_store_clear() {
        let store = LatencyStore::new(10);
        store.push(LatencyEvent {
            command_received_ms: 0,
            adapter_complete_ms: 10,
            latency_ms: 10,
            command: "test".to_string(),
            source: CommandSource::Osc,
            adapter: "test".to_string(),
            wall_clock_ms: 0,
        });
        assert_eq!(store.get_all().len(), 1);
        store.clear();
        assert_eq!(store.get_all().len(), 0);
    }

    #[test]
    fn test_monotonic_ms_increases() {
        let a = monotonic_ms();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let b = monotonic_ms();
        assert!(b > a);
    }
}
