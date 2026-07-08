//! # Feedback + OSC Send (test) commands
//!
//! Frontend surface for the cached desktop feedback state and a "send test
//! command" hook so the UI / HTTP API can drive the OSC sender without
//! needing a live USB clicker attached (Step 5 will wire the real path).

use crate::osc::feedback::FeedbackState;
use crate::osc::sender::OscSender;
use crate::state::BridgeState;
use std::time::Duration;
use tokio::time::sleep;

/// Cache-snapshot of the latest desktop feedback received by the bridge.
#[tauri::command]
pub fn get_feedback_state(state: tauri::State<BridgeState>) -> FeedbackState {
    state.feedback_state.lock().unwrap().clone()
}

/// Send a one-off OSC command to a target host:port.
///
/// Used by device-slot "Test" buttons and the HTTP tester endpoint so the
/// user can verify the path from bridge -> desktop without needing a clicker.
///
/// `command` is one of `"next"`, `"prev"`, or `"goto"` (with `slide`).
#[tauri::command]
pub async fn send_test_osc(
    state: tauri::State<'_, BridgeState>,
    host: String,
    port: u16,
    command: String,
    slide: Option<i32>,
) -> Result<(), String> {
    let sender = OscSender::new(&host, port)
        .map_err(|e| format!("Failed to create OSC sender: {e}"))?;

    match command.as_str() {
        "next" => sender.send_next().await,
        "prev" | "previous" => sender.send_prev().await,
        "goto" => {
            let n = slide.unwrap_or(1);
            sender.send_goto(n).await;
        }
        other => return Err(format!("Unknown test command: {other}")),
    }

    // Stamp last_command so the UI's "Last command" badge stays in sync across
    // real clicker events and manual test sends.
    {
        let mut snapshot = state.feedback_state.lock().unwrap();
        let address = match command.as_str() {
            "next" => "/oscpoint/next",
            "prev" | "previous" => "/oscpoint/previous",
            "goto" => "/oscpoint/goto/slide",
            _ => "",
        };
        if !address.is_empty() {
            snapshot.remember_outgoing_command(address);
        }
    }

    // Small delay so the user can perceive the change in the UI feedback panel.
    sleep(Duration::from_millis(20)).await;
    Ok(())
}