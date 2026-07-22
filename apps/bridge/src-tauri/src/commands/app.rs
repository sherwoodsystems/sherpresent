//! # App-level commands
//!
//! Self-information about the bridge for the frontend.

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
    pub config_url: String,
}

/// Return a snapshot of the bridge's self-info for the frontend.
#[tauri::command]
pub fn get_bridge_info(state: tauri::State<BridgeState>) -> Result<BridgeInfo, String> {
    let core = state.core()?;
    let cfg = core.config();
    let lan_ip = get_local_ip();
    let config_url = match &lan_ip {
        Some(ip) => format!("http://{ip}:{}/", cfg.config_port),
        None => format!("http://localhost:{}/", cfg.config_port),
    };
    let mode = match cfg.mode {
        sherpresent_bridge_core::config::BridgeMode::Direct => "direct".to_string(),
        sherpresent_bridge_core::config::BridgeMode::Satellite => "satellite".to_string(),
    };
    Ok(BridgeInfo {
        bridge_id: cfg.bridge_id.to_string(),
        bridge_name: cfg.bridge_name,
        feedback_port: cfg.feedback_port,
        config_port: cfg.config_port,
        mode,
        log_level: cfg.log_level,
        lan_ip,
        config_url,
    })
}
