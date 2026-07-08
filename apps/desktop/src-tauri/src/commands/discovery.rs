use tauri::{AppHandle, Emitter};
use sherpresent_core::{DiscoveredPeer, DiscoveryService};
use crate::state::AppState;

#[tauri::command]
pub fn get_discovered_peers(state: tauri::State<AppState>) -> Vec<DiscoveredPeer> {
    let discovery = state.discovery_service.lock().unwrap();
    if let Some(service) = &*discovery {
        service.get_peers()
    } else {
        Vec::new()
    }
}

#[tauri::command]
pub async fn start_discovery(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    instance_id: String,
    display_name: Option<String>,
    osc_port: u16,
    network_interface: Option<String>,
) -> Result<(), String> {
    // Check if already running
    {
        let discovery = state.discovery_service.lock().unwrap();
        if discovery.is_some() {
            return Err("Discovery service already running".to_string());
        }
    }

    log::info!(
        "Starting discovery service (instance: {}, name: {:?})",
        instance_id,
        display_name,
    );

    // Create channel for peer updates
    let (peer_tx, mut peer_rx) = tokio::sync::mpsc::channel::<Vec<DiscoveredPeer>>(32);

    // Create and start the discovery service
    let mut service = DiscoveryService::new(instance_id, display_name, osc_port, peer_tx, network_interface)?;

    service.register()?;
    let _browse_handle = service.start_browsing()?;

    // Store the service
    {
        let mut discovery_slot = state.discovery_service.lock().unwrap();
        *discovery_slot = Some(service);
    }

    // Spawn a task to forward peer updates to the frontend
    let app_clone = app.clone();
    tokio::spawn(async move {
        while let Some(peers) = peer_rx.recv().await {
            let _ = app_clone.emit("peers-updated", &peers);
        }
    });

    log::info!("Discovery service started");

    Ok(())
}

#[tauri::command]
pub fn set_instance_name(state: tauri::State<AppState>, name: Option<String>) -> Result<(), String> {
    let mut discovery = state.discovery_service.lock().unwrap();
    if let Some(service) = discovery.as_mut() {
        service.update_display_name(name)?;
        log::info!("Instance display name updated");
    } else {
        return Err("Discovery service not running".to_string());
    }
    Ok(())
}

#[tauri::command]
pub fn stop_discovery(state: tauri::State<AppState>) -> Result<(), String> {
    let mut discovery = state.discovery_service.lock().unwrap();
    if let Some(mut service) = discovery.take() {
        service.shutdown()?;
        log::info!("Discovery service stopped");
    }
    Ok(())
}
