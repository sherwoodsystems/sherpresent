//! # Bridge State
//!
//! Tauri application state. Holds the shared `BridgeCore` instance so Tauri
//! commands can access it.

use crate::bridge::BridgeCore;
use std::sync::Mutex;

pub struct BridgeState {
    pub core: Mutex<Option<BridgeCore>>,
}

impl Default for BridgeState {
    fn default() -> Self {
        Self {
            core: Mutex::new(None),
        }
    }
}

impl BridgeState {
    /// Get the bridge core, returning an error if it is not initialized.
    pub fn core(&self) -> Result<BridgeCore, String> {
        self.core
            .lock()
            .map_err(|e| e.to_string())?
            .clone()
            .ok_or_else(|| "Bridge core not initialized".to_string())
    }
}
