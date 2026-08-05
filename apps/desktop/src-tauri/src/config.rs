use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;
use uuid::Uuid;

// =============================================================================
// ADAPTER CONFIG
// =============================================================================

/// Per-adapter network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AdapterConfig {
    #[serde(rename = "libreoffice")]
    LibreOffice { host: String, port: u16 },
    #[serde(rename = "canva")]
    Canva { url: String },
    #[serde(rename = "none")]
    None,
}

impl Default for AdapterConfig {
    fn default() -> Self {
        AdapterConfig::None
    }
}

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
// DISCOVERY CONFIG
// =============================================================================

/// Configuration for mDNS peer discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Whether discovery is enabled
    pub enabled: bool,
    /// Unique instance identifier (auto-generated UUID)
    #[serde(rename = "instanceId")]
    pub instance_id: String,
    /// Human-readable display name for this instance (e.g., "Chris's Laptop")
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    /// Network interface to advertise mDNS on (None = "auto" = all interfaces)
    #[serde(rename = "networkInterface", default)]
    pub network_interface: Option<String>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            instance_id: Uuid::new_v4().to_string(),
            display_name: None,
            network_interface: None,
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

// =============================================================================
// WEB SERVER CONFIG
// =============================================================================

/// Configuration for the LAN-accessible web server (notes + timer view)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebServerConfig {
    /// Port to serve on (default: 8080)
    pub port: u16,
    /// Ontime server host (IP or hostname)
    #[serde(rename = "ontimeHost")]
    pub ontime_host: String,
    /// Ontime server port (default: 4001)
    #[serde(rename = "ontimePort")]
    pub ontime_port: u16,
    /// Font size in px for stage view notes (default: 32)
    #[serde(rename = "fontSize", default = "default_font_size")]
    pub font_size: u16,
}

fn default_font_size() -> u16 {
    32
}

impl Default for WebServerConfig {
    fn default() -> Self {
        Self {
            port: 8080,
            ontime_host: String::new(),
            ontime_port: 4001,
            font_size: default_font_size(),
        }
    }
}

/// API keys for live caption providers.
///
/// Stored in plaintext alongside the rest of the config. Fine for a
/// single-operator production machine; do not sync this file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CaptionApiKeys {
    #[serde(default)]
    pub gemini: String,
    #[serde(default)]
    pub openai: String,
}

/// Live caption / translation settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptionsConfig {
    /// Auto-start captions when the app launches
    #[serde(default)]
    pub enabled: bool,
    /// Provider id: "gemini" (phase 1) or "openai" (phase 2)
    #[serde(default = "default_caption_provider")]
    pub provider: String,
    /// cpal input device name. `None` = system default input.
    #[serde(rename = "inputDevice", default)]
    pub input_device: Option<String>,
    /// BCP-47 source language. `None` = let the provider auto-detect.
    #[serde(rename = "sourceLanguage", default)]
    pub source_language: Option<String>,
    /// BCP-47 target language for the caption output
    #[serde(rename = "targetLanguage", default = "default_target_language")]
    pub target_language: String,
    /// Caption font size in px, relative to a 1080p frame
    #[serde(rename = "fontSize", default = "default_caption_font_size")]
    pub font_size: u16,
    /// How many finalized lines to keep on screen
    #[serde(rename = "maxLines", default = "default_caption_max_lines")]
    pub max_lines: u8,
    /// Overlay background. Hex colour, or "transparent" for OBS browser sources.
    #[serde(rename = "chromaColor", default = "default_chroma_color")]
    pub chroma_color: String,
    #[serde(rename = "apiKeys", default)]
    pub api_keys: CaptionApiKeys,
}

/// Apple on-device when this Mac can actually run it (macOS 26+, Apple
/// Silicon); Gemini otherwise, as the cross-platform fallback.
fn default_caption_provider() -> String {
    if crate::captions::provider::apple::is_platform_supported() {
        "apple".to_string()
    } else {
        "gemini".to_string()
    }
}

fn default_target_language() -> String {
    "fr".to_string()
}

fn default_caption_font_size() -> u16 {
    56
}

fn default_caption_max_lines() -> u8 {
    2
}

fn default_chroma_color() -> String {
    // Broadcast chroma green
    "#00B140".to_string()
}

impl Default for CaptionsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_caption_provider(),
            input_device: None,
            source_language: None,
            target_language: default_target_language(),
            font_size: default_caption_font_size(),
            max_lines: default_caption_max_lines(),
            chroma_color: default_chroma_color(),
            api_keys: CaptionApiKeys::default(),
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
    /// Peer discovery settings
    #[serde(default, alias = "channel")]
    pub discovery: DiscoveryConfig,
    /// Per-adapter network configuration
    #[serde(rename = "adapterConfig", default)]
    pub adapter_config: AdapterConfig,
    /// LAN web server settings
    #[serde(rename = "webServer", default)]
    pub web_server: WebServerConfig,
    /// Live caption / translation settings
    #[serde(default)]
    pub captions: CaptionsConfig,
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
            discovery: DiscoveryConfig::default(),
            adapter_config: AdapterConfig::default(),
            web_server: WebServerConfig::default(),
            captions: CaptionsConfig::default(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_config_default_is_none() {
        let config = AdapterConfig::default();
        assert!(matches!(config, AdapterConfig::None));
    }

    #[test]
    fn test_adapter_config_libreoffice_serialization() {
        let config = AdapterConfig::LibreOffice {
            host: "10.0.0.5".to_string(),
            port: 2002,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"type\":\"libreoffice\""));
        assert!(json.contains("\"host\":\"10.0.0.5\""));
        assert!(json.contains("\"port\":2002"));
    }

    #[test]
    fn test_adapter_config_libreoffice_deserialization() {
        let json = r#"{"type":"libreoffice","host":"192.168.1.50","port":1599}"#;
        let config: AdapterConfig = serde_json::from_str(json).unwrap();
        match config {
            AdapterConfig::LibreOffice { host, port } => {
                assert_eq!(host, "192.168.1.50");
                assert_eq!(port, 1599);
            }
            _ => panic!("Expected LibreOffice variant"),
        }
    }

    #[test]
    fn test_adapter_config_canva_serialization() {
        let config = AdapterConfig::Canva {
            url: "https://www.canva.com/design/remote?id2=abc123".to_string(),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"type\":\"canva\""));
        assert!(json.contains("\"url\":\"https://www.canva.com/design/remote?id2=abc123\""));
    }

    #[test]
    fn test_adapter_config_canva_deserialization() {
        let json = r#"{"type":"canva","url":"https://example.com/remote?id2=test"}"#;
        let config: AdapterConfig = serde_json::from_str(json).unwrap();
        match config {
            AdapterConfig::Canva { url } => {
                assert_eq!(url, "https://example.com/remote?id2=test");
            }
            _ => panic!("Expected Canva variant"),
        }
    }

    #[test]
    fn test_adapter_config_none_serialization() {
        let config = AdapterConfig::None;
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"type\":\"none\""));
    }

    #[test]
    fn test_adapter_config_none_deserialization() {
        let json = r#"{"type":"none"}"#;
        let config: AdapterConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(config, AdapterConfig::None));
    }

    #[test]
    fn test_app_config_default_has_adapter_config_none() {
        let config = AppConfig::default();
        assert!(matches!(config.adapter_config, AdapterConfig::None));
    }

    #[test]
    fn test_app_config_roundtrip_with_adapter_config() {
        let mut config = AppConfig::default();
        config.adapter_config = AdapterConfig::LibreOffice {
            host: "10.0.0.1".to_string(),
            port: 1599,
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: AppConfig = serde_json::from_str(&json).unwrap();
        match restored.adapter_config {
            AdapterConfig::LibreOffice { host, port } => {
                assert_eq!(host, "10.0.0.1");
                assert_eq!(port, 1599);
            }
            _ => panic!("Expected LibreOffice variant after roundtrip"),
        }
    }

    #[test]
    fn test_app_config_missing_adapter_config_defaults_to_none() {
        // Simulate a config JSON from before adapter_config was added
        let json = r#"{
            "osc": {"receivePort": 9000, "feedbackPort": 9001, "feedbackHost": "127.0.0.1", "host": "0.0.0.0", "feedbackDestinations": []},
            "adapter": "libreoffice",
            "presentationName": "",
            "logging": {"enabled": true, "verbose": false}
        }"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(config.adapter_config, AdapterConfig::None));
    }

    #[test]
    fn test_captions_config_defaults() {
        let c = CaptionsConfig::default();
        assert!(!c.enabled);
        let expected_provider = if crate::captions::provider::apple::is_platform_supported() {
            "apple"
        } else {
            "gemini"
        };
        assert_eq!(c.provider, expected_provider);
        assert_eq!(c.target_language, "fr");
        assert_eq!(c.max_lines, 2);
        assert!(c.input_device.is_none());
        assert!(c.source_language.is_none());
        assert!(c.api_keys.gemini.is_empty());
    }

    #[test]
    fn test_app_config_missing_captions_defaults() {
        // Config JSON written before captions existed must still load.
        let json = r#"{
            "osc": {"receivePort": 9000, "feedbackPort": 9001, "feedbackHost": "127.0.0.1", "host": "0.0.0.0", "feedbackDestinations": []},
            "adapter": "libreoffice",
            "presentationName": "",
            "logging": {"enabled": true, "verbose": false}
        }"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert!(!config.captions.enabled);
        assert_eq!(config.captions.target_language, "fr");
        assert_eq!(config.captions.chroma_color, "#00B140");
    }

    #[test]
    fn test_app_config_roundtrip_with_captions() {
        let mut config = AppConfig::default();
        config.captions.enabled = true;
        config.captions.input_device = Some("Scarlett 2i2 USB".to_string());
        config.captions.source_language = Some("en".to_string());
        config.captions.target_language = "es".to_string();
        config.captions.api_keys.gemini = "test-key".to_string();

        let json = serde_json::to_string(&config).unwrap();
        // camelCase over the wire, matching the rest of AppConfig
        assert!(json.contains("\"inputDevice\":\"Scarlett 2i2 USB\""));
        assert!(json.contains("\"targetLanguage\":\"es\""));

        let restored: AppConfig = serde_json::from_str(&json).unwrap();
        assert!(restored.captions.enabled);
        assert_eq!(
            restored.captions.input_device.as_deref(),
            Some("Scarlett 2i2 USB")
        );
        assert_eq!(restored.captions.source_language.as_deref(), Some("en"));
        assert_eq!(restored.captions.target_language, "es");
        assert_eq!(restored.captions.api_keys.gemini, "test-key");
    }

    #[test]
    fn test_captions_partial_json_fills_defaults() {
        // A captions block written by an older build that only knew some fields.
        let json = r#"{"enabled": true, "provider": "openai"}"#;
        let c: CaptionsConfig = serde_json::from_str(json).unwrap();
        assert!(c.enabled);
        assert_eq!(c.provider, "openai");
        assert_eq!(c.target_language, "fr");
        assert_eq!(c.font_size, 56);
    }
}
