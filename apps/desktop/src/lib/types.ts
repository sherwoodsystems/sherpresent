// =============================================================================
// FEEDBACK DESTINATION
// =============================================================================

export interface FeedbackDestination {
  host: string;
  port: number;
}

// =============================================================================
// OSC CONFIG
// =============================================================================

export interface OscConfig {
  receivePort: number;
  /** Legacy single feedback port (for backward compatibility) */
  feedbackPort: number;
  /** Legacy single feedback host (for backward compatibility) */
  feedbackHost: string;
  host: string;
  /** Multiple feedback destinations */
  feedbackDestinations: FeedbackDestination[];
}

// =============================================================================
// CHANNEL CONFIG
// =============================================================================

// Import from generated constants (source of truth: spec/protocol-constants.json)
import { VALID_CHANNELS, DEFAULT_BROADCAST_PORT, type ChannelName } from './generated-constants';
export { VALID_CHANNELS, DEFAULT_BROADCAST_PORT, type ChannelName };

export interface ChannelConfig {
  /** Whether channel sync is enabled */
  enabled: boolean;
  /** Name of the channel (e.g., "main", "backup", "keynote1") */
  channelName: string;
  /** Unique instance identifier (auto-generated UUID) */
  instanceId: string;
  /** Human-readable display name for this instance */
  displayName: string | null;
  /** Whether to use UDP broadcast mode (vs direct IP) */
  broadcastMode: boolean;
  /** Port for broadcast communication (default: 9002) */
  broadcastPort: number;
  /** Network interface to advertise mDNS on (null = auto/all interfaces) */
  networkInterface: string | null;
}

// =============================================================================
// NETWORK INTERFACE
// =============================================================================

/** Information about a network interface available for mDNS */
export interface NetworkInterface {
  /** Interface name (e.g., "en0", "Wi-Fi", "Ethernet") */
  name: string;
  /** IPv4 address of the interface */
  ip: string;
  /** Whether this is a loopback interface */
  isLoopback: boolean;
}

// =============================================================================
// DISCOVERED PEER
// =============================================================================

export interface DiscoveredPeer {
  /** Unique instance identifier */
  instanceId: string;
  /** Human-readable display name */
  displayName: string | null;
  /** Auto-assigned numeric ID for easy reference (1, 2, 3...) */
  displayId: number;
  /** IP address or hostname of the peer */
  host: string;
  /** OSC port */
  port: number;
  /** Channel name */
  channel: string;
  /** Protocol version */
  version: string;
  /** Whether this is our own instance */
  isSelf: boolean;
  /** Config API port (bridges only) */
  configPort: number | null;
}

// =============================================================================
// LOGGING CONFIG
// =============================================================================

export interface LoggingConfig {
  enabled: boolean;
  verbose: boolean;
}

// =============================================================================
// APP CONFIG
// =============================================================================

export type AdapterType = 'powerpoint' | 'keynote' | 'libreoffice' | 'canva';

/** Per-adapter network configuration */
export type AdapterConfig =
  | { type: 'libreoffice'; host: string; port: number }
  | { type: 'canva'; url: string }
  | { type: 'none' };

/** Connection status for network-based adapters */
export type ConnectionStatus = 'Disconnected' | 'Connecting' | 'Connected' | { Error: string };

/** LAN web server configuration (notes + timer view for external browsers) */
export interface WebServerConfig {
  enabled: boolean;
  port: number;
  ontimeHost: string;
  ontimePort: number;
  /** Font size in px for stage view notes (default: 32) */
  fontSize: number;
}

export interface AppConfig {
  osc: OscConfig;
  adapter: AdapterType;
  presentationName: string;
  logging: LoggingConfig;
  /** Channel synchronization settings */
  channel: ChannelConfig;
  /** Per-adapter network configuration */
  adapterConfig: AdapterConfig;
  /** LAN web server settings */
  webServer: WebServerConfig;
}

// =============================================================================
// LATENCY / DEBUG
// =============================================================================

export type CommandSource = 'osc' | 'osc_broadcast' | 'ui';

export interface LatencyEvent {
  command_received_ms: number;
  adapter_complete_ms: number;
  latency_ms: number;
  command: string;
  source: CommandSource;
  adapter: string;
  wall_clock_ms: number;
}

// =============================================================================
// PRESENTATION STATE
// =============================================================================

export interface PresentationState {
  is_open: boolean;
  is_presenting: boolean;
}

export interface SlideInfo {
  current: number;
  total: number;
}

export type NotesCache = Record<string, string>;

export interface LiveStatus {
  is_open: boolean;
  is_presenting: boolean;
  current_slide: number;
  total_slides: number;
  zoom_level: number | null;
  presenter_notes?: string | null;
}

export const defaultConfig: AppConfig = {
  osc: {
    receivePort: 9000,
    feedbackPort: 9001,
    feedbackHost: '127.0.0.1',
    host: '0.0.0.0',
    feedbackDestinations: []
  },
  adapter: 'libreoffice', // Default to libreoffice (cross-platform)
  presentationName: '',
  logging: {
    enabled: true,
    verbose: false
  },
  channel: {
    enabled: true,
    channelName: 'main',
    instanceId: '', // Will be generated by backend
    displayName: null,
    broadcastMode: true,
    broadcastPort: DEFAULT_BROADCAST_PORT,
    networkInterface: null // Auto (all interfaces)
  },
  adapterConfig: { type: 'none' },
  webServer: {
    enabled: false,
    port: 8080,
    ontimeHost: '',
    ontimePort: 4001,
    fontSize: 32
  }
};

// =============================================================================
// BRIDGE REMOTE CONFIG TYPES
// =============================================================================

export interface BridgeStatus {
  running: boolean;
}

export interface BridgeSatelliteConfig {
  host: string | null;
  port: number | null;
}

export interface BridgeGlobalConfig {
  mode: string;
  broadcast_port: number;
  feedback_port: number;
  log_level: string;
  bridge_id?: string | null;
  bridge_name?: string | null;
  satellite: BridgeSatelliteConfig;
  valid_channels: string[];
  valid_modes: string[];
  error?: string | null;
}

export interface BridgeDeviceSlot {
  label: string;
  usb_phys: string;
  channel: string;
}

export interface BridgeRegisteredDevices {
  devices: Record<string, BridgeDeviceSlot | null>;
  mode?: string | null;
  log_level?: string | null;
  feedback_port?: number | null;
  broadcast_port?: number | null;
  bridge_id?: string | null;
  bridge_name?: string | null;
  valid_channels?: string[] | null;
  valid_modes?: string[] | null;
  error?: string | null;
}

export interface BridgeRegistrationStatus {
  active: boolean;
  target_slot: string | null;
  detected_phys: string | null;
  detected_name: string | null;
}

export interface BridgeSatelliteStatus {
  connected: boolean;
  host: string | null;
  port: number | null;
  companion_version: string | null;
  api_version: string | null;
}

export interface BridgeFeedback {
  presenting: boolean;
  open: boolean;
  current_slide: number;
  total_slides: number;
  zoom_level: number;
  last_updated: string | null;
  last_command: string | null;
  last_command_time: string | null;
}

export interface BridgeApiResponse {
  success: boolean;
  message: string;
}

export interface BridgeLogs {
  logs: string[];
}

export interface SaveGlobalConfigRequest {
  mode?: string;
  broadcast_port: number;
  feedback_port: number;
  log_level: string;
  bridge_name?: string;
  satellite?: BridgeSatelliteConfig;
}

export interface BridgeConnectedDevices {
  devices: Array<{
    name: string;
    path: string;
    usb_phys?: string | null;
    is_perfect_cue?: boolean | null;
  }>;
}
