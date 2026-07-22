//! # HTTP API Handlers
//!
//! Each handler is a plain axum function that reads from the shared
//! [`CoreState`] and returns JSON.

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::config::{BridgeConfig, DeviceConfig, DeviceTarget};
use crate::osc::feedback::FeedbackState;
use crate::state::ApiState;

use sherpresent_core::DiscoveredPeer;

/// Top-level API error response.
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub success: bool,
    pub message: String,
}

impl ApiError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
        }
    }
}

/// Standard success response.
#[derive(Debug, Serialize)]
pub struct ApiOk {
    pub success: bool,
    pub message: String,
}

impl ApiOk {
    fn new(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
        }
    }
}

// =============================================================================
// Status
// =============================================================================

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    running: bool,
    bridge_name: String,
    bridge_id: String,
    mode: String,
    feedback_port: u16,
    config_port: u16,
}

pub async fn get_status(State(state): State<Arc<ApiState>>) -> Json<StatusResponse> {
    let cfg = state.config.lock().unwrap();
    Json(StatusResponse {
        running: true,
        bridge_name: cfg.bridge_name.clone(),
        bridge_id: cfg.bridge_id.to_string(),
        mode: match cfg.mode {
            crate::config::BridgeMode::Direct => "direct".to_string(),
            crate::config::BridgeMode::Satellite => "satellite".to_string(),
        },
        feedback_port: cfg.feedback_port,
        config_port: cfg.config_port,
    })
}

// =============================================================================
// Peers
// =============================================================================

#[derive(Debug, Serialize)]
pub struct PeersResponse {
    peers: Vec<DiscoveredPeer>,
    updated_at: String,
}

pub async fn get_peers(State(state): State<Arc<ApiState>>) -> Json<PeersResponse> {
    let peers = {
        let discovery = state.discovery_service.lock().unwrap();
        match discovery.as_ref() {
            Some(s) => s.get_peers(),
            None => Vec::new(),
        }
    };
    Json(PeersResponse {
        peers,
        updated_at: chrono::Utc::now().to_rfc3339(),
    })
}

// =============================================================================
// Feedback
// =============================================================================

pub async fn get_feedback(State(state): State<Arc<ApiState>>) -> Json<FeedbackState> {
    Json(state.feedback_state.lock().unwrap().clone())
}

// =============================================================================
// Config
// =============================================================================

pub async fn get_global_config(State(state): State<Arc<ApiState>>) -> Json<BridgeConfig> {
    Json(state.config.lock().unwrap().clone())
}

#[derive(Debug, Deserialize)]
pub struct SaveConfigPayload {
    pub mode: Option<String>,
    pub feedback_port: Option<u16>,
    pub config_port: Option<u16>,
    pub log_level: Option<String>,
    pub bridge_name: Option<String>,
}

pub async fn save_global_config(
    State(state): State<Arc<ApiState>>,
    Json(payload): Json<SaveConfigPayload>,
) -> Result<Json<ApiOk>, (axum::http::StatusCode, Json<ApiError>)> {
    {
        let mut cfg = state.config.lock().unwrap();
        if let Some(m) = payload.mode {
            cfg.mode = match m.as_str() {
                "direct" => crate::config::BridgeMode::Direct,
                "satellite" => crate::config::BridgeMode::Satellite,
                _ => {
                    return Err((
                        axum::http::StatusCode::BAD_REQUEST,
                        Json(ApiError::new("invalid mode")),
                    ));
                }
            };
        }
        if let Some(p) = payload.feedback_port {
            cfg.feedback_port = p;
        }
        if let Some(p) = payload.config_port {
            cfg.config_port = p;
        }
        if let Some(l) = payload.log_level {
            cfg.log_level = l;
        }
        if let Some(n) = payload.bridge_name {
            cfg.bridge_name = n;
        }
    }
    Ok(Json(ApiOk::new("Config saved")))
}

// =============================================================================
// Registered devices
// =============================================================================

#[derive(Debug, Serialize)]
pub struct RegisteredDevicesResponse {
    devices: std::collections::BTreeMap<String, Option<DeviceConfig>>,
    mode: String,
    log_level: String,
    feedback_port: u16,
    config_port: u16,
    bridge_id: String,
    bridge_name: String,
}

pub async fn get_registered_devices(
    State(state): State<Arc<ApiState>>,
) -> Json<RegisteredDevicesResponse> {
    let cfg = state.config.lock().unwrap();
    Json(RegisteredDevicesResponse {
        devices: cfg.devices.clone(),
        mode: match cfg.mode {
            crate::config::BridgeMode::Direct => "direct".to_string(),
            crate::config::BridgeMode::Satellite => "satellite".to_string(),
        },
        log_level: cfg.log_level.clone(),
        feedback_port: cfg.feedback_port,
        config_port: cfg.config_port,
        bridge_id: cfg.bridge_id.to_string(),
        bridge_name: cfg.bridge_name.clone(),
    })
}

// =============================================================================
// Device target assignment
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct SetTargetPayload {
    target: Option<DeviceTarget>,
}

pub async fn set_device_target(
    State(state): State<Arc<ApiState>>,
    Path(slot): Path<String>,
    Json(payload): Json<SetTargetPayload>,
) -> Result<Json<ApiOk>, (axum::http::StatusCode, Json<ApiError>)> {
    let mut cfg = state.config.lock().unwrap();
    let entry = cfg.devices.get_mut(&slot).ok_or_else(|| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(ApiError::new(format!("invalid slot {slot}"))),
        )
    })?;

    if let Some(device) = entry.as_mut() {
        device.target = payload.target;
    } else {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            Json(ApiError::new(format!("slot {slot} is empty"))),
        ));
    }

    Ok(Json(ApiOk::new(format!("Target updated for {slot}"))))
}

// =============================================================================
// Static HTML fallback
// =============================================================================

pub async fn html_status_page() -> axum::response::Html<&'static str> {
    axum::response::Html(STATUS_HTML)
}

const STATUS_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>SherPresent Bridge</title>
<style>
  body { font-family: system-ui, sans-serif; background: #0f1115; color: #e6e9ef; margin: 0; padding: 32px; }
  h1 { font-size: 1.5rem; }
  .card { background: #181b22; border: 1px solid #283042; border-radius: 8px; padding: 16px 20px; max-width: 640px; margin: 16px 0; }
  .mono { font-family: ui-monospace, SFMono-Regular, Consolas, monospace; }
  .accent { color: #4ade80; }
  .muted { color: #8b94a3; }
  a { color: #4ade80; }
</style>
</head>
<body>
  <h1>SherPresent Bridge</h1>
  <div class="card">
    <p>The bridge is running.</p>
    <p class="muted">API endpoints exposed here:</p>
    <ul>
      <li><span class="mono">GET /status</span></li>
      <li><span class="mono">GET /peers</span></li>
      <li><span class="mono">GET /feedback</span></li>
      <li><span class="mono">GET /config/global</span></li>
      <li><span class="mono">POST /config/global</span></li>
      <li><span class="mono">GET /devices/registered</span></li>
      <li><span class="mono">POST /devices/{slot}/target</span></li>
    </ul>
  </div>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn status_html_is_valid() {
        assert!(STATUS_HTML.contains("SherPresent Bridge"));
        assert!(STATUS_HTML.contains("GET /status"));
    }

    #[test]
    fn save_config_payload_deserializes() {
        let v: SaveConfigPayload = serde_json::from_value(json!({
            "mode": "direct",
            "feedback_port": 9001,
            "log_level": "INFO"
        }))
        .unwrap();
        assert_eq!(v.mode, Some("direct".to_string()));
        assert_eq!(v.feedback_port, Some(9001));
        assert_eq!(v.log_level, Some("INFO".to_string()));
    }
}
