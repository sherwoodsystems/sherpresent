//! # Channel Manager
//!
//! Coordinates peer communication and command forwarding within a channel.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use super::peer::Peer;

/// Commands that flow through the channel system
#[derive(Debug, Clone)]
pub enum ChannelCommand {
    // Commands to broadcast to peers (from local action)
    BroadcastNext,
    BroadcastPrev,
    BroadcastGoto(i32),

    // Commands received from peers (to execute locally)
    ExecuteNext { origin: String },
    ExecutePrev { origin: String },
    ExecuteGoto { origin: String, slide: i32 },

    // Peer lifecycle
    PeerJoined(Peer),
    PeerLeft(String), // instance_id
    PeerHeartbeat(String),
}

/// Manages channel membership and peer communication
pub struct ChannelManager {
    /// Our unique instance identifier
    instance_id: String,
    /// Channel name we're in
    channel_name: String,
    /// Known peers (keyed by instance_id)
    peers: Arc<Mutex<HashMap<String, Peer>>>,
    /// Channel for sending commands to be processed
    command_tx: mpsc::Sender<ChannelCommand>,
}

impl ChannelManager {
    /// Create a new channel manager
    pub fn new(
        instance_id: String,
        channel_name: String,
        command_tx: mpsc::Sender<ChannelCommand>,
    ) -> Self {
        Self {
            instance_id,
            channel_name,
            peers: Arc::new(Mutex::new(HashMap::new())),
            command_tx,
        }
    }

    /// Get our instance ID
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    /// Get the channel name
    pub fn channel_name(&self) -> &str {
        &self.channel_name
    }

    /// Add or update a peer
    pub fn add_peer(&self, peer: Peer) {
        let mut peers = self.peers.lock().unwrap();
        if let Some(existing) = peers.get_mut(&peer.instance_id) {
            existing.touch();
            existing.address = peer.address;
        } else {
            log::info!("Peer joined channel: {} at {}", peer.instance_id, peer.address);
            peers.insert(peer.instance_id.clone(), peer);
        }
    }

    /// Remove a peer
    pub fn remove_peer(&self, instance_id: &str) {
        let mut peers = self.peers.lock().unwrap();
        if peers.remove(instance_id).is_some() {
            log::info!("Peer left channel: {}", instance_id);
        }
    }

    /// Update peer heartbeat
    pub fn touch_peer(&self, instance_id: &str) {
        let mut peers = self.peers.lock().unwrap();
        if let Some(peer) = peers.get_mut(instance_id) {
            peer.touch();
        }
    }

    /// Get addresses of all active (non-stale) peers
    pub fn get_peer_addresses(&self) -> Vec<SocketAddr> {
        let peers = self.peers.lock().unwrap();
        peers
            .values()
            .filter(|p| !p.is_stale())
            .map(|p| p.address)
            .collect()
    }

    /// Get all peers
    pub fn get_peers(&self) -> Vec<Peer> {
        let peers = self.peers.lock().unwrap();
        peers.values().cloned().collect()
    }

    /// Check if a command should be executed (i.e., it didn't originate from us)
    pub fn should_execute(&self, origin_id: &str) -> bool {
        origin_id != self.instance_id
    }

    /// Send a command through the channel
    pub async fn send_command(&self, cmd: ChannelCommand) {
        if let Err(e) = self.command_tx.send(cmd).await {
            log::warn!("Failed to send channel command: {}", e);
        }
    }

    /// Remove stale peers (call periodically)
    pub fn prune_stale_peers(&self) -> Vec<String> {
        let mut peers = self.peers.lock().unwrap();
        let stale: Vec<String> = peers
            .iter()
            .filter(|(_, p)| p.is_stale())
            .map(|(id, _)| id.clone())
            .collect();

        for id in &stale {
            log::info!("Removing stale peer: {}", id);
            peers.remove(id);
        }

        stale
    }

    /// Update channel name
    pub fn set_channel_name(&mut self, name: String) {
        self.channel_name = name;
        // Clear peers since we changed channels
        let mut peers = self.peers.lock().unwrap();
        peers.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_peer_management() {
        let (tx, _rx) = mpsc::channel(32);
        let manager = ChannelManager::new("test-id".to_string(), "test-channel".to_string(), tx);

        let peer = Peer::new(
            "peer-1".to_string(),
            "192.168.1.100:9000".parse().unwrap(),
            "test-channel".to_string(),
        );

        manager.add_peer(peer);
        assert_eq!(manager.get_peers().len(), 1);

        manager.remove_peer("peer-1");
        assert_eq!(manager.get_peers().len(), 0);
    }

    #[test]
    fn test_should_execute() {
        let (tx, _rx) = mpsc::channel(32);
        let manager = ChannelManager::new("my-id".to_string(), "channel".to_string(), tx);

        assert!(!manager.should_execute("my-id"));
        assert!(manager.should_execute("other-id"));
    }
}
