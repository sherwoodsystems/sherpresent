use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;
use uuid::Uuid;

// =============================================================================
// FEEDBACK DESTINATION
// =============================================================================

/// A single feedback destination (host:port pair)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeedbackDestination {
    pub host: String,
    pub port: u16,
}

impl FeedbackDestination {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    /// Convert to socket address string
    pub fn to_addr_string(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

// =============================================================================
// OSC CONFIG
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OscConfig {
    #[serde(rename = "receivePort")]
    pub receive_port: u16,
    /// Legacy single feedback port (for backward compatibility)
    #[serde(rename = "feedbackPort")]
    pub feedback_port: u16,
    /// Legacy single feedback host (for backward compatibility)
    #[serde(rename = "feedbackHost")]
    pub feedback_host: String,
    pub host: String,
    /// Multiple feedback destinations (includes legacy host:port if set)
    #[serde(rename = "feedbackDestinations", default)]
    pub feedback_destinations: Vec<FeedbackDestination>,
}

impl OscConfig {
    /// Get all feedback destinations, including legacy single destination
    pub fn get_all_destinations(&self) -> Vec<FeedbackDestination> {
        let mut destinations = self.feedback_destinations.clone();

        // Include legacy destination if it's not already in the list
        let legacy = FeedbackDestination::new(&self.feedback_host, self.feedback_port);
        if !destinations.contains(&legacy) {
            destinations.insert(0, legacy);
        }

        destinations
    }
}

impl Default for OscConfig {
    fn default() -> Self {
        Self {
            receive_port: 9000,
            feedback_port: 9001,
            feedback_host: "127.0.0.1".to_string(),
            host: "0.0.0.0".to_string(),
            feedback_destinations: Vec::new(),
        }
    }
}

// =============================================================================
// CHANNEL CONFIG
// =============================================================================

/// Valid channel names for broadcast mode
pub const VALID_CHANNELS: &[&str] = &[
    "main", "backup",
    "keynote1", "keynote2", "keynote3", "keynote4", "keynote5",
    "keynote6", "keynote7", "keynote8", "keynote9",
    "aux1", "aux2", "aux3", "aux4", "aux5",
    "aux6", "aux7", "aux8", "aux9",
];

/// Default broadcast port for channel communication
pub const DEFAULT_BROADCAST_PORT: u16 = 9002;

/// Configuration for peer-to-peer channel synchronization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    /// Whether channel sync is enabled
    pub enabled: bool,
    /// Name of the channel to join (e.g., "main", "backup", "keynote1")
    #[serde(rename = "channelName")]
    pub channel_name: String,
    /// Unique instance identifier (auto-generated UUID)
    #[serde(rename = "instanceId")]
    pub instance_id: String,
    /// Human-readable display name for this instance (e.g., "Chris's Laptop")
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    /// Whether to use UDP broadcast mode (vs direct IP)
    #[serde(rename = "broadcastMode", default)]
    pub broadcast_mode: bool,
    /// Port for broadcast communication (default: 9002)
    #[serde(rename = "broadcastPort", default = "default_broadcast_port")]
    pub broadcast_port: u16,
    /// Network interface to advertise mDNS on (None = "auto" = all interfaces)
    #[serde(rename = "networkInterface", default)]
    pub network_interface: Option<String>,
}

fn default_broadcast_port() -> u16 {
    DEFAULT_BROADCAST_PORT
}

impl ChannelConfig {
    /// Validate that a channel name is one of the allowed values
    pub fn is_valid_channel_name(name: &str) -> bool {
        VALID_CHANNELS.contains(&name)
    }

    /// Get the OSC address prefix for this channel
    #[allow(dead_code)]
    pub fn osc_prefix(&self) -> String {
        format!("/clicker/{}", self.channel_name)
    }
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            channel_name: "main".to_string(),
            instance_id: Uuid::new_v4().to_string(),
            display_name: None,
            broadcast_mode: false,
            broadcast_port: DEFAULT_BROADCAST_PORT,
            network_interface: None, // Auto (all interfaces)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub enabled: bool,
    pub verbose: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            verbose: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub osc: OscConfig,
    pub adapter: String,
    #[serde(rename = "presentationName")]
    pub presentation_name: String,
    pub logging: LoggingConfig,
    /// Channel synchronization settings
    #[serde(default)]
    pub channel: ChannelConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        // Default to libreoffice on non-macOS platforms, powerpoint on macOS
        #[cfg(target_os = "macos")]
        let default_adapter = "powerpoint".to_string();
        #[cfg(not(target_os = "macos"))]
        let default_adapter = "libreoffice".to_string();

        Self {
            osc: OscConfig::default(),
            adapter: default_adapter,
            presentation_name: String::new(),
            logging: LoggingConfig::default(),
            channel: ChannelConfig::default(),
        }
    }
}

/// Get the path to the config file
pub fn get_config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Failed to get config directory: {}", e))?;

    Ok(config_dir.join("config.json"))
}

/// Load configuration from disk
pub fn load_config(app: &tauri::AppHandle) -> Result<AppConfig, String> {
    let path = get_config_path(app)?;

    if !path.exists() {
        return Ok(AppConfig::default());
    }

    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read config file: {}", e))?;

    serde_json::from_str(&content).map_err(|e| format!("Failed to parse config: {}", e))
}

/// Save configuration to disk
pub fn save_config(app: &tauri::AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = get_config_path(app)?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }

    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    fs::write(&path, content).map_err(|e| format!("Failed to write config file: {}", e))
}
