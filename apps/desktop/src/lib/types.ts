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
// DISCOVERY CONFIG
// =============================================================================

export interface DiscoveryConfig {
  /** Whether discovery is enabled */
  enabled: boolean;
  /** Unique instance identifier (auto-generated UUID) */
  instanceId: string;
  /** Human-readable display name for this instance */
  displayName: string | null;
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
  port: number;
  ontimeHost: string;
  ontimePort: number;
  /** Font size in px for stage view notes (default: 32) */
  fontSize: number;
}

/** Live caption provider id */
export type CaptionProviderId = 'gemini' | 'openai';

/** API keys for caption providers. Stored in plaintext in config.json. */
export interface CaptionApiKeys {
  gemini: string;
  openai: string;
}

/** Live caption / translation configuration */
export interface CaptionsConfig {
  /** Auto-start captions on app launch */
  enabled: boolean;
  provider: CaptionProviderId;
  /** cpal input device name; null = system default input */
  inputDevice: string | null;
  /** BCP-47 source language; null = provider auto-detects */
  sourceLanguage: string | null;
  /** BCP-47 target language for caption output (default: 'fr') */
  targetLanguage: string;
  /** Caption font size in px, relative to a 1080p frame */
  fontSize: number;
  /** How many finalized lines stay on screen */
  maxLines: number;
  /** Overlay background: hex colour, or 'transparent' for OBS */
  chromaColor: string;
  apiKeys: CaptionApiKeys;
}

export interface AppConfig {
  osc: OscConfig;
  adapter: AdapterType;
  presentationName: string;
  logging: LoggingConfig;
  /** Peer discovery settings */
  discovery: DiscoveryConfig;
  /** Per-adapter network configuration */
  adapterConfig: AdapterConfig;
  /** LAN web server settings */
  webServer: WebServerConfig;
  /** Live caption / translation settings */
  captions: CaptionsConfig;
}

// =============================================================================
// CAPTIONS
// =============================================================================

/** An audio input device reported by cpal */
export interface AudioDevice {
  /** Device name, used as the stable identifier in config */
  name: string;
  /** Whether this is the system default input */
  isDefault: boolean;
  /** Native sample rate in Hz */
  sampleRate: number;
  /** Native channel count */
  channels: number;
}

/**
 * One caption line. `interim` segments are replaced as deltas arrive;
 * they become final on turn completion.
 */
export interface CaptionSegment {
  /** Monotonic id; interim updates reuse the id of the line they replace */
  id: number;
  /** Transcript in the speaker's original language */
  source: string;
  /** Translation in the configured target language */
  translated: string;
  /** False while the provider is still streaming deltas for this line */
  final: boolean;
  /** Unix ms when the segment was last updated */
  timestamp: number;
}

/** Connection state of the caption engine */
export type CaptionEngineState =
  | 'stopped'
  | 'starting'
  | 'running'
  | 'reconnecting'
  | { error: string };

/** Status snapshot emitted on the `caption-status` event */
export interface CaptionStatus {
  state: CaptionEngineState;
  /** Seconds of audio streamed this session — drives the cost readout */
  elapsedSeconds: number;
  /** How many times the provider session has been resumed */
  reconnects: number;
  provider: CaptionProviderId;
}

// =============================================================================
// LATENCY / DEBUG
// =============================================================================

export type CommandSource = 'osc' | 'ws' | 'ui';

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
  transition_duration?: number | null;
}

export type NotesCache = Record<string, string>;

export interface LiveStatus {
  is_open: boolean;
  is_presenting: boolean;
  current_slide: number;
  total_slides: number;
  zoom_level: number | null;
  presenter_notes?: string | null;
  /** Current build/animation step (0 = none fired). Only present when slide has builds. */
  current_build?: number | null;
  /** Total click-triggered build steps on this slide. Only present when slide has builds. */
  total_builds?: number | null;
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
  discovery: {
    enabled: true,
    instanceId: '', // Will be generated by backend
    displayName: null,
    networkInterface: null // Auto (all interfaces)
  },
  adapterConfig: { type: 'none' },
  webServer: {
    port: 8080,
    ontimeHost: '',
    ontimePort: 4001,
    fontSize: 32
  },
  captions: {
    enabled: false,
    provider: 'gemini',
    inputDevice: null, // System default input
    sourceLanguage: null, // Provider auto-detects
    targetLanguage: 'fr',
    fontSize: 56,
    maxLines: 2,
    chromaColor: '#00B140', // Broadcast green
    apiKeys: { gemini: '', openai: '' }
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
