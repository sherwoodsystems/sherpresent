//! # mDNS Service Discovery
//!
//! Implements service registration and discovery using mDNS (Multicast DNS).
//! This allows sher-present instances to automatically find each other on
//! the local network without manual IP configuration.
//!
//! ## Service Type
//!
//! We use `_sher-present._udp.local.` as our service type.
//!
//! ## TXT Records
//!
//! Each service advertises:
//! - `channel=<name>` - The channel this instance belongs to
//! - `version=1` - Protocol version for compatibility
//! - `instance=<uuid>` - Unique instance identifier
//! - `name=<display_name>` - Human-readable display name

use mdns_sd::{IfKind, ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// The mDNS service type for sher-present
const SERVICE_TYPE: &str = "_sher-present._udp.local.";

/// Protocol version for compatibility checking
const PROTOCOL_VERSION: &str = "1";

/// Get the local LAN IP address of this machine.
fn get_local_ip() -> Option<String> {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}

/// Get all available network interfaces for mDNS.
/// Returns non-loopback IPv4 interfaces.
pub fn get_network_interfaces() -> Vec<NetworkInterface> {
    match if_addrs::get_if_addrs() {
        Ok(interfaces) => {
            interfaces
                .into_iter()
                .filter_map(|iface| {
                    if let if_addrs::IfAddr::V4(v4_addr) = &iface.addr {
                        // Skip link-local addresses (169.254.x.x)
                        let ip = v4_addr.ip;
                        if ip.octets()[0] == 169 && ip.octets()[1] == 254 {
                            return None;
                        }
                        let is_loopback = iface.is_loopback();
                        Some(NetworkInterface {
                            name: iface.name,
                            ip: ip.to_string(),
                            is_loopback,
                        })
                    } else {
                        None
                    }
                })
                .collect()
        }
        Err(e) => {
            log::warn!("Failed to enumerate network interfaces: {}", e);
            Vec::new()
        }
    }
}

// =============================================================================
// NETWORK INTERFACE
// =============================================================================

/// Information about a network interface available for mDNS
#[derive(Debug, Clone, serde::Serialize)]
pub struct NetworkInterface {
    /// Interface name (e.g., "en0", "Wi-Fi", "Ethernet")
    pub name: String,
    /// IPv4 address of the interface
    pub ip: String,
    /// Whether this is a loopback interface
    #[serde(rename = "isLoopback")]
    pub is_loopback: bool,
}

// =============================================================================
// DISCOVERED PEER
// =============================================================================

/// Information about a discovered peer on the network
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiscoveredPeer {
    /// Unique instance identifier (UUID)
    #[serde(rename = "instanceId")]
    pub instance_id: String,
    /// Human-readable display name (e.g., "Chris's Laptop")
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    /// Auto-assigned numeric ID for easy reference (1, 2, 3...)
    #[serde(rename = "displayId")]
    pub display_id: u8,
    /// Channel name the peer belongs to
    pub channel: String,
    /// IP address of the peer
    pub host: String,
    /// OSC port of the peer
    pub port: u16,
    /// Protocol version
    pub version: String,
    /// Whether this is our own instance
    #[serde(rename = "isSelf")]
    pub is_self: bool,
    /// Config API port (bridges only)
    #[serde(rename = "configPort")]
    pub config_port: Option<u16>,
}

// =============================================================================
// DISCOVERY SERVICE
// =============================================================================

/// Manages mDNS service registration and peer discovery
pub struct DiscoveryService {
    /// The mDNS daemon
    daemon: ServiceDaemon,
    /// Our instance ID
    instance_id: String,
    /// Our display name (human-readable)
    display_name: Option<String>,
    /// Our channel name
    channel_name: String,
    /// Our OSC port
    osc_port: u16,
    /// Our assigned display ID
    our_display_id: Arc<Mutex<u8>>,
    /// Discovered peers (keyed by instance_id)
    peers: Arc<Mutex<HashMap<String, DiscoveredPeer>>>,
    /// Display ID assignments (instance_id -> display_id)
    display_id_map: Arc<Mutex<HashMap<String, u8>>>,
    /// Channel to send peer updates
    peer_tx: mpsc::Sender<Vec<DiscoveredPeer>>,
    /// Whether we're currently registered
    is_registered: bool,
    /// Our registered service name (for unregistration)
    service_fullname: Option<String>,
}

impl DiscoveryService {
    /// Create a new discovery service.
    ///
    /// ## Parameters
    ///
    /// - `instance_id` - Unique identifier for this instance (UUID)
    /// - `display_name` - Human-readable name for this instance (optional)
    /// - `channel_name` - Name of the channel to join (empty string means "show all")
    /// - `osc_port` - Port where this instance's OSC server listens
    /// - `peer_tx` - Channel to send peer list updates
    /// - `network_interface` - Network interface to advertise on (None = all interfaces)
    pub fn new(
        instance_id: String,
        display_name: Option<String>,
        channel_name: String,
        osc_port: u16,
        peer_tx: mpsc::Sender<Vec<DiscoveredPeer>>,
        network_interface: Option<String>,
    ) -> Result<Self, String> {
        let daemon = ServiceDaemon::new()
            .map_err(|e| format!("Failed to create mDNS daemon: {}", e))?;

        // Enable network interfaces for mDNS advertisement
        // By default, mdns-sd only enables loopback interfaces
        match &network_interface {
            Some(iface_name) if iface_name != "auto" => {
                // User selected a specific interface
                daemon
                    .enable_interface(IfKind::Name(iface_name.clone()))
                    .map_err(|e| format!("Failed to enable interface '{}': {}", iface_name, e))?;
                log::info!("mDNS daemon created, enabled interface: {}", iface_name);
            }
            _ => {
                // Auto mode: enable all interfaces
                daemon
                    .enable_interface(IfKind::All)
                    .map_err(|e| format!("Failed to enable all network interfaces: {}", e))?;
                log::info!("mDNS daemon created, enabled all network interfaces");
            }
        }

        // We get display_id 1 by default (first to arrive)
        let our_display_id = Arc::new(Mutex::new(1u8));
        let display_id_map = Arc::new(Mutex::new(HashMap::new()));

        // Reserve display_id 1 for ourselves
        {
            let mut map = display_id_map.lock().unwrap();
            map.insert(instance_id.clone(), 1);
        }

        Ok(Self {
            daemon,
            instance_id,
            display_name,
            channel_name,
            osc_port,
            our_display_id,
            peers: Arc::new(Mutex::new(HashMap::new())),
            display_id_map,
            peer_tx,
            is_registered: false,
            service_fullname: None,
        })
    }

    /// Assign the next available display ID
    fn assign_display_id(display_id_map: &Arc<Mutex<HashMap<String, u8>>>, instance_id: &str) -> u8 {
        let mut map = display_id_map.lock().unwrap();

        // Check if already assigned
        if let Some(&id) = map.get(instance_id) {
            return id;
        }

        // Find next available ID
        let used_ids: HashSet<u8> = map.values().cloned().collect();
        let new_id = (1..=255).find(|id| !used_ids.contains(id)).unwrap_or(255);

        map.insert(instance_id.to_string(), new_id);
        new_id
    }

    /// Register our service on the network.
    pub fn register(&mut self) -> Result<(), String> {
        if self.is_registered {
            log::debug!("Service already registered, skipping");
            return Ok(());
        }

        // Get hostname for service registration
        let host_name = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        // Format hostname for mDNS (must end with .local.)
        let mdns_hostname = format!("{}.local.", host_name);

        // Use instance_id as the service name (must be unique)
        let service_name = &self.instance_id;

        // Create TXT record properties
        // Use display_name if set, otherwise hostname as fallback
        let name_value = self
            .display_name
            .clone()
            .unwrap_or_else(|| host_name.clone());

        let properties = [
            ("channel", self.channel_name.as_str()),
            ("version", PROTOCOL_VERSION),
            ("instance", self.instance_id.as_str()),
            ("name", name_value.as_str()),
        ];

        // Get our local IP address explicitly (addr_auto doesn't work reliably on Windows)
        let local_ip = get_local_ip();
        log::info!("Local IP for mDNS registration: {:?}", local_ip);

        let service_info = ServiceInfo::new(
            SERVICE_TYPE,
            service_name,
            &mdns_hostname,
            local_ip.as_deref().unwrap_or(""),  // Explicit IP address
            self.osc_port,
            &properties[..],
        )
        .map_err(|e| format!("Failed to create service info: {}", e))?
        .enable_addr_auto();  // Still enable auto for additional interfaces

        // Store the full name for later unregistration
        self.service_fullname = Some(service_info.get_fullname().to_string());

        // Log the addresses the service will advertise
        let addrs: Vec<_> = service_info.get_addresses().iter().map(|a| a.to_string()).collect();
        log::info!("mDNS service will advertise on addresses: {:?} (addr_auto enabled)", addrs);

        self.daemon
            .register(service_info)
            .map_err(|e| format!("Failed to register mDNS service: {}", e))?;

        self.is_registered = true;

        log::info!(
            "Registered mDNS service: {} (name: {}, channel: {}, port: {}, addr_auto: true)",
            service_name,
            name_value,
            self.channel_name,
            self.osc_port
        );

        Ok(())
    }

    /// Unregister our service from the network.
    pub fn unregister(&mut self) -> Result<(), String> {
        if !self.is_registered {
            return Ok(());
        }

        if let Some(fullname) = &self.service_fullname {
            self.daemon
                .unregister(fullname)
                .map_err(|e| format!("Failed to unregister mDNS service: {}", e))?;

            log::info!("Unregistered mDNS service: {}", fullname);
        }

        self.is_registered = false;
        self.service_fullname = None;

        Ok(())
    }

    /// Start browsing for other services.
    ///
    /// Returns a task handle that processes discovery events.
    pub fn start_browsing(&self) -> Result<tokio::task::JoinHandle<()>, String> {
        let receiver = self.daemon
            .browse(SERVICE_TYPE)
            .map_err(|e| format!("Failed to start browsing: {}", e))?;

        let peers = self.peers.clone();
        let our_instance_id = self.instance_id.clone();
        let display_id_map = self.display_id_map.clone();
        let peer_tx = self.peer_tx.clone();

        let handle = tokio::spawn(async move {
            log::info!("Started mDNS browsing for {}", SERVICE_TYPE);

            loop {
                // Use recv_async for non-blocking receive in tokio context
                match receiver.recv_async().await {
                    Ok(event) => {
                        Self::handle_event(
                            event,
                            &peers,
                            &our_instance_id,
                            &display_id_map,
                            &peer_tx,
                        )
                        .await;
                    }
                    Err(e) => {
                        log::debug!("mDNS browse channel closed: {}", e);
                        break;
                    }
                }
            }

            log::info!("mDNS browsing stopped");
        });

        Ok(handle)
    }

    /// Handle an mDNS service event.
    async fn handle_event(
        event: ServiceEvent,
        peers: &Arc<Mutex<HashMap<String, DiscoveredPeer>>>,
        our_instance_id: &str,
        display_id_map: &Arc<Mutex<HashMap<String, u8>>>,
        peer_tx: &mpsc::Sender<Vec<DiscoveredPeer>>,
    ) {
        match event {
            ServiceEvent::ServiceResolved(info) => {
                // Extract peer info from TXT records
                let properties = info.get_properties();

                let instance_id = properties
                    .get_property_val_str("instance")
                    .unwrap_or_default()
                    .to_string();
                let channel = properties
                    .get_property_val_str("channel")
                    .unwrap_or_default()
                    .to_string();
                let version = properties
                    .get_property_val_str("version")
                    .unwrap_or(PROTOCOL_VERSION)
                    .to_string();
                let display_name = properties
                    .get_property_val_str("name")
                    .map(|s| s.to_string());
                let config_port = properties
                    .get_property_val_str("config_port")
                    .and_then(|s| s.parse::<u16>().ok());

                // Skip ourselves - we add ourselves separately in get_peers
                if instance_id == our_instance_id {
                    log::debug!("Ignoring our own service");
                    return;
                }

                // NOTE: We no longer filter by channel - show ALL peers on the network

                // Get the first IP address (prefer IPv4)
                let host = info
                    .get_addresses()
                    .iter()
                    .find(|scoped_ip| scoped_ip.is_ipv4())
                    .or_else(|| info.get_addresses().iter().next())
                    .map(|scoped_ip| scoped_ip.to_string())
                    .unwrap_or_default();

                if host.is_empty() {
                    log::warn!("Discovered peer {} has no addresses", instance_id);
                    return;
                }

                let port = info.get_port();

                // Assign a display ID
                let display_id = Self::assign_display_id(display_id_map, &instance_id);

                let peer = DiscoveredPeer {
                    instance_id: instance_id.clone(),
                    display_name,
                    display_id,
                    channel,
                    host,
                    port,
                    version,
                    is_self: false,
                    config_port,
                };

                log::info!(
                    "Discovered peer: #{} {} ({}) at {}:{} (channel: {})",
                    peer.display_id,
                    peer.display_name.as_deref().unwrap_or("unnamed"),
                    peer.instance_id,
                    peer.host,
                    peer.port,
                    peer.channel
                );

                // Add to peers
                {
                    let mut peers_lock = peers.lock().unwrap();
                    peers_lock.insert(instance_id, peer);
                }

                // Notify listeners
                Self::notify_peers(peers, peer_tx).await;
            }

            ServiceEvent::ServiceRemoved(_, fullname) => {
                // Extract instance_id from fullname (format: "instance_id._sher-present._udp.local.")
                let instance_id = fullname
                    .split('.')
                    .next()
                    .unwrap_or(&fullname)
                    .to_string();

                log::info!("Peer removed: {}", instance_id);

                // Remove from peers (but keep display_id assignment for stability)
                {
                    let mut peers_lock = peers.lock().unwrap();
                    peers_lock.remove(&instance_id);
                }

                // Notify listeners
                Self::notify_peers(peers, peer_tx).await;
            }

            ServiceEvent::SearchStarted(_) => {
                log::debug!("mDNS search started");
            }

            ServiceEvent::SearchStopped(_) => {
                log::debug!("mDNS search stopped");
            }

            _ => {
                // Ignore other events
            }
        }
    }

    /// Send current peer list to listeners.
    async fn notify_peers(
        peers: &Arc<Mutex<HashMap<String, DiscoveredPeer>>>,
        peer_tx: &mpsc::Sender<Vec<DiscoveredPeer>>,
    ) {
        let peer_list: Vec<DiscoveredPeer> = {
            let peers_lock = peers.lock().unwrap();
            peers_lock.values().cloned().collect()
        };

        if let Err(e) = peer_tx.send(peer_list).await {
            log::warn!("Failed to send peer update: {}", e);
        }
    }

    /// Get the current list of discovered peers, including ourselves.
    pub fn get_peers(&self) -> Vec<DiscoveredPeer> {
        let mut peer_list: Vec<DiscoveredPeer> = {
            let peers_lock = self.peers.lock().unwrap();
            peers_lock.values().cloned().collect()
        };

        // Add ourselves to the list
        let our_display_id = *self.our_display_id.lock().unwrap();
        let our_peer = DiscoveredPeer {
            instance_id: self.instance_id.clone(),
            display_name: self.display_name.clone().or_else(|| {
                hostname::get()
                    .map(|h| h.to_string_lossy().to_string())
                    .ok()
            }),
            display_id: our_display_id,
            channel: self.channel_name.clone(),
            host: get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string()),
            port: self.osc_port,
            version: PROTOCOL_VERSION.to_string(),
            is_self: true,
            config_port: None,
        };
        peer_list.push(our_peer);

        // Sort by display_id for consistent ordering
        peer_list.sort_by_key(|p| p.display_id);

        peer_list
    }

    /// Update the display name and re-register to broadcast it.
    pub fn update_display_name(&mut self, new_name: Option<String>) -> Result<(), String> {
        if self.display_name == new_name {
            return Ok(());
        }

        self.display_name = new_name;

        // Re-register with new name if we were registered
        if self.is_registered {
            self.unregister()?;
            self.register()?;
        }

        Ok(())
    }

    /// Update the channel name and re-register.
    #[allow(dead_code)]
    pub fn update_channel(&mut self, new_channel: String) -> Result<(), String> {
        if self.channel_name == new_channel {
            return Ok(());
        }

        self.channel_name = new_channel;

        // Re-register with new channel if we were registered
        if self.is_registered {
            self.unregister()?;
            self.register()?;
        }

        // Note: We no longer clear peers since we show all peers regardless of channel

        Ok(())
    }

    /// Shutdown the discovery service.
    pub fn shutdown(&mut self) -> Result<(), String> {
        self.unregister()?;
        self.daemon
            .shutdown()
            .map_err(|e| format!("Failed to shutdown mDNS daemon: {}", e))?;
        Ok(())
    }
}

impl Drop for DiscoveryService {
    fn drop(&mut self) {
        if let Err(e) = self.shutdown() {
            log::warn!("Error during discovery service shutdown: {}", e);
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
    fn test_service_type() {
        assert!(SERVICE_TYPE.ends_with(".local."));
        assert!(SERVICE_TYPE.contains("_sher-present"));
    }
}
