use reqwest::Client;
use std::time::Duration;

use super::types::*;

pub struct BridgeApiClient {
    base_url: String,
    client: Client,
}

impl BridgeApiClient {
    pub fn new(host: &str, port: u16) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_default();

        Self {
            base_url: format!("http://{}:{}", host, port),
            client,
        }
    }

    pub async fn get_status(&self) -> Result<BridgeStatus, String> {
        self.client
            .get(format!("{}/status", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Parse failed: {}", e))
    }

    pub async fn get_config(&self) -> Result<BridgeGlobalConfig, String> {
        self.client
            .get(format!("{}/config/global", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Parse failed: {}", e))
    }

    pub async fn save_config(&self, req: SaveGlobalConfigRequest) -> Result<BridgeApiResponse, String> {
        self.client
            .post(format!("{}/config/global", self.base_url))
            .json(&req)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Parse failed: {}", e))
    }

    pub async fn get_feedback(&self) -> Result<BridgeFeedback, String> {
        self.client
            .get(format!("{}/feedback", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Parse failed: {}", e))
    }

    pub async fn get_devices(&self) -> Result<BridgeConnectedDevices, String> {
        self.client
            .get(format!("{}/devices", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Parse failed: {}", e))
    }

    pub async fn get_registered_devices(&self) -> Result<BridgeRegisteredDevices, String> {
        self.client
            .get(format!("{}/devices/registered", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Parse failed: {}", e))
    }

    pub async fn get_registration_status(&self) -> Result<BridgeRegistrationStatus, String> {
        self.client
            .get(format!("{}/registration/status", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Parse failed: {}", e))
    }

    pub async fn start_registration(&self, slot: &str) -> Result<BridgeApiResponse, String> {
        self.client
            .post(format!("{}/registration/start", self.base_url))
            .json(&StartRegistrationRequest { slot: slot.to_string() })
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Parse failed: {}", e))
    }

    pub async fn cancel_registration(&self) -> Result<BridgeApiResponse, String> {
        self.client
            .post(format!("{}/registration/cancel", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Parse failed: {}", e))
    }

    pub async fn confirm_registration(
        &self,
        slot: &str,
        usb_phys: &str,
        channel: &str,
        label: &str,
    ) -> Result<BridgeApiResponse, String> {
        self.client
            .post(format!("{}/registration/confirm", self.base_url))
            .json(&ConfirmRegistrationRequest {
                slot: slot.to_string(),
                usb_phys: usb_phys.to_string(),
                channel: channel.to_string(),
                label: label.to_string(),
            })
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Parse failed: {}", e))
    }

    pub async fn unregister_device(&self, slot: &str) -> Result<BridgeApiResponse, String> {
        self.client
            .post(format!("{}/devices/{}/unregister", self.base_url, slot))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Parse failed: {}", e))
    }

    pub async fn test_device(&self, slot: &str, command: &str) -> Result<BridgeApiResponse, String> {
        self.client
            .post(format!("{}/devices/{}/test/{}", self.base_url, slot, command))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Parse failed: {}", e))
    }

    pub async fn get_logs(&self) -> Result<BridgeLogs, String> {
        self.client
            .get(format!("{}/logs", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Parse failed: {}", e))
    }

    pub async fn get_satellite_status(&self) -> Result<BridgeSatelliteStatus, String> {
        self.client
            .get(format!("{}/satellite/status", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Parse failed: {}", e))
    }
}
