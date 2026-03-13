use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeStatus {
    pub running: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeGlobalConfig {
    pub mode: String,
    pub broadcast_port: u16,
    pub feedback_port: u16,
    pub log_level: String,
    #[serde(default)]
    pub bridge_id: Option<String>,
    #[serde(default)]
    pub bridge_name: Option<String>,
    pub satellite: SatelliteConfig,
    pub valid_channels: Vec<String>,
    pub valid_modes: Vec<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SatelliteConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeRegisteredDevices {
    pub devices: HashMap<String, Option<BridgeDeviceSlot>>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub log_level: Option<String>,
    #[serde(default)]
    pub feedback_port: Option<u16>,
    #[serde(default)]
    pub broadcast_port: Option<u16>,
    #[serde(default)]
    pub bridge_id: Option<String>,
    #[serde(default)]
    pub bridge_name: Option<String>,
    #[serde(default)]
    pub valid_channels: Option<Vec<String>>,
    #[serde(default)]
    pub valid_modes: Option<Vec<String>>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeDeviceSlot {
    pub label: String,
    pub usb_phys: String,
    pub channel: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeConnectedDevices {
    pub devices: Vec<ConnectedDevice>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectedDevice {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub usb_phys: Option<String>,
    #[serde(default)]
    pub is_perfect_cue: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeRegistrationStatus {
    pub active: bool,
    pub target_slot: Option<String>,
    pub detected_phys: Option<String>,
    pub detected_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeSatelliteStatus {
    pub connected: bool,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub companion_version: Option<String>,
    pub api_version: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeFeedback {
    #[serde(default)]
    pub presenting: bool,
    #[serde(default)]
    pub open: bool,
    #[serde(default)]
    pub current_slide: u32,
    #[serde(default)]
    pub total_slides: u32,
    #[serde(default)]
    pub zoom_level: u32,
    #[serde(default)]
    pub last_updated: Option<String>,
    #[serde(default)]
    pub last_command: Option<String>,
    #[serde(default)]
    pub last_command_time: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeApiResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeLogs {
    pub logs: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SaveGlobalConfigRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    pub broadcast_port: u16,
    pub feedback_port: u16,
    pub log_level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bridge_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub satellite: Option<SatelliteConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StartRegistrationRequest {
    pub slot: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfirmRegistrationRequest {
    pub slot: String,
    pub usb_phys: String,
    pub channel: String,
    pub label: String,
}
