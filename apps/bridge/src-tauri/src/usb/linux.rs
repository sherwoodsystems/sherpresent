//! Linux evdev implementation of the USB clicker manager.

use crate::usb::{is_perfect_cue, CLICKER_KEYS, UsbDeviceInfo, UsbEvent};
use evdev::{Device, EventType, InputEventKind};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::async_runtime::JoinHandle as RuntimeJoinHandle;
use tokio::sync::{broadcast, Mutex};
use tokio::task::JoinHandle;

/// Scan interval for hot-plug detection.
const SCAN_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

pub struct UsbManagerImpl {
    state: Arc<LinuxState>,
    _scan_handle: RuntimeJoinHandle<()>,
}

struct LinuxState {
    devices: Mutex<HashMap<String, UsbDeviceInfo>>,
    tracked: Mutex<HashMap<String, JoinHandle<()>>>,
    event_tx: broadcast::Sender<UsbEvent>,
}

impl UsbManagerImpl {
    pub fn new(event_tx: broadcast::Sender<UsbEvent>) -> std::io::Result<Self> {
        let state = Arc::new(LinuxState {
            devices: Mutex::new(HashMap::new()),
            tracked: Mutex::new(HashMap::new()),
            event_tx,
        });

        // Run the scan loop on Tauri's async runtime. `UsbManagerImpl::new()` is
        // called synchronously from Tauri's `setup()`, which is not inside a
        // runtime context — a bare `tokio::spawn` here would panic with
        // "there is no reactor running". Per-device readers spawned inside the
        // loop (see `scan_once`) then run within this runtime and can use
        // `tokio::spawn` freely.
        let scan_state = state.clone();
        let _scan_handle = tauri::async_runtime::spawn(async move {
            scan_loop(scan_state).await;
        });

        Ok(Self {
            state,
            _scan_handle,
        })
    }

    pub async fn devices(&self) -> Vec<UsbDeviceInfo> {
        self.state.devices.lock().await.values().cloned().collect()
    }
}

async fn scan_loop(state: Arc<LinuxState>) {
    loop {
        if let Err(e) = scan_once(&state).await {
            log::warn!("USB scan failed: {e}");
        }

        // Clean up any reader tasks that have exited (device disconnected).
        let finished: Vec<String> = {
            let mut tracked = state.tracked.lock().await;
            let mut finished = Vec::new();
            for (id, handle) in tracked.iter() {
                if handle.is_finished() {
                    finished.push(id.clone());
                }
            }
            for id in &finished {
                tracked.remove(id);
                state.devices.lock().await.remove(id);
                let _ = state.event_tx.send(UsbEvent::Disconnected {
                    device_id: id.clone(),
                });
            }
            finished
        };

        for id in finished {
            log::info!("USB device disconnected: {id}");
        }

        tokio::time::sleep(SCAN_INTERVAL).await;
    }
}

async fn scan_once(state: &Arc<LinuxState>) -> std::io::Result<()> {
    let enumerated = enumerate_clickers()?;
    let mut tracked = state.tracked.lock().await;

    for (id, info) in enumerated {
        if tracked.contains_key(&id) {
            continue;
        }

        log::info!(
            "USB clicker detected: {} at {} (phys={})",
            info.name,
            info.path,
            info.phys
        );

        // Spawn a reader for this device.
        let path = PathBuf::from(&info.path);
        let event_tx = state.event_tx.clone();
        let state_for_reader = state.clone();
        let id_for_reader = id.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = device_reader(&id_for_reader, path, event_tx.clone()).await {
                log::debug!("USB reader for {id_for_reader} exited: {e}");
            }
            // Remove from the live device list on exit.
            state_for_reader.devices.lock().await.remove(&id_for_reader);
            let _ = event_tx.send(UsbEvent::Disconnected {
                device_id: id_for_reader,
            });
        });

        tracked.insert(id.clone(), handle);
        state.devices.lock().await.insert(id.clone(), info.clone());
        let _ = state.event_tx.send(UsbEvent::Connected(info));
    }

    Ok(())
}

async fn device_reader(
    device_id: &str,
    path: PathBuf,
    event_tx: broadcast::Sender<UsbEvent>,
) -> std::io::Result<()> {
    let mut device = Device::open(&path)?;
    if let Err(e) = device.grab() {
        log::warn!("Could not grab {path:?}: {e}");
    }

    let mut stream = device.into_event_stream()?;

    loop {
        match stream.next_event().await {
            Ok(ev) => {
                if ev.event_type() != EventType::KEY {
                    continue;
                }
                let key = match ev.kind() {
                    InputEventKind::Key(k) => k,
                    _ => continue,
                };
                if !CLICKER_KEYS.contains(&key) {
                    continue;
                }
                // 0 = key-up, 1 = key-down, 2 = repeat. We trigger on key-up.
                if ev.value() != 0 {
                    continue;
                }

                let key_name = format!("{key:?}");
                log::debug!("USB {device_id} key-up: {key_name}");
                let _ = event_tx.send(UsbEvent::KeyUp {
                    device_id: device_id.to_string(),
                    key: key_name,
                });
            }
            Err(e) => {
                log::debug!("USB reader error for {device_id}: {e}");
                return Err(e);
            }
        }
    }
}

/// Enumerate all currently attached USB clicker-like input devices.
fn enumerate_clickers() -> std::io::Result<HashMap<String, UsbDeviceInfo>> {
    let mut result = HashMap::new();

    for (path, device) in evdev::enumerate() {
        let Some(info) = classify_device(&path, &device) else {
            continue;
        };
        result.insert(info.id.clone(), info);
    }

    Ok(result)
}

/// Classify a single evdev device as a clicker or not.
fn classify_device(path: &Path, device: &Device) -> Option<UsbDeviceInfo> {
    let name = device.name().unwrap_or("unknown").to_string();
    let input_id = device.input_id();

    // Must support keys and at least one clicker key.
    let keys = device.supported_keys()?;
    let has_clicker_key = CLICKER_KEYS.iter().any(|k| keys.contains(*k));
    if !has_clicker_key {
        return None;
    }

    // Only real USB HID devices; exclude HDMI CEC and other non-USB buses.
    let phys = read_phys(path).unwrap_or_default();
    if phys.is_empty() || !phys.to_lowercase().starts_with("usb") {
        return None;
    }
    if phys.to_lowercase().contains("hdmi") {
        return None;
    }

    let id = phys.clone();
    Some(UsbDeviceInfo {
        id,
        name: name.clone(),
        path: path.to_string_lossy().to_string(),
        vendor_id: input_id.vendor(),
        product_id: input_id.product(),
        phys,
        is_perfect_cue: is_perfect_cue(&name),
    })
}

/// Read the PHYS field from sysfs for a /dev/input/event* node.
fn read_phys(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    let event_num = file_name.strip_prefix("event")?;
    let sysfs = PathBuf::from(format!("/sys/class/input/event{event_num}/device/uevent"));

    let contents = std::fs::read_to_string(sysfs).ok()?;
    for line in contents.lines() {
        if let Some(value) = line.strip_prefix("PHYS=") {
            return Some(value.trim().trim_matches('"').to_string());
        }
    }
    None
}

