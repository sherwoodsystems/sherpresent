"""Shared constants for rpi-osc-bridge."""

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
DEVICE_SLOTS = ["usb_1", "usb_2", "usb_3"]
