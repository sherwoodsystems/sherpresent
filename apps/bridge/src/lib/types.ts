// =============================================================================
// SherPresent Bridge — TypeScript types mirroring the Rust structs in
// `apps/bridge/src-tauri/src/`.
//
// Field names follow whatever serde renames are declared on the Rust side:
// - `DiscoveredPeer` (from sherpresent-core): camelCase
// - `BridgeConfig`: snake_case (mirrors the Python bridge v5 schema)
// - `BridgeInfo`: snake_case (private to the bridge's webview)
// - `FeedbackState`: camelCase (mirrors the desktop feedback fields)
//
// NOTE: `DiscoveredPeer` is intentionally duplicated from the desktop app's
// types.ts rather than cross-imported so the bridge bundles as a standalone
// SPA. The sherpresent-core shared crate guarantees the wire format stays in
// sync; any drift in this TS def needs to be fixed when the Rust struct
// changes.
// =============================================================================

// =============================================================================
// DISCOVERED PEER
// =============================================================================

export interface DiscoveredPeer {
  /** Unique instance identifier (UUID) */
  instanceId: string;
  /** Human-readable display name (e.g., "Chris's Laptop") */
  displayName: string | null;
  /** Auto-assigned numeric ID for easy reference (1, 2, 3...) */
  displayId: number;
  /** IP address of the peer */
  host: string;
  /** OSC port of the peer */
  port: number;
  /** Protocol version (e.g., "1" for desktops, "bridge" for bridges) */
  version: string;
  /** Whether this is our own instance */
  isSelf: boolean;
  /** Config API port (bridges only). `null` for desktop peers. */
  configPort: number | null;
}

// =============================================================================
// BRIDGE CONFIG (mirrors the v5 schema from the Python bridge)
// =============================================================================

export type BridgeMode = 'direct' | 'satellite';

export interface DeviceTarget {
  host: string;
  port: number;
  name?: string | null;
  instance_id?: string | null;
}

export type KeyAction = 'next' | 'prev';

export interface DeviceConfig {
  label: string;
  /** Stable USB physical path used for hot-plug matching. */
  usb_phys: string;
  target?: DeviceTarget | null;
  /** evdev key name (e.g. "KEY_RIGHT") → action. A device is "registered"
   *  once it has at least one binding. */
  bindings: Record<string, KeyAction>;
}

export interface SatelliteConfig {
  host: string | null;
  port: number;
}

export interface BridgeConfig {
  version: number;
  mode: BridgeMode;
  /** UDP port where the bridge listens for OSC feedback from desktops. */
  feedback_port: number;
  /** TCP port where the bridge serves its HTTP config API. */
  config_port: number;
  log_level: 'DEBUG' | 'INFO' | 'WARNING' | 'ERROR';
  bridge_id: string;
  bridge_name: string;
  satellite: SatelliteConfig;
  /** Registered USB devices keyed by device id. Legacy `null` slots are
   *  dropped by the backend on load, so values are always present. */
  devices: Record<string, DeviceConfig>;
}

// =============================================================================
// BRIDGE INFO (self-description returned by get_bridge_info)
// =============================================================================

export interface BridgeInfo {
  bridge_id: string;
  bridge_name: string;
  feedback_port: number;
  config_port: number;
  mode: BridgeMode;
  log_level: string;
  /** LAN IP advertised via mDNS; null if we couldn't detect one. */
  lan_ip: string | null;
  /** `http://{lan_ip}:{config_port}/`. */
  config_url: string;
}

// =============================================================================
// USB CLICKER DEVICES
// =============================================================================

export interface UsbDeviceInfo {
  id: string;
  name: string;
  path: string;
  vendor_id: number;
  product_id: number;
  phys: string;
  is_perfect_cue: boolean;
}

export interface UsbRegistrationDetected {
  deviceId: string;
  action: KeyAction;
  key: string;
}

/** Whether the app can read USB input devices (Linux permissions). */
export interface UsbPermissionStatus {
  accessDenied: boolean;
  count: number;
  fixCommand: string;
}

// =============================================================================
// FEEDBACK STATE (cached snapshot of OSC feedback from desktops)
// =============================================================================

export interface FeedbackState {
  presenting: boolean | null;
  open: boolean | null;
  currentSlide: number | null;
  slideCount: number | null;
  presentationName: string | null;
  slideNotes: string | null;
  lastUpdated: string | null;
  lastCommand: string | null;
  lastCommandTime: string | null;
}