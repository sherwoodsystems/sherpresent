use sherpresent_core::{get_network_interfaces, NetworkInterface};

/// Local LAN IP address, or 127.0.0.1 when offline.
pub fn get_local_ip_internal() -> String {
    sherpresent_core::get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string())
}

/// `http://<lan-ip>:<port><path>`, the address other machines reach us at.
pub fn lan_url(port: u16, path: &str) -> String {
    format!("http://{}:{}{}", get_local_ip_internal(), port, path)
}

/// Get the local LAN IP address of this machine.
#[tauri::command]
pub fn get_local_ip() -> Option<String> {
    let ip = get_local_ip_internal();
    if ip == "127.0.0.1" { None } else { Some(ip) }
}

/// Get all available network interfaces for mDNS service advertisement.
#[tauri::command]
pub fn get_available_interfaces() -> Vec<NetworkInterface> {
    get_network_interfaces()
}
