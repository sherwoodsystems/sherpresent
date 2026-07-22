//! # Bridge Configuration
//!
//! On-disk configuration matching the Python bridge's v5 schema.
//!
//! Config is stored in the platform config directory as `config.json`:
//!
//! - Linux: `~/.config/systems.sherwood.presenter-bridge/config.json`
//! - macOS: `~/Library/Application Support/systems.sherwood.presenter-bridge/config.json`
//! - Windows: `%APPDATA%\systems.sherwood.presenter-bridge\config.json`
//!
//! A custom config directory can be supplied for headless/service deployments.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Current config schema version. Matches the Python bridge's v5.
pub const CONFIG_VERSION: u32 = 5;

/// Default OSC feedback listener port (where the bridge receives
/// `/oscpoint/slideshow/*` etc. from desktops).
pub const DEFAULT_FEEDBACK_PORT: u16 = 9001;

/// Default HTTP config API port (where the desktop deep-links to the bridge's
/// web config UI). The Python bridge used port 80; we use 8080 to avoid root
/// requirements and conflicts on shared hosts (laptops, sticks).
pub const DEFAULT_CONFIG_PORT: u16 = 8080;

/// Default Companion Satellite TCP port.
pub const DEFAULT_SATELLITE_PORT: u16 = 16622;

/// Operating mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BridgeMode {
    Direct,
    Satellite,
}

impl Default for BridgeMode {
    fn default() -> Self {
        BridgeMode::Direct
    }
}

/// A device target — where a USB clicker's keypresses get sent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTarget {
    pub host: String,
    pub port: u16,
    /// Human-readable name shown in the web UI (e.g., "MacBook Pro").
    #[serde(default)]
    pub name: Option<String>,
    /// Instance ID of the peer (matches DiscoveredPeer.instanceId).
    #[serde(rename = "instance_id", default)]
    pub instance_id: Option<String>,
}

/// The presentation function a bound key triggers.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum KeyAction {
    Next,
    Prev,
}

/// A registered USB device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    /// User-visible label, e.g. "Perfect Cue Micro".
    pub label: String,
    /// USB physical path — the unique stable identifier for hot-plugging.
    pub usb_phys: String,
    /// Optional OSC target; `None` means the device is registered but untargeted.
    #[serde(default)]
    pub target: Option<DeviceTarget>,
    /// Per-key action bindings, keyed by evdev key name (e.g. `"KEY_RIGHT"`).
    #[serde(default)]
    pub bindings: BTreeMap<String, KeyAction>,
}

/// Companion Satellite connection config (satellite mode).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SatelliteConfig {
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default = "default_satellite_port")]
    pub port: u16,
}

fn default_satellite_port() -> u16 {
    DEFAULT_SATELLITE_PORT
}

impl Default for SatelliteConfig {
    fn default() -> Self {
        Self {
            host: None,
            port: DEFAULT_SATELLITE_PORT,
        }
    }
}

/// Top-level bridge config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    #[serde(default = "default_config_version")]
    pub version: u32,
    /// `"direct"` or `"satellite"`.
    #[serde(default)]
    pub mode: BridgeMode,
    /// UDP port where the bridge's OSC feedback listener runs.
    #[serde(default = "default_feedback_port", rename = "feedback_port")]
    pub feedback_port: u16,
    /// TCP port where the bridge's HTTP config API runs.
    #[serde(default = "default_config_port", rename = "config_port")]
    pub config_port: u16,
    /// Logging level: `"DEBUG"`, `"INFO"`, `"WARNING"`, `"ERROR"`.
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// Unique bridge UUID; auto-generated on first run.
    #[serde(default = "Uuid::new_v4", rename = "bridge_id", serialize_with = "serialize_uuid")]
    pub bridge_id: Uuid,
    /// Human-readable bridge name; auto-generated on first run if absent.
    #[serde(default = "auto_bridge_name", rename = "bridge_name")]
    pub bridge_name: String,
    /// Companion Satellite config (satellite mode).
    #[serde(default)]
    pub satellite: SatelliteConfig,
    /// Per-slot device registration.
    #[serde(default = "default_device_slots")]
    pub devices: BTreeMap<String, Option<DeviceConfig>>,
}

fn default_config_version() -> u32 {
    CONFIG_VERSION
}
fn default_feedback_port() -> u16 {
    DEFAULT_FEEDBACK_PORT
}
fn default_config_port() -> u16 {
    DEFAULT_CONFIG_PORT
}
fn default_log_level() -> String {
    "INFO".to_string()
}
fn auto_bridge_name() -> String {
    let id = Uuid::new_v4();
    format!("Bridge {}", &id.to_string()[..6])
}

fn default_device_slots() -> BTreeMap<String, Option<DeviceConfig>> {
    BTreeMap::new()
}

/// `serde_with` helper to serialize the UUID as a string.
fn serialize_uuid<S: serde::Serializer>(uuid: &Uuid, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&uuid.to_string())
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            mode: BridgeMode::Direct,
            feedback_port: DEFAULT_FEEDBACK_PORT,
            config_port: DEFAULT_CONFIG_PORT,
            log_level: default_log_level(),
            bridge_id: Uuid::new_v4(),
            bridge_name: auto_bridge_name(),
            satellite: SatelliteConfig {
                host: None,
                port: DEFAULT_SATELLITE_PORT,
            },
            devices: default_device_slots(),
        }
    }
}

impl BridgeConfig {
    /// Generate a fresh default bridge_name ("Bridge abc123") based on the
    /// current bridge_id.
    pub fn refresh_default_name(&mut self) {
        self.bridge_name = format!("Bridge {}", &self.bridge_id.to_string()[..6]);
    }
}

/// Resolve the bridge config directory.
///
/// If `config_dir` is supplied, use it directly. Otherwise fall back to the
/// platform config dir under `systems.sherwood.presenter-bridge`.
pub fn resolve_config_dir(config_dir: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(dir) = config_dir {
        return Ok(dir.to_path_buf());
    }
    dirs::config_dir()
        .map(|d| d.join("systems.sherwood.presenter-bridge"))
        .ok_or_else(|| "Could not determine config directory".to_string())
}

/// Get the path to the bridge config file.
pub fn get_config_path(config_dir: Option<&Path>) -> Result<PathBuf, String> {
    Ok(resolve_config_dir(config_dir)?.join("config.json"))
}

/// Load the bridge config from disk, or create a default if missing.
pub fn load_config(config_dir: Option<&Path>) -> Result<BridgeConfig, String> {
    let path = get_config_path(config_dir)?;

    if !path.exists() {
        let mut config = BridgeConfig::default();
        config.refresh_default_name();
        save_config(config_dir, &config)?;
        log::info!(
            "Created default bridge config at {} (id={}, name={})",
            path.display(),
            config.bridge_id,
            config.bridge_name
        );
        return Ok(config);
    }

    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read config file: {}", e))?;

    let mut config: BridgeConfig = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse config: {}", e))?;

    // Backfill bridge_id / bridge_name for older configs that may not have them.
    let dirty = (config.bridge_name.is_empty() || config.bridge_name == "Bridge ")
        || config.bridge_id == Uuid::nil();
    if dirty {
        if config.bridge_id == Uuid::nil() {
            config.bridge_id = Uuid::new_v4();
        }
        config.refresh_default_name();
        save_config(config_dir, &config)?;
        log::info!(
            "Backfilled bridge identity (id={}, name={})",
            config.bridge_id,
            config.bridge_name
        );
    }

    Ok(config)
}

/// Persist the bridge config to disk.
pub fn save_config(config_dir: Option<&Path>, config: &BridgeConfig) -> Result<(), String> {
    let path = get_config_path(config_dir)?;
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
    fn default_config_serializes() {
        let config = BridgeConfig::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        assert!(json.contains("\"version\": 5"));
        assert!(json.contains("\"mode\": \"direct\""));
        assert!(json.contains("\"feedback_port\": 9001"));
        assert!(json.contains("\"config_port\": 8080"));
        assert!(config.devices.is_empty());
    }

    #[test]
    fn default_config_round_trips() {
        let config = BridgeConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let restored: BridgeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.feedback_port, config.feedback_port);
        assert_eq!(restored.config_port, config.config_port);
        assert_eq!(restored.bridge_id, config.bridge_id);
        assert!(restored.devices.is_empty());
    }

    #[test]
    fn empty_name_backfilled() {
        let mut config = BridgeConfig::default();
        config.bridge_name = String::new();
        config.bridge_id = Uuid::nil();
        config.refresh_default_name();
        assert!(config.bridge_name.starts_with("Bridge "));
        assert!(config.bridge_name.len() > "Bridge ".len());
    }
}
