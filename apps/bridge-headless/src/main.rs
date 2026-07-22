//! # SherPresent Bridge — Headless
//!
//! Runs the bridge core without a GUI. Intended for Raspberry Pi and other
//! server-style deployments where the user configures the bridge via the HTTP
//! API or from another machine's desktop app.

use sherpresent_bridge_core::{BridgeCore, BridgeEvent};
use std::path::PathBuf;

#[derive(Debug)]
struct Args {
    config_dir: Option<PathBuf>,
}

fn parse_args() -> Args {
    let mut config_dir = None;
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--config-dir" | "-c" => {
                if let Some(val) = iter.next() {
                    config_dir = Some(PathBuf::from(val));
                }
            }
            "--help" | "-h" => {
                println!(
                    "SherPresent Bridge (headless)\n\n\
                     Usage: bridge-headless [OPTIONS]\n\n\
                     Options:\n\
                     -c, --config-dir <PATH>  Custom configuration directory\n\
                     -h, --help               Print this help message"
                );
                std::process::exit(0);
            }
            other => {
                eprintln!("Unknown argument: {other}");
                std::process::exit(1);
            }
        }
    }
    Args { config_dir }
}

#[tokio::main]
async fn main() {
    let args = parse_args();

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let core = match BridgeCore::new(args.config_dir) {
        Ok(core) => core,
        Err(e) => {
            log::error!("Failed to initialize bridge core: {e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = core.start().await {
        log::error!("Failed to start bridge core: {e}");
        std::process::exit(1);
    }

    let mut events = core.subscribe();
    tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            match event {
                BridgeEvent::UsbConnected(info) => {
                    log::info!("USB connected: {} ({})", info.name, info.id);
                }
                BridgeEvent::UsbDisconnected { device_id } => {
                    log::info!("USB disconnected: {device_id}");
                }
                BridgeEvent::UsbAccessDenied { count } => {
                    log::warn!("USB access denied for {count} device(s)");
                }
                BridgeEvent::RegistrationDetected {
                    device_id,
                    action,
                    key,
                } => {
                    log::info!(
                        "Registration detected: device={device_id} key={key} action={action}"
                    );
                }
            }
        }
    });

    let cfg = core.config();
    log::info!(
        "SherPresent Bridge headless running — name={}, feedback_port={}, config_port={}",
        cfg.bridge_name, cfg.feedback_port, cfg.config_port
    );

    // Wait for SIGTERM or SIGINT.
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    let mut sigterm =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();

    #[cfg(unix)]
    tokio::select! {
        _ = ctrl_c => {}
        _ = sigterm.recv() => {}
    }

    #[cfg(not(unix))]
    ctrl_c.await.unwrap();

    log::info!("Shutdown signal received; stopping bridge core");
    core.shutdown();
}
