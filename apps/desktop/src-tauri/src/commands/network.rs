use crate::discovery::{get_network_interfaces, NetworkInterface};

/// Get the local LAN IP address of this machine.
#[tauri::command]
pub fn get_local_ip() -> Option<String> {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    // Connect to Google's DNS - doesn't actually send anything
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}

/// Get all available network interfaces for mDNS service advertisement.
#[tauri::command]
pub fn get_available_interfaces() -> Vec<NetworkInterface> {
    get_network_interfaces()
}
