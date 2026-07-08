"""USB device detection utilities for rpi-osc-bridge."""

import os
import logging
from typing import Optional, Dict, List

try:
    import evdev
    from evdev import ecodes
except ImportError:
    print("ERROR: evdev not installed. Run: sudo pip3 install evdev")
    import sys
    sys.exit(1)

from constants import ALL_CLICKER_KEYS


def is_perfect_cue(device_name: str) -> bool:
    """Check if device is a DSan Perfect Cue based on name."""
    name_lower = device_name.lower()
    return "perfect cue" in name_lower or "dsan" in name_lower


def is_hdmi_device(device_path: str) -> bool:
    """
    Check if device is HDMI-connected (should be excluded).
    HDMI CEC devices have 'hdmi' in their PHYS path.
    """
    try:
        event_num = device_path.split("event")[-1]
        sysfs_path = f"/sys/class/input/event{event_num}/device/uevent"

        if os.path.exists(sysfs_path):
            with open(sysfs_path, 'r') as f:
                for line in f:
                    if line.startswith('PHYS='):
                        phys = line.split('=', 1)[1].strip().strip('"').lower()
                        if 'hdmi' in phys:
                            return True
                        if not phys.startswith('usb'):
                            return True
    except Exception:
        pass
    return False


def get_usb_port_info(device_path: str) -> Optional[Dict[str, str]]:
    """
    Get USB port information for an input device.
    Returns full PHYS string as unique device identifier.
    """
    try:
        event_num = device_path.split("event")[-1]
        sysfs_path = f"/sys/class/input/event{event_num}/device/uevent"

        if not os.path.exists(sysfs_path):
            logging.debug(f"Sysfs path not found: {sysfs_path}")
            return None

        with open(sysfs_path, 'r') as f:
            for line in f:
                if line.startswith('PHYS='):
                    phys = line.split('=', 1)[1].strip().strip('"')
                    logging.debug(f"Found PHYS: {phys}")
                    if phys and phys.startswith('usb'):
                        return {
                            "usb_port": phys,
                            "bus": "usb",
                            "device_path": device_path,
                            "phys": phys
                        }

        logging.debug(f"No USB PHYS field found in {sysfs_path}")

    except Exception as e:
        logging.debug(f"Could not get USB port info for {device_path}: {e}")

    return None


def find_keyboards_with_ports() -> List[Dict[str, str]]:
    """
    Find all keyboards and their USB port assignments.
    Returns list of dicts with device info and USB port.
    Priority sorted: Perfect Cue devices first, then other USB devices.
    HDMI devices are excluded entirely.
    """
    keyboards = []
    devices = [evdev.InputDevice(path) for path in evdev.list_devices()]

    for device in devices:
        if is_hdmi_device(device.path):
            logging.debug(f"Skipping HDMI device: {device.name} at {device.path}")
            continue

        caps = device.capabilities()
        if ecodes.EV_KEY in caps:
            keys = caps[ecodes.EV_KEY]
            if any(k in keys for k in ALL_CLICKER_KEYS):
                port_info = get_usb_port_info(device.path)
                is_pc = is_perfect_cue(device.name)
                keyboard_info = {
                    "name": device.name,
                    "path": device.path,
                    "usb_port": port_info["usb_port"] if port_info else "unknown",
                    "usb_phys": port_info["phys"] if port_info else None,
                    "bus": port_info["bus"] if port_info else "unknown",
                    "is_perfect_cue": is_pc,
                    "priority": 0 if is_pc else 1
                }
                keyboards.append(keyboard_info)

    keyboards.sort(key=lambda x: x["priority"])
    return keyboards
