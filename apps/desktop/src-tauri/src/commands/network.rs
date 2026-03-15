use crate::discovery::{get_network_interfaces, NetworkInterface};

/// Internal helper to get the local LAN IP address.
pub fn get_local_ip_internal() -> String {
    use std::net::UdpSocket;
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            socket.connect("8.8.8.8:80")?;
            socket.local_addr()
        })
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
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
