//! # App-level commands
//!
//! Self-information about the bridge for the frontend: name, id, ports, LAN IP.
//! Mirrors a subset of the Python bridge's `/status` + `/config/global`
//! endpoints but lives natively in the Tauri webview (no HTTP hop).

use serde::Serialize;
use sherpresent_core::get_local_ip;
use crate::state::BridgeState;

/// Compact self-description used by the bridge's own UI.
#[derive(Debug, Serialize)]
pub struct BridgeInfo {
    pub bridge_id: String,
    pub bridge_name: String,
    pub feedback_port: u16,
    pub config_port: u16,
    pub mode: String,
    pub log_level: String,
    pub lan_ip: Option<String>,
    /// URL where the desktop app's "Open Config Page" button redirects users
    /// (i.e. `http://{lan_ip}:{config_port}/`).
    pub config_url: String,
}

/// Return a snapshot of the bridge's self-info for the frontend.
#[tauri::command]
pub fn get_bridge_info(state: tauri::State<BridgeState>) -> BridgeInfo {
    let cfg = state.config.lock().unwrap();
    let lan_ip = get_local_ip();
    let config_url = match &lan_ip {
        Some(ip) => format!("http://{ip}:{}/", cfg.config_port),
        None => format!("http://localhost:{}/", cfg.config_port),
    };
    let mode = match cfg.mode {
        crate::config::BridgeMode::Direct => "direct".to_string(),
        crate::config::BridgeMode::Satellite => "satellite".to_string(),
    };
    BridgeInfo {
        bridge_id: cfg.bridge_id.to_string(),
        bridge_name: cfg.bridge_name.clone(),
        feedback_port: cfg.feedback_port,
        config_port: cfg.config_port,
        mode,
        log_level: cfg.log_level.clone(),
        lan_ip,
        config_url,
    }
}