"""Shared constants for rpi-osc-bridge."""

try:
    from evdev import ecodes
    NEXT_KEYS = {ecodes.KEY_RIGHT, ecodes.KEY_PAGEDOWN}
    PREV_KEYS = {ecodes.KEY_LEFT, ecodes.KEY_PAGEUP}
    IGNORED_KEYS = {ecodes.KEY_B}  # Perfect Cue phantom key
    ALL_CLICKER_KEYS = NEXT_KEYS | PREV_KEYS
except ImportError:
    NEXT_KEYS = set()
    PREV_KEYS = set()
    IGNORED_KEYS = set()
    ALL_CLICKER_KEYS = set()

MDNS_SERVICE_TYPE = "_sher-present._udp.local."
BRIDGE_VERSION_STRING = "bridge"

VALID_MODES = ["direct", "satellite"]
DEFAULT_SATELLITE_PORT = 16622
DEFAULT_DIRECT_PORT = 9000
DEFAULT_FEEDBACK_PORT = 9001

# Re-export from generated constants (source of truth: spec/protocol-constants.json)
try:
    from rpi_osc_bridge.generated_constants import VALID_CHANNELS, DEFAULT_BROADCAST_PORT
except ImportError:
    from generated_constants import VALID_CHANNELS, DEFAULT_BROADCAST_PORT
DEFAULT_CONFIG_PORT = 80

CONFIG_FILE = "/etc/rpi-osc-bridge/config.json"
REGISTRATION_FILE = "/var/run/rpi-osc-bridge/registration.json"
FEEDBACK_STATE_FILE = "/var/run/rpi-osc-bridge/feedback.json"
PEERS_STATE_FILE = "/var/run/rpi-osc-bridge/peers.json"
SATELLITE_STATUS_FILE = "/var/run/rpi-osc-bridge/satellite.json"
DEVICE_SLOTS = ["usb_1", "usb_2", "usb_3"]
