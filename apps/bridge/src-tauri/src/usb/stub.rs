//! Stub USB manager for non-Linux platforms.

use crate::usb::{UsbDeviceInfo, UsbEvent};
use tokio::sync::broadcast;

pub struct UsbManagerImpl;

impl UsbManagerImpl {
    pub fn new(_event_tx: broadcast::Sender<UsbEvent>) -> std::io::Result<Self> {
        Ok(Self)
    }

    pub async fn devices(&self) -> Vec<UsbDeviceInfo> {
        Vec::new()
    }
}
