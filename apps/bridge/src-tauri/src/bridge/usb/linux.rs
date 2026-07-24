//! Linux evdev implementation of the USB clicker manager.
//!
//! We read input events passively: we do **not** call `evdev::Device::grab()`,
//! so the device remains usable for normal typing while the bridge watches for
//! key-ups. This avoids permission complications (no udev rules needed on most
//! distros as long as the user is in the `input` group) and keeps the bridge
//! deployable as an unprivileged service on Raspberry Pi OS.

use crate::bridge::usb::{is_perfect_cue, UsbDeviceInfo, UsbEvent};
use evdev::{Device, EventType, InputEventKind};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tokio::task::JoinHandle;

/// Scan interval for hot-plug detection.
const SCAN_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

pub struct UsbManagerImpl {
    state: Arc<LinuxState>,
    _scan_handle: JoinHandle<()>,
}

/// A running per-device reader task.
struct Tracked {
    handle: JoinHandle<()>,
}

struct LinuxState {
    devices: Mutex<HashMap<String, UsbDeviceInfo>>,
    tracked: Mutex<HashMap<String, Tracked>>,
    event_tx: broadcast::Sender<UsbEvent>,
    /// Count of USB key devices that exist but can't be opened (permission).
    access_denied: AtomicUsize,
}

impl UsbManagerImpl {
    pub fn new(event_tx: broadcast::Sender<UsbEvent>) -> std::io::Result<Self> {
        let state = Arc::new(LinuxState {
            devices: Mutex::new(HashMap::new()),
            tracked: Mutex::new(HashMap::new()),
            event_tx,
            access_denied: AtomicUsize::new(0),
        });

        let scan_state = state.clone();
        let _scan_handle = tokio::spawn(async move {
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

    pub fn access_denied_count(&self) -> usize {
        self.state.access_denied.load(Ordering::Relaxed)
    }
}

async fn scan_loop(state: Arc<LinuxState>) {
    loop {
        if let Err(e) = scan_once(&state).await {
            log::warn!("USB scan failed: {e}");
        }

        // Detect USB key devices we can see in sysfs but can't open (missing
        // permission on /dev/input/event*). Emit an event only on transitions so
        // the frontend can surface a "fix permissions" banner instead of an
        // unexplained empty list.
        let denied = count_inaccessible_usb_key_devices();
        let prev = state.access_denied.swap(denied, Ordering::Relaxed);
        if denied != prev {
            if denied > 0 {
                log::warn!(
                    "USB: {denied} USB input device(s) present but not readable \
                     (permission denied on /dev/input/event*). Add your user to \
                     the 'input' group: sudo usermod -aG input $USER, then log \
                     out and back in."
                );
            } else {
                log::info!("USB: input device access restored");
            }
            let _ = state.event_tx.send(UsbEvent::AccessDenied { count: denied });
        }

        // Clean up any reader tasks that have exited (device disconnected).
        let finished: Vec<String> = {
            let mut tracked = state.tracked.lock().await;
            let mut finished = Vec::new();
            for (id, t) in tracked.iter() {
                if t.handle.is_finished() {
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
    let enumerated = enumerate_devices()?;
    let mut tracked = state.tracked.lock().await;

    for (id, info) in enumerated {
        // Already reading this device — nothing to do.
        if tracked.contains_key(&id) {
            continue;
        }

        // Newly connected device.
        log::info!(
            "USB device detected: {} at {} (phys={})",
            info.name,
            info.path,
            info.phys
        );
        let handle = spawn_reader(state, &id, &info);
        tracked.insert(id.clone(), Tracked { handle });
        state.devices.lock().await.insert(id.clone(), info.clone());
        let _ = state.event_tx.send(UsbEvent::Connected(info));
    }

    Ok(())
}

/// Spawn a reader task for a device. On exit (device unplugged / read error)
/// the device is removed from the live list and a `Disconnected` event fires.
fn spawn_reader(state: &Arc<LinuxState>, id: &str, info: &UsbDeviceInfo) -> JoinHandle<()> {
    let path = PathBuf::from(&info.path);
    let event_tx = state.event_tx.clone();
    let state_for_reader = state.clone();
    let id_for_reader = id.to_string();
    tokio::spawn(async move {
        if let Err(e) = device_reader(&id_for_reader, path, event_tx.clone()).await {
            log::debug!("USB reader for {id_for_reader} exited: {e}");
        }
        // Remove from the live device list on exit.
        state_for_reader.devices.lock().await.remove(&id_for_reader);
        let _ = event_tx.send(UsbEvent::Disconnected {
            device_id: id_for_reader,
        });
    })
}

async fn device_reader(
    device_id: &str,
    path: PathBuf,
    event_tx: broadcast::Sender<UsbEvent>,
) -> std::io::Result<()> {
    let device = Device::open(&path)?;
    // Passive read: do NOT grab. The keyboard/clicker remains usable by the OS
    // while we watch for key-ups. This avoids needing root or udev rules.
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

/// Enumerate all currently attached USB key-capable input devices.
fn enumerate_devices() -> std::io::Result<HashMap<String, UsbDeviceInfo>> {
    let mut result = HashMap::new();

    for (path, device) in evdev::enumerate() {
        let Some(info) = classify_device(&path, &device) else {
            continue;
        };
        result.insert(info.id.clone(), info);
    }

    Ok(result)
}

/// Classify a single evdev device: accept any USB device that can emit keys
/// (clicker, keyboard, …); reject non-USB buses and HDMI CEC.
fn classify_device(path: &Path, device: &Device) -> Option<UsbDeviceInfo> {
    let name = device.name().unwrap_or("unknown").to_string();
    let input_id = device.input_id();

    // Must be able to emit at least one key.
    let keys = device.supported_keys()?;
    if keys.iter().next().is_none() {
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

/// Count USB key-capable input devices that exist but can't be opened because
/// the process lacks permission on `/dev/input/event*`.
fn count_inaccessible_usb_key_devices() -> usize {
    let entries = match std::fs::read_dir("/dev/input") {
        Ok(e) => e,
        Err(_) => return 0,
    };

    let mut count = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with("event") {
            continue;
        }

        match Device::open(&path) {
            // Openable — no permission problem for this node.
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                let phys = read_phys(&path).unwrap_or_default();
                if phys.to_lowercase().starts_with("usb")
                    && !phys.to_lowercase().contains("hdmi")
                    && sysfs_reports_keys(&path)
                {
                    count += 1;
                }
            }
            // Other errors (device vanished mid-scan, etc.) aren't permission
            // problems — ignore.
            Err(_) => {}
        }
    }
    count
}

/// True if sysfs reports this event node has at least one key capability,
/// without needing to open the (possibly unreadable) device node.
fn sysfs_reports_keys(path: &Path) -> bool {
    let Some(event_num) = path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix("event"))
    else {
        return false;
    };
    let key_caps =
        PathBuf::from(format!("/sys/class/input/event{event_num}/device/capabilities/key"));
    match std::fs::read_to_string(key_caps) {
        // The capabilities bitmap is a space-separated list of hex words; if any
        // word is nonzero the device can emit keys.
        Ok(contents) => contents
            .split_whitespace()
            .any(|w| u64::from_str_radix(w, 16).map(|v| v != 0).unwrap_or(false)),
        Err(_) => false,
    }
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
