//! Linux evdev implementation of the USB clicker manager.

use crate::config::BridgeConfig;
use crate::usb::{is_perfect_cue, UsbDeviceInfo, UsbEvent};
use evdev::{Device, EventType, InputEventKind};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tauri::async_runtime::JoinHandle as RuntimeJoinHandle;
use tokio::sync::{broadcast, Mutex};
use tokio::task::JoinHandle;

/// Scan interval for hot-plug detection.
const SCAN_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

pub struct UsbManagerImpl {
    state: Arc<LinuxState>,
    _scan_handle: RuntimeJoinHandle<()>,
}

/// A running per-device reader task plus whether it grabbed the device.
struct Tracked {
    handle: JoinHandle<()>,
    grabbed: bool,
}

struct LinuxState {
    devices: Mutex<HashMap<String, UsbDeviceInfo>>,
    tracked: Mutex<HashMap<String, Tracked>>,
    event_tx: broadcast::Sender<UsbEvent>,
    /// Shared app config — used to decide which devices to grab exclusively.
    config: Arc<StdMutex<BridgeConfig>>,
    /// Count of USB key devices that exist but can't be opened (permission).
    access_denied: AtomicUsize,
}

impl UsbManagerImpl {
    pub fn new(
        event_tx: broadcast::Sender<UsbEvent>,
        config: Arc<StdMutex<BridgeConfig>>,
    ) -> std::io::Result<Self> {
        let state = Arc::new(LinuxState {
            devices: Mutex::new(HashMap::new()),
            tracked: Mutex::new(HashMap::new()),
            event_tx,
            config,
            access_denied: AtomicUsize::new(0),
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
                     (permission denied on /dev/input/event*)"
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

    // Devices with at least one key binding are "registered" and get grabbed
    // for exclusive access; everything else stays usable by the OS.
    let registered: HashSet<String> = {
        let cfg = state.config.lock().unwrap();
        cfg.devices
            .iter()
            .filter_map(|(id, d)| {
                d.as_ref()
                    .filter(|d| !d.bindings.is_empty())
                    .map(|_| id.clone())
            })
            .collect()
    };

    let mut tracked = state.tracked.lock().await;

    for (id, info) in enumerated {
        let want_grab = registered.contains(&id);

        match tracked.get(&id) {
            // Already reading with the correct grab state — nothing to do.
            Some(t) if t.grabbed == want_grab => continue,
            // Reading but grab state changed (device just got its first
            // binding, or lost its last). Abort and re-open with the new grab.
            // Aborting drops the Device, releasing any exclusive grab.
            Some(t) => {
                t.handle.abort();
                log::info!("USB re-opening {id} (grab={want_grab})");
                let handle = spawn_reader(state, &id, &info, want_grab);
                tracked.insert(id.clone(), Tracked { handle, grabbed: want_grab });
                state.devices.lock().await.insert(id.clone(), info.clone());
            }
            // Newly connected device.
            None => {
                log::info!(
                    "USB device detected: {} at {} (phys={}, grab={want_grab})",
                    info.name,
                    info.path,
                    info.phys
                );
                let handle = spawn_reader(state, &id, &info, want_grab);
                tracked.insert(id.clone(), Tracked { handle, grabbed: want_grab });
                state.devices.lock().await.insert(id.clone(), info.clone());
                let _ = state.event_tx.send(UsbEvent::Connected(info));
            }
        }
    }

    Ok(())
}

/// Spawn a reader task for a device. On exit (device unplugged / read error)
/// the device is removed from the live list and a `Disconnected` event fires.
fn spawn_reader(
    state: &Arc<LinuxState>,
    id: &str,
    info: &UsbDeviceInfo,
    grab: bool,
) -> JoinHandle<()> {
    let path = PathBuf::from(&info.path);
    let event_tx = state.event_tx.clone();
    let state_for_reader = state.clone();
    let id_for_reader = id.to_string();
    tokio::spawn(async move {
        if let Err(e) = device_reader(&id_for_reader, path, grab, event_tx.clone()).await {
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
    grab: bool,
    event_tx: broadcast::Sender<UsbEvent>,
) -> std::io::Result<()> {
    let mut device = Device::open(&path)?;
    // Only grab (exclusive capture) registered devices, so an unregistered
    // keyboard keeps working normally until the user binds a key on it.
    if grab {
        if let Err(e) = device.grab() {
            log::warn!("Could not grab {path:?}: {e}");
        }
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
///
/// `evdev::enumerate()` silently skips nodes it can't open, so a permission
/// problem otherwise looks identical to "no devices attached". We instead walk
/// `/dev/input/event*` directly: for each node we can't open due to
/// `PermissionDenied`, we consult sysfs (world-readable even when the device
/// node is not) to confirm it's a USB device that reports keys, and count it.
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

