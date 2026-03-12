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

VALID_MODES = ["broadcast", "satellite"]
DEFAULT_SATELLITE_PORT = 16622

VALID_CHANNELS = [
    "main", "backup",
    "keynote1", "keynote2", "keynote3", "keynote4", "keynote5",
    "keynote6", "keynote7", "keynote8", "keynote9",
    "aux1", "aux2", "aux3", "aux4", "aux5",
    "aux6", "aux7", "aux8", "aux9",
]

DEFAULT_BROADCAST_PORT = 9002

CONFIG_FILE = "/etc/rpi-osc-bridge/config.json"
REGISTRATION_FILE = "/var/run/rpi-osc-bridge/registration.json"
FEEDBACK_STATE_FILE = "/var/run/rpi-osc-bridge/feedback.json"
SATELLITE_STATUS_FILE = "/var/run/rpi-osc-bridge/satellite.json"
DEVICE_SLOTS = ["usb_1", "usb_2", "usb_3"]
