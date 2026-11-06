//! Finds the workspaces open in QLab on this network, for the QLab tab's list.
//!
//! `/workspaces` is one of the two messages QLab answers without a passcode.
//! Sent over UDP as a broadcast it reaches every QLab on the subnet, across
//! switches, and each answers on port 53001. The socket is only held for one
//! scan, so other tools can use that port in between.
//!
//! The tab scans while it's open; following a workspace is `client`'s job and
//! doesn't depend on any of this.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use rosc::{OscMessage, OscPacket, OscType};
use serde::Serialize;
use serde_json::Value;
use tokio::net::UdpSocket;
use tokio::time::{timeout_at, Instant};

use super::cues;

/// Where QLab listens for OSC.
const QLAB_PORT: u16 = 53000;
/// Where QLab sends its UDP replies, whichever port the message came from.
const REPLY_PORT: u16 = 53001;
/// How long to collect replies.
const LISTEN: Duration = Duration::from_millis(600);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundWorkspace {
    /// Where to reach it: `127.0.0.1` for a QLab on this computer
    pub host: String,
    pub name: String,
    pub id: String,
    pub version: String,
}

pub async fn scan() -> Result<Vec<FoundWorkspace>, String> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, REPLY_PORT))
        .await
        .map_err(|e| format!("Can't listen on UDP port {}: {}", REPLY_PORT, e))?;
    socket
        .set_broadcast(true)
        .map_err(|e| format!("Can't send a broadcast: {}", e))?;

    let query = rosc::encoder::encode(&OscPacket::Message(OscMessage {
        addr: "/workspaces".to_string(),
        args: vec![],
    }))
    .map_err(|e| format!("{:?}", e))?;

    let mut targets = sherpresent_core::get_broadcast_addresses();
    targets.push(Ipv4Addr::LOCALHOST);
    for target in targets {
        // A network that won't take it shouldn't stop the others.
        if let Err(e) = socket.send_to(&query, (target, QLAB_PORT)).await {
            log::debug!("QLab scan: can't send to {}: {}", target, e);
        }
    }

    let own_ips: Vec<String> = sherpresent_core::get_network_interfaces()
        .into_iter()
        .map(|iface| iface.ip)
        .collect();
    let deadline = Instant::now() + LISTEN;
    let mut found = Vec::new();
    let mut buf = [0u8; 8192];
    while let Ok(received) = timeout_at(deadline, socket.recv_from(&mut buf)).await {
        match received {
            Ok((len, from)) => add_reply(&mut found, &buf[..len], from, &own_ips),
            // Windows reports an unreachable target as a failed receive.
            Err(e) => log::debug!("QLab scan: {}", e),
        }
    }
    found.sort_by(|a, b| (&a.host, &a.name).cmp(&(&b.host, &b.name)));
    Ok(found)
}

/// Adds a `/workspaces` reply's workspaces, once per computer: this computer
/// answers on every address it has. Keyed on the host as well as the ID since
/// a main and backup machine can have copies of the same workspace open.
fn add_reply(found: &mut Vec<FoundWorkspace>, packet: &[u8], from: SocketAddr, own_ips: &[String]) {
    let Ok((_, OscPacket::Message(msg))) = rosc::decoder::decode_udp(packet) else {
        return;
    };
    if msg.addr != "/reply/workspaces" {
        return;
    }
    let Some(OscType::String(json)) = msg.args.first() else {
        return;
    };
    let Ok(reply) = serde_json::from_str::<Value>(json) else {
        return;
    };
    let Some(workspaces) = reply.get("data").and_then(Value::as_array) else {
        return;
    };

    let ip = from.ip();
    let host = if is_own(ip, own_ips) {
        Ipv4Addr::LOCALHOST.to_string()
    } else {
        ip.to_string()
    };
    for workspace in workspaces {
        let id = cues::str_field(workspace, "uniqueID");
        if id.is_empty() || found.iter().any(|f| f.host == host && f.id == id) {
            continue;
        }
        found.push(FoundWorkspace {
            host: host.clone(),
            name: cues::str_field(workspace, "displayName"),
            id,
            version: cues::str_field(workspace, "version"),
        });
    }
}

fn is_own(ip: IpAddr, own_ips: &[String]) -> bool {
    ip.is_loopback() || own_ips.contains(&ip.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reply as QLab 5.6.5 sent it.
    fn reply(workspaces: &str) -> Vec<u8> {
        let json = format!(
            r#"{{"address":"\/workspaces","status":"ok","data":[{}]}}"#,
            workspaces
        );
        rosc::encoder::encode(&OscPacket::Message(OscMessage {
            addr: "/reply/workspaces".to_string(),
            args: vec![OscType::String(json)],
        }))
        .unwrap()
    }

    const SHOW: &str = r#"{"version":"5.6.5","displayName":"Show","udpReplyPort":53001,"uniqueID":"6DD92E10","port":53000}"#;

    fn from(ip: &str) -> SocketAddr {
        SocketAddr::new(ip.parse().unwrap(), 57299)
    }

    #[test]
    fn reads_a_workspace_from_a_reply() {
        let mut found = Vec::new();
        add_reply(&mut found, &reply(SHOW), from("10.0.0.7"), &[]);
        assert_eq!(
            found,
            vec![FoundWorkspace {
                host: "10.0.0.7".to_string(),
                name: "Show".to_string(),
                id: "6DD92E10".to_string(),
                version: "5.6.5".to_string(),
            }]
        );
    }

    #[test]
    fn this_computer_is_listed_once_at_localhost() {
        let own = vec!["192.168.1.5".to_string()];
        let mut found = Vec::new();
        add_reply(&mut found, &reply(SHOW), from("127.0.0.1"), &own);
        add_reply(&mut found, &reply(SHOW), from("192.168.1.5"), &own);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].host, "127.0.0.1");
    }

    #[test]
    fn main_and_backup_with_the_same_workspace_are_both_listed() {
        let mut found = Vec::new();
        add_reply(&mut found, &reply(SHOW), from("10.0.0.7"), &[]);
        add_reply(&mut found, &reply(SHOW), from("10.0.0.8"), &[]);
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn ignores_anything_else() {
        let mut found = Vec::new();
        add_reply(&mut found, b"not osc", from("10.0.0.7"), &[]);
        add_reply(&mut found, &reply(""), from("10.0.0.7"), &[]);
        assert!(found.is_empty());
    }
}
