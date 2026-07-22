//! # Feedback + OSC Send (test) commands

use std::time::Duration;
use tokio::time::sleep;

use crate::state::BridgeState;

/// Cache-snapshot of the latest desktop feedback received by the bridge.
#[tauri::command]
pub fn get_feedback_state(
    state: tauri::State<BridgeState>,
) -> Result<sherpresent_bridge_core::osc::feedback::FeedbackState, String> {
    Ok(state.core()?.feedback_state())
}

/// Send a one-off OSC command to a target host:port.
#[tauri::command]
pub async fn send_test_osc(
    state: tauri::State<'_, BridgeState>,
    host: String,
    port: u16,
    command: String,
    _slide: Option<i32>,
) -> Result<(), String> {
    let core = state.core()?;
    let action = match command.as_str() {
        "next" => sherpresent_bridge_core::config::KeyAction::Next,
        "prev" | "previous" => sherpresent_bridge_core::config::KeyAction::Prev,
        other => return Err(format!("Unknown test command: {other}")),
    };
    core.send_test_osc(host, port, action).await;
    sleep(Duration::from_millis(20)).await;
    Ok(())
}
