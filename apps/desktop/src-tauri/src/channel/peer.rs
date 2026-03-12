//! # Peer
//!
//! Represents a remote sher-present instance in the same channel.

use std::net::SocketAddr;
use std::time::Instant;

/// A peer in the same channel
#[derive(Debug, Clone)]
pub struct Peer {
    /// Unique instance identifier
    pub instance_id: String,
    /// Network address (IP:port)
    pub address: SocketAddr,
    /// Channel name
    pub channel: String,
    /// Last time we heard from this peer
    pub last_seen: Instant,
}

impl Peer {
    /// Create a new peer
    pub fn new(instance_id: String, address: SocketAddr, channel: String) -> Self {
        Self {
            instance_id,
            address,
            channel,
            last_seen: Instant::now(),
        }
    }

    /// Check if this peer is stale (no heartbeat in 15 seconds)
    pub fn is_stale(&self) -> bool {
        self.last_seen.elapsed() > std::time::Duration::from_secs(15)
    }

    /// Update the last seen time
    pub fn touch(&mut self) {
        self.last_seen = Instant::now();
    }
}
