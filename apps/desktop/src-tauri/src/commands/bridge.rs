use crate::bridge::client::BridgeApiClient;
use crate::bridge::types::*;

#[tauri::command]
pub async fn bridge_get_status(host: String, config_port: u16) -> Result<BridgeStatus, String> {
    BridgeApiClient::new(&host, config_port).get_status().await
}

#[tauri::command]
pub async fn bridge_get_config(host: String, config_port: u16) -> Result<BridgeGlobalConfig, String> {
    BridgeApiClient::new(&host, config_port).get_config().await
}

#[tauri::command]
pub async fn bridge_save_config(
    host: String,
    config_port: u16,
    config: SaveGlobalConfigRequest,
) -> Result<BridgeApiResponse, String> {
    BridgeApiClient::new(&host, config_port).save_config(config).await
}

#[tauri::command]
pub async fn bridge_get_feedback(host: String, config_port: u16) -> Result<BridgeFeedback, String> {
    BridgeApiClient::new(&host, config_port).get_feedback().await
}

#[tauri::command]
pub async fn bridge_get_devices(host: String, config_port: u16) -> Result<BridgeConnectedDevices, String> {
    BridgeApiClient::new(&host, config_port).get_devices().await
}

#[tauri::command]
pub async fn bridge_get_registered_devices(
    host: String,
    config_port: u16,
) -> Result<BridgeRegisteredDevices, String> {
    BridgeApiClient::new(&host, config_port).get_registered_devices().await
}

#[tauri::command]
pub async fn bridge_start_registration(
    host: String,
    config_port: u16,
    slot: String,
) -> Result<BridgeApiResponse, String> {
    BridgeApiClient::new(&host, config_port).start_registration(&slot).await
}

#[tauri::command]
pub async fn bridge_cancel_registration(
    host: String,
    config_port: u16,
) -> Result<BridgeApiResponse, String> {
    BridgeApiClient::new(&host, config_port).cancel_registration().await
}

#[tauri::command]
pub async fn bridge_confirm_registration(
    host: String,
    config_port: u16,
    slot: String,
    usb_phys: String,
    channel: String,
    label: String,
) -> Result<BridgeApiResponse, String> {
    BridgeApiClient::new(&host, config_port)
        .confirm_registration(&slot, &usb_phys, &channel, &label)
        .await
}

#[tauri::command]
pub async fn bridge_get_registration_status(
    host: String,
    config_port: u16,
) -> Result<BridgeRegistrationStatus, String> {
    BridgeApiClient::new(&host, config_port).get_registration_status().await
}

#[tauri::command]
pub async fn bridge_unregister_device(
    host: String,
    config_port: u16,
    slot: String,
) -> Result<BridgeApiResponse, String> {
    BridgeApiClient::new(&host, config_port).unregister_device(&slot).await
}

#[tauri::command]
pub async fn bridge_test_device(
    host: String,
    config_port: u16,
    slot: String,
    command: String,
) -> Result<BridgeApiResponse, String> {
    BridgeApiClient::new(&host, config_port).test_device(&slot, &command).await
}

#[tauri::command]
pub async fn bridge_get_logs(host: String, config_port: u16) -> Result<BridgeLogs, String> {
    BridgeApiClient::new(&host, config_port).get_logs().await
}

#[tauri::command]
pub async fn bridge_get_satellite_status(
    host: String,
    config_port: u16,
) -> Result<BridgeSatelliteStatus, String> {
    BridgeApiClient::new(&host, config_port).get_satellite_status().await
}
