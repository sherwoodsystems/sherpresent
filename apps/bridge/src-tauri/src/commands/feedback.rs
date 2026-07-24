//! # Feedback + OSC Send (test) commands

use std::time::Duration;
use tokio::time::sleep;

use crate::state::BridgeState;

/// Cache-snapshot of the latest desktop feedback received by the bridge.
#[tauri::command]
pub fn get_feedback_state(
    state: tauri::State<BridgeState>,
) -> Result<crate::bridge::osc::feedback::FeedbackState, String> {
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
    let action: crate::bridge::config::KeyAction = command.parse()?;
    core.send_test_osc(host, port, action).await;
    sleep(Duration::from_millis(20)).await;
    Ok(())
}
