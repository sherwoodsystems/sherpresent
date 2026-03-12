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

/** Valid channel names for broadcast mode */
export const VALID_CHANNELS = [
  'main', 'backup',
  'keynote1', 'keynote2', 'keynote3', 'keynote4', 'keynote5',
  'keynote6', 'keynote7', 'keynote8', 'keynote9',
  'aux1', 'aux2', 'aux3', 'aux4', 'aux5',
  'aux6', 'aux7', 'aux8', 'aux9'
] as const;

export type ChannelName = typeof VALID_CHANNELS[number];

/** Default broadcast port for channel communication */
export const DEFAULT_BROADCAST_PORT = 9002;

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

export interface AppConfig {
  osc: OscConfig;
  adapter: AdapterType;
  presentationName: string;
  logging: LoggingConfig;
  /** Channel synchronization settings */
  channel: ChannelConfig;
  /** Per-adapter network configuration */
  adapterConfig: AdapterConfig;
}

export interface PresentationState {
  is_open: boolean;
  is_presenting: boolean;
}

export interface SlideInfo {
  current: number;
  total: number;
}

export interface LiveStatus {
  is_open: boolean;
  is_presenting: boolean;
  current_slide: number;
  total_slides: number;
  zoom_level: number | null;
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
  adapterConfig: { type: 'none' }
};
