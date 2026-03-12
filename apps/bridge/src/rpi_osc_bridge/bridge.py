#!/usr/bin/env python3
"""
RPi OSC Bridge - Keyboard to OSC converter
Listens for arrow key presses and sends OSC commands to sher-present
"""

import os
import sys
import json
import time
import signal
import logging
import re
import fcntl
import threading
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional, Dict, List
from dataclasses import dataclass

try:
    import evdev
    from evdev import InputDevice, categorize, ecodes
except ImportError:
    print("ERROR: evdev not installed. Run: sudo pip3 install evdev")
    sys.exit(1)

try:
    from pythonosc import udp_client
    from pythonosc import dispatcher
    from pythonosc import osc_server
    from pythonosc.osc_message_builder import OscMessageBuilder
except ImportError:
    print("ERROR: python-osc not installed. Run: sudo pip3 install python-osc")
    sys.exit(1)

try:
    import socket
    import struct
    import netifaces
except ImportError:
    netifaces = None  # Optional, will fall back to 255.255.255.255

# Valid channel names for broadcast mode
VALID_CHANNELS = [
    "main", "backup",
    "keynote1", "keynote2", "keynote3", "keynote4", "keynote5",
    "keynote6", "keynote7", "keynote8", "keynote9",
    "aux1", "aux2", "aux3", "aux4", "aux5",
    "aux6", "aux7", "aux8", "aux9",
]

DEFAULT_BROADCAST_PORT = 9002


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
                        # HDMI CEC devices have patterns like "vc4-hdmi-0/input0"
                        if 'hdmi' in phys:
                            return True
                        # Also check if it's NOT a USB device
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
                            "usb_port": phys,  # Use full PHYS as identifier
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
        # Skip HDMI devices entirely
        if is_hdmi_device(device.path):
            logging.debug(f"Skipping HDMI device: {device.name} at {device.path}")
            continue

        caps = device.capabilities()
        if ecodes.EV_KEY in caps:
            keys = caps[ecodes.EV_KEY]
            # Check if device has arrow keys
            if ecodes.KEY_LEFT in keys and ecodes.KEY_RIGHT in keys:
                port_info = get_usb_port_info(device.path)
                is_pc = is_perfect_cue(device.name)
                keyboard_info = {
                    "name": device.name,
                    "path": device.path,
                    "usb_port": port_info["usb_port"] if port_info else "unknown",
                    "usb_phys": port_info["phys"] if port_info else None,
                    "bus": port_info["bus"] if port_info else "unknown",
                    "is_perfect_cue": is_pc,
                    "priority": 0 if is_pc else 1  # Lower = higher priority
                }
                keyboards.append(keyboard_info)

    # Sort by priority (Perfect Cue devices first)
    keyboards.sort(key=lambda x: x["priority"])
    return keyboards


class FeedbackListener:
    """Listens for OSC feedback from sher-present and writes to state file"""

    def __init__(self, feedback_port: int, state_file: str):
        self.port = feedback_port
        self.state_file = state_file
        self.state = {
            "presenting": False,
            "open": False,
            "current_slide": 0,
            "total_slides": 0,
            "zoom_level": 0,
            "last_updated": None,
            "last_command": None,
            "last_command_time": None
        }
        self._last_error_logged = 0  # Rate limit error logging

        # Setup OSC dispatcher
        self.dispatcher = dispatcher.Dispatcher()
        self.dispatcher.map("/clicker/state/presenting", self.handle_presenting)
        self.dispatcher.map("/clicker/state/open", self.handle_open)
        self.dispatcher.map("/clicker/slide/current", self.handle_current_slide)
        self.dispatcher.map("/clicker/slide/total", self.handle_total_slides)
        self.dispatcher.map("/clicker/zoom/level", self.handle_zoom_level)

        # Create OSC server
        self.server = osc_server.ThreadingOSCUDPServer(
            ('0.0.0.0', self.port),
            self.dispatcher
        )

        self.thread = None
        self.running = False

        # Ensure state file directory exists
        try:
            os.makedirs(os.path.dirname(self.state_file), exist_ok=True)
        except Exception:
            pass  # Will log error on first write attempt

    def handle_presenting(self, address, *args):
        """Handle /clicker/state/presenting message"""
        if args:
            self.update_state({"presenting": bool(args[0])})

    def handle_open(self, address, *args):
        """Handle /clicker/state/open message"""
        if args:
            self.update_state({"open": bool(args[0])})

    def handle_current_slide(self, address, *args):
        """Handle /clicker/slide/current message"""
        if args:
            self.update_state({"current_slide": int(args[0])})

    def handle_total_slides(self, address, *args):
        """Handle /clicker/slide/total message"""
        if args:
            self.update_state({"total_slides": int(args[0])})

    def handle_zoom_level(self, address, *args):
        """Handle /clicker/zoom/level message"""
        if args:
            self.update_state({"zoom_level": int(args[0])})

    def update_state(self, updates: Dict):
        """Update state and write to file with locking"""
        self.state.update(updates)
        self.state["last_updated"] = datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z')

        try:
            # Ensure directory exists
            os.makedirs(os.path.dirname(self.state_file), exist_ok=True)

            # Write to temp file first, then atomic rename
            temp_file = self.state_file + ".tmp"
            with open(temp_file, 'w') as f:
                # Lock file during write
                fcntl.flock(f.fileno(), fcntl.LOCK_EX)
                json_str = json.dumps(self.state, indent=2)

                # Limit file size to 1KB
                if len(json_str) > 1024:
                    logging.warning("Feedback state exceeds 1KB, truncating")
                    json_str = json_str[:1024]

                f.write(json_str)
                fcntl.flock(f.fileno(), fcntl.LOCK_UN)

            # Atomic rename
            os.rename(temp_file, self.state_file)

        except Exception as e:
            # Rate limit error logging (once per 60 seconds)
            now = time.time()
            if now - self._last_error_logged > 60:
                logging.warning(f"Cannot write feedback state: {e}")
                self._last_error_logged = now

    def update_last_command(self, command: str):
        """Record the last OSC command sent"""
        self.state["last_command"] = command
        self.state["last_command_time"] = datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z')
        # Write immediately to file
        self.update_state({})

    def start(self):
        """Start listening for OSC feedback in background thread"""
        if self.running:
            return

        self.running = True
        self.thread = threading.Thread(target=self._run, daemon=True)
        self.thread.start()
        logging.debug(f"Feedback listener on port {self.port}")

    def _run(self):
        """Background thread that runs OSC server"""
        try:
            self.server.serve_forever()
        except Exception as e:
            logging.error(f"Feedback listener error: {e}")

    def stop(self):
        """Stop listening for OSC feedback"""
        if not self.running:
            return

        self.running = False
        self.server.shutdown()
        if self.thread:
            self.thread.join(timeout=2.0)
        logging.debug("Feedback listener stopped")


class BroadcastSender:
    """
    Sends OSC commands via UDP broadcast with channel addressing.
    Uses subnet broadcast address when available, falls back to 255.255.255.255.
    """

    def __init__(self, channel: str, port: int = DEFAULT_BROADCAST_PORT):
        self.channel = channel
        self.port = port
        self.broadcast_addr = self._get_broadcast_address()
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
        logging.info(f"Broadcast sender ready: {self.broadcast_addr}:{self.port} channel={self.channel}")

    def _get_broadcast_address(self) -> str:
        """Calculate subnet broadcast address from local interface."""
        if netifaces is None:
            logging.debug("netifaces not available, using 255.255.255.255")
            return "255.255.255.255"

        try:
            # Try to find non-loopback interface with IPv4
            for iface in netifaces.interfaces():
                if iface == 'lo':
                    continue

                addrs = netifaces.ifaddresses(iface)
                if netifaces.AF_INET not in addrs:
                    continue

                for addr_info in addrs[netifaces.AF_INET]:
                    ip = addr_info.get('addr')
                    netmask = addr_info.get('netmask')

                    if not ip or not netmask:
                        continue

                    # Skip loopback and link-local
                    if ip.startswith('127.') or ip.startswith('169.254.'):
                        continue

                    # Calculate broadcast address
                    ip_int = struct.unpack('!I', socket.inet_aton(ip))[0]
                    mask_int = struct.unpack('!I', socket.inet_aton(netmask))[0]
                    broadcast_int = ip_int | (~mask_int & 0xFFFFFFFF)
                    broadcast = socket.inet_ntoa(struct.pack('!I', broadcast_int))

                    logging.debug(f"Using broadcast address {broadcast} from {iface}")
                    return broadcast

        except Exception as e:
            logging.warning(f"Could not determine subnet broadcast: {e}")

        return "255.255.255.255"

    def send_next(self):
        """Send next slide command on channel."""
        self._send(f"/clicker/{self.channel}/next")

    def send_prev(self):
        """Send previous slide command on channel."""
        self._send(f"/clicker/{self.channel}/prev")

    def send_goto(self, slide: int):
        """Send goto slide command on channel."""
        self._send(f"/clicker/{self.channel}/goto", slide)

    def _send(self, address: str, *args):
        """Send OSC message to broadcast address."""
        try:
            builder = OscMessageBuilder(address=address)
            for arg in args:
                builder.add_arg(arg)
            msg = builder.build()
            self.socket.sendto(msg.dgram, (self.broadcast_addr, self.port))
            logging.debug(f"Broadcast: {address} -> {self.broadcast_addr}:{self.port}")
        except Exception as e:
            logging.error(f"Broadcast send failed: {e}")

    def close(self):
        """Close the broadcast socket."""
        try:
            self.socket.close()
        except Exception:
            pass


class BroadcastFeedbackListener:
    """
    Listens for OSC feedback from broadcast channel and writes to state file.
    Handles channel-aware messages: /clicker/<channel>/state/*
    """

    def __init__(self, channel: str, port: int, state_file: str):
        self.channel = channel
        self.port = port
        self.state_file = state_file
        self.state = {
            "channel": channel,
            "presenting": False,
            "open": False,
            "current_slide": 0,
            "total_slides": 0,
            "zoom_level": 0,
            "last_updated": None,
            "last_command": None,
            "last_command_time": None,
            "peers": []  # Track discovered peers
        }
        self._last_error_logged = 0

        # Setup OSC dispatcher with channel-aware handlers
        self.dispatcher = dispatcher.Dispatcher()
        # Match our specific channel
        self.dispatcher.map(f"/clicker/{channel}/state/presenting", self.handle_presenting)
        self.dispatcher.map(f"/clicker/{channel}/state/open", self.handle_open)
        self.dispatcher.map(f"/clicker/{channel}/state/slide", self.handle_slide)
        self.dispatcher.map(f"/clicker/{channel}/state/zoom", self.handle_zoom)
        # Also catch all channel state messages for peer discovery
        self.dispatcher.map("/clicker/*/state/*", self.handle_any_state)

        # Create OSC server - bind to broadcast port
        self.server = osc_server.ThreadingOSCUDPServer(
            ('0.0.0.0', self.port),
            self.dispatcher
        )

        self.thread = None
        self.running = False

        try:
            os.makedirs(os.path.dirname(self.state_file), exist_ok=True)
        except Exception:
            pass

    def handle_presenting(self, address, *args):
        """Handle /clicker/<channel>/state/presenting message"""
        if args:
            self.update_state({"presenting": bool(args[0])})

    def handle_open(self, address, *args):
        """Handle /clicker/<channel>/state/open message"""
        if args:
            self.update_state({"open": bool(args[0])})

    def handle_slide(self, address, *args):
        """Handle /clicker/<channel>/state/slide message (current, total)"""
        if len(args) >= 2:
            self.update_state({
                "current_slide": int(args[0]),
                "total_slides": int(args[1])
            })
        elif args:
            self.update_state({"current_slide": int(args[0])})

    def handle_zoom(self, address, *args):
        """Handle /clicker/<channel>/state/zoom message"""
        if args:
            self.update_state({"zoom_level": int(args[0])})

    def handle_any_state(self, address, *args):
        """
        Handle any state message for peer discovery.
        Parse channel from address and track unique responders.
        """
        # Parse: /clicker/<channel>/state/<type>
        parts = address.split('/')
        if len(parts) >= 4 and parts[1] == 'clicker' and parts[3] == 'state':
            peer_channel = parts[2]
            # Could track peer info here if we had source IP
            # For now, just log if it's a different channel
            if peer_channel != self.channel:
                logging.debug(f"Heard from channel '{peer_channel}': {address}")

    def update_state(self, updates: Dict):
        """Update state and write to file."""
        self.state.update(updates)
        self.state["last_updated"] = datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z')

        try:
            os.makedirs(os.path.dirname(self.state_file), exist_ok=True)

            temp_file = self.state_file + ".tmp"
            with open(temp_file, 'w') as f:
                fcntl.flock(f.fileno(), fcntl.LOCK_EX)
                json_str = json.dumps(self.state, indent=2)
                if len(json_str) > 1024:
                    json_str = json_str[:1024]
                f.write(json_str)
                fcntl.flock(f.fileno(), fcntl.LOCK_UN)

            os.rename(temp_file, self.state_file)

        except Exception as e:
            now = time.time()
            if now - self._last_error_logged > 60:
                logging.warning(f"Cannot write feedback state: {e}")
                self._last_error_logged = now

    def update_last_command(self, command: str):
        """Record the last OSC command sent."""
        self.state["last_command"] = command
        self.state["last_command_time"] = datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z')
        self.update_state({})

    def start(self):
        """Start listening for broadcast feedback in background thread."""
        if self.running:
            return

        self.running = True
        self.thread = threading.Thread(target=self._run, daemon=True)
        self.thread.start()
        logging.debug(f"Broadcast feedback listener on port {self.port} for channel '{self.channel}'")

    def _run(self):
        """Background thread that runs OSC server."""
        try:
            self.server.serve_forever()
        except Exception as e:
            logging.error(f"Broadcast feedback listener error: {e}")

    def stop(self):
        """Stop listening for feedback."""
        if not self.running:
            return

        self.running = False
        self.server.shutdown()
        if self.thread:
            self.thread.join(timeout=2.0)
        logging.debug("Broadcast feedback listener stopped")


class Config:
    """Configuration management (legacy v1 format)"""

    def __init__(self, config_path: str = "/etc/rpi-osc-bridge/config.json"):
        self.config_path = config_path
        self.osc_host = "192.168.1.100"
        self.osc_port = 9000
        self.feedback_port = 9001
        self.keyboard_device = "auto"
        self.log_level = "INFO"

        self.load()

    def load(self):
        """Load configuration from file"""
        if os.path.exists(self.config_path):
            try:
                with open(self.config_path, 'r') as f:
                    data = json.load(f)
                    self.osc_host = data.get("osc_host", self.osc_host)
                    self.osc_port = data.get("osc_port", self.osc_port)
                    self.feedback_port = data.get("feedback_port", self.feedback_port)
                    self.keyboard_device = data.get("keyboard_device", self.keyboard_device)
                    self.log_level = data.get("log_level", self.log_level)
                logging.info(f"Loaded config from {self.config_path}")
            except Exception as e:
                logging.error(f"Failed to load config: {e}")
        else:
            logging.warning(f"Config file not found: {self.config_path}, using defaults")


@dataclass
class DeviceTarget:
    """Configuration for a single registered device with channel assignment."""
    slot: str           # "usb_1", "usb_2", etc.
    label: str          # User-friendly name like "USB 1"
    usb_phys: str       # USB physical path for identification
    channel: str        # Broadcast channel (e.g., "main", "backup")


class MultiDeviceConfig:
    """
    Configuration management with per-device channel support (v3 format).
    Backward compatible with v1/v2 configs via migration.
    Each device broadcasts to its own channel.
    """
    CONFIG_VERSION = 3
    DEVICE_SLOTS = ["usb_1", "usb_2", "usb_3"]

    def __init__(self, config_path: str = "/etc/rpi-osc-bridge/config.json"):
        self.config_path = config_path
        self.version = self.CONFIG_VERSION
        self.log_level = "INFO"
        self.feedback_port = DEFAULT_BROADCAST_PORT
        self.broadcast_port = DEFAULT_BROADCAST_PORT

        self.devices: Dict[str, Optional[DeviceTarget]] = {
            slot: None for slot in self.DEVICE_SLOTS
        }
        self._raw_data = {}

        self.load()

    def load(self):
        """Load configuration, handling v1, v2, and v3 formats."""
        if not os.path.exists(self.config_path):
            logging.warning(f"Config file not found: {self.config_path}, using defaults")
            return

        try:
            with open(self.config_path, 'r') as f:
                data = json.load(f)
                self._raw_data = data

            # Check version to determine format
            version = data.get("version", 1)

            if version >= 3:
                self._load_v3(data)
            elif version >= 2:
                self._load_v2(data)
            else:
                self._load_v1(data)

            logging.info(f"Loaded config (v{version}) from {self.config_path}")

        except Exception as e:
            logging.error(f"Failed to load config: {e}")

    def _load_v1(self, data: Dict):
        """Load legacy v1 single-device config."""
        self.log_level = data.get("log_level", self.log_level)
        self.feedback_port = data.get("feedback_port", self.feedback_port)

        # Migrate v1 to device_1 slot
        osc_host = data.get("osc_host", "192.168.1.100")
        osc_port = data.get("osc_port", 9000)

        # Note: In v1 we don't have usb_phys, so device won't be registered
        # User will need to re-register via web UI
        logging.info("Loaded v1 config - devices need to be registered via web UI")

    def _load_v2(self, data: Dict):
        """Load v2 config and migrate to v3 format."""
        self.log_level = data.get("log_level", self.log_level)
        self.feedback_port = data.get("feedback_port", self.feedback_port)
        self.broadcast_port = data.get("broadcast_port", self.broadcast_port)

        # Get the global channel from v2 (used as default for migrated devices)
        global_channel = data.get("channel", "main")
        if global_channel not in VALID_CHANNELS:
            global_channel = "main"

        # Migrate v2 devices to v3 format (assign global channel to each)
        devices_data = data.get("devices", {})
        for slot in self.DEVICE_SLOTS:
            device_data = devices_data.get(slot)
            if device_data and isinstance(device_data, dict):
                # Migrate: use v2's global channel as this device's channel
                self.devices[slot] = DeviceTarget(
                    slot=slot,
                    label=device_data.get("label", slot),
                    usb_phys=device_data.get("usb_phys", ""),
                    channel=device_data.get("channel", global_channel)
                )
            else:
                self.devices[slot] = None

        logging.info(f"Migrated v2 config to v3 format (default channel: {global_channel})")

    def _load_v3(self, data: Dict):
        """Load v3 per-device channel config."""
        self.version = data.get("version", self.CONFIG_VERSION)
        self.log_level = data.get("log_level", self.log_level)
        self.feedback_port = data.get("feedback_port", self.feedback_port)
        self.broadcast_port = data.get("broadcast_port", self.broadcast_port)

        devices_data = data.get("devices", {})
        for slot in self.DEVICE_SLOTS:
            device_data = devices_data.get(slot)
            if device_data and isinstance(device_data, dict):
                channel = device_data.get("channel", "main")
                if channel not in VALID_CHANNELS:
                    logging.warning(f"Invalid channel '{channel}' for {slot}, defaulting to 'main'")
                    channel = "main"

                self.devices[slot] = DeviceTarget(
                    slot=slot,
                    label=device_data.get("label", slot),
                    usb_phys=device_data.get("usb_phys", ""),
                    channel=channel
                )
            else:
                self.devices[slot] = None

    def get_target_for_phys(self, usb_phys: str) -> Optional[DeviceTarget]:
        """Find the target configuration for a given USB physical path."""
        for slot, target in self.devices.items():
            if target and target.usb_phys == usb_phys:
                return target
        return None

    def get_registered_devices(self) -> List[DeviceTarget]:
        """Get list of all registered devices."""
        return [t for t in self.devices.values() if t is not None]

    def register_device(self, slot: str, usb_phys: str, channel: str,
                       label: str) -> bool:
        """Register a device to a slot with its broadcast channel."""
        if slot not in self.DEVICE_SLOTS:
            logging.error(f"Invalid slot: {slot}")
            return False

        if channel not in VALID_CHANNELS:
            logging.error(f"Invalid channel: {channel}")
            return False

        self.devices[slot] = DeviceTarget(
            slot=slot,
            label=label,
            usb_phys=usb_phys,
            channel=channel
        )

        return self.save()

    def unregister_device(self, slot: str) -> bool:
        """Remove a device registration."""
        if slot not in self.DEVICE_SLOTS:
            logging.error(f"Invalid slot: {slot}")
            return False

        self.devices[slot] = None
        return self.save()

    def save(self) -> bool:
        """Save configuration to file in v3 format."""
        try:
            data = {
                "version": self.CONFIG_VERSION,
                "broadcast_port": self.broadcast_port,
                "feedback_port": self.feedback_port,
                "log_level": self.log_level,
                "devices": {}
            }

            for slot, target in self.devices.items():
                if target:
                    data["devices"][slot] = {
                        "label": target.label,
                        "usb_phys": target.usb_phys,
                        "channel": target.channel
                    }
                else:
                    data["devices"][slot] = None

            os.makedirs(os.path.dirname(self.config_path), exist_ok=True)

            with open(self.config_path, 'w') as f:
                json.dump(data, f, indent=2)

            logging.info(f"Saved config to {self.config_path}")
            return True

        except Exception as e:
            logging.error(f"Failed to save config: {e}")
            return False


class KeyboardOSCBridge:
    """Main bridge service"""

    def __init__(self, config: Config):
        self.config = config
        self.running = False
        self.device: Optional[InputDevice] = None
        self.osc_client: Optional[udp_client.SimpleUDPClient] = None
        self.feedback_listener: Optional[FeedbackListener] = None

        # Setup signal handlers
        signal.signal(signal.SIGINT, self.signal_handler)
        signal.signal(signal.SIGTERM, self.signal_handler)

    def signal_handler(self, signum, frame):
        """Handle shutdown signals"""
        logging.info(f"Received signal {signum}, shutting down...")
        self.running = False

    def find_keyboard_device(self) -> Optional[InputDevice]:
        """Auto-detect USB keyboard device"""
        devices = [evdev.InputDevice(path) for path in evdev.list_devices()]

        # Log all available input devices for debugging
        logging.info(f"Found {len(devices)} input devices:")
        for device in devices:
            port_info = get_usb_port_info(device.path)
            port_str = f" [USB port: {port_info['usb_port']}]" if port_info else ""
            logging.info(f"  - {device.name} at {device.path}{port_str}")

        # Look for devices with key capabilities (keyboards)
        for device in devices:
            caps = device.capabilities()
            if ecodes.EV_KEY in caps:
                keys = caps[ecodes.EV_KEY]
                # Check if device has arrow keys
                if ecodes.KEY_LEFT in keys and ecodes.KEY_RIGHT in keys:
                    port_info = get_usb_port_info(device.path)
                    port_str = f" on USB port {port_info['usb_port']}" if port_info else ""
                    logging.info(f"Selected keyboard: {device.name} at {device.path}{port_str}")
                    return device

        logging.warning("No keyboard with arrow keys found")
        return None

    def setup(self) -> bool:
        """Initialize keyboard and OSC client"""
        # Find keyboard
        if self.config.keyboard_device == "auto":
            self.device = self.find_keyboard_device()
            if not self.device:
                logging.error("No keyboard device found with arrow keys")
                return False
        else:
            try:
                self.device = InputDevice(self.config.keyboard_device)
                logging.info(f"Using keyboard: {self.device.name}")
            except Exception as e:
                logging.error(f"Failed to open keyboard device: {e}")
                return False

        # Grab exclusive access to prevent keys reaching other apps
        try:
            self.device.grab()
            logging.info("Grabbed exclusive access to keyboard")
        except Exception as e:
            logging.warning(f"Could not grab device (may not be necessary): {e}")

        # Setup OSC client
        try:
            self.osc_client = udp_client.SimpleUDPClient(
                self.config.osc_host,
                self.config.osc_port
            )
            logging.info(f"OSC client ready: {self.config.osc_host}:{self.config.osc_port}")
        except Exception as e:
            logging.error(f"Failed to create OSC client: {e}")
            return False

        # Setup OSC feedback listener
        try:
            feedback_state_file = "/var/run/rpi-osc-bridge/feedback.json"
            self.feedback_listener = FeedbackListener(
                self.config.feedback_port,
                feedback_state_file
            )
            self.feedback_listener.start()
            logging.info(f"OSC feedback listener ready on port {self.config.feedback_port}")
        except Exception as e:
            logging.error(f"Failed to start feedback listener: {e}")
            # Non-fatal - continue without feedback
            self.feedback_listener = None

        return True

    def send_osc(self, address: str):
        """Send OSC message"""
        try:
            self.osc_client.send_message(address, [])
            logging.debug(f"Sent OSC: {address}")

            # Record command in feedback state
            if self.feedback_listener:
                self.feedback_listener.update_last_command(address)
        except Exception as e:
            logging.error(f"Failed to send OSC {address}: {e}")

    def handle_key_event(self, event):
        """Process keyboard events"""
        if event.type != ecodes.EV_KEY:
            return

        # Only respond to key down events (value == 1)
        if event.value != 1:
            return

        if event.code == ecodes.KEY_LEFT:
            logging.info("LEFT arrow pressed → /clicker/prev")
            self.send_osc("/clicker/prev")
        elif event.code == ecodes.KEY_RIGHT:
            logging.info("RIGHT arrow pressed → /clicker/next")
            self.send_osc("/clicker/next")

    def run(self):
        """Main event loop"""
        if not self.setup():
            logging.error("Setup failed, exiting")
            return 1

        # Critical: Ensure device is not None before entering read_loop
        if self.device is None:
            logging.error("No keyboard device available, cannot start event loop")
            return 1

        logging.info("Bridge running, listening for arrow keys...")
        logging.info("Press Ctrl+C to stop")

        self.running = True

        try:
            import select

            # Keep device in blocking mode, use select() for timeout
            while self.running:
                # Wait up to 0.5 seconds for input, then check self.running flag
                r, _, _ = select.select([self.device], [], [], 0.5)
                if r:
                    # Device has events available, read them
                    for event in self.device.read():
                        self.handle_key_event(event)
                # If no events, loop continues and checks self.running again
        except Exception as e:
            logging.error(f"Error in event loop: {e}")
            return 1
        finally:
            self.cleanup()

        logging.info("Bridge stopped")
        return 0

    def cleanup(self):
        """Release resources"""
        # Stop feedback listener
        if self.feedback_listener:
            try:
                self.feedback_listener.stop()
            except Exception as e:
                logging.error(f"Error stopping feedback listener: {e}")

        # Release keyboard
        if self.device:
            try:
                self.device.ungrab()
                logging.info("Released keyboard grab")
            except Exception as e:
                logging.debug(f"Could not ungrab device: {e}")
            try:
                self.device.close()
                logging.info("Closed keyboard device")
            except Exception as e:
                logging.error(f"Error closing device: {e}")


@dataclass
class ActiveDevice:
    """Tracks an active device with its input and broadcast sender."""
    input_device: InputDevice
    usb_phys: str
    target: Optional[DeviceTarget]
    broadcast_sender: Optional[BroadcastSender]


class MultiDeviceBridge:
    """
    Multi-device bridge service supporting multiple USB keyboard devices.
    Each device broadcasts to its own channel based on registration.
    Unregistered devices can be detected for registration via web UI.
    """
    REGISTRATION_FILE = "/var/run/rpi-osc-bridge/registration.json"

    def __init__(self, config: MultiDeviceConfig):
        self.config = config
        self.running = False
        self.active_devices: Dict[int, ActiveDevice] = {}  # fd -> ActiveDevice
        self.feedback_listeners: Dict[str, BroadcastFeedbackListener] = {}  # channel -> listener
        self.registration_mode = False

        signal.signal(signal.SIGINT, self.signal_handler)
        signal.signal(signal.SIGTERM, self.signal_handler)

    def signal_handler(self, signum, frame):
        """Handle shutdown signals."""
        logging.info(f"Received signal {signum}, shutting down...")
        self.running = False

    def setup(self) -> bool:
        """Initialize all registered devices with their broadcast senders."""
        # Find all USB keyboard devices
        keyboards = find_keyboards_with_ports()

        logging.info(f"Found {len(keyboards)} keyboard device(s):")
        for kb in keyboards:
            pc_str = " [Perfect Cue]" if kb.get("is_perfect_cue") else ""
            logging.info(f"  - {kb['name']} at {kb['path']}{pc_str}")
            logging.info(f"    PHYS: {kb.get('usb_phys', 'unknown')}")

        # Track which channels are in use for feedback listeners
        active_channels = set()

        # Match keyboards to registered devices
        registered = self.config.get_registered_devices()
        if not registered:
            logging.warning("No devices registered - use web UI to register devices")

        for kb in keyboards:
            usb_phys = kb.get("usb_phys")
            if not usb_phys:
                continue

            # Find matching registration
            target = self.config.get_target_for_phys(usb_phys)
            if not target:
                logging.debug(f"Device {kb['name']} not registered")
                continue

            # Setup this device with its own BroadcastSender
            try:
                input_device = InputDevice(kb["path"])
                input_device.grab()

                # Create broadcast sender for this device's channel
                sender = BroadcastSender(
                    channel=target.channel,
                    port=self.config.broadcast_port
                )

                active = ActiveDevice(
                    input_device=input_device,
                    usb_phys=usb_phys,
                    target=target,
                    broadcast_sender=sender
                )

                self.active_devices[input_device.fd] = active
                active_channels.add(target.channel)

                logging.info(f"  {target.label} -> channel '{target.channel}'")

            except Exception as e:
                logging.error(f"Failed to setup device {kb['name']}: {e}")

        # Also open unregistered devices for registration detection
        # (Any USB keyboard, not just Perfect Cue)
        for kb in keyboards:
            usb_phys = kb.get("usb_phys")
            if not usb_phys:
                continue

            # Skip if already active
            already_active = any(
                d.usb_phys == usb_phys for d in self.active_devices.values()
            )
            if already_active:
                continue

            try:
                input_device = InputDevice(kb["path"])
                input_device.grab()

                # Create a placeholder ActiveDevice with no target/sender
                active = ActiveDevice(
                    input_device=input_device,
                    usb_phys=usb_phys,
                    target=None,
                    broadcast_sender=None
                )

                self.active_devices[input_device.fd] = active
                logging.info(f"  {kb['name']} -> (unregistered, available for registration)")

            except Exception as e:
                logging.debug(f"Could not open device {kb['name']}: {e}")

        if not self.active_devices:
            logging.warning("No devices found")

        # Setup feedback listeners for active channels
        feedback_state_file = "/var/run/rpi-osc-bridge/feedback.json"
        try:
            os.makedirs(os.path.dirname(feedback_state_file), exist_ok=True)

            # Create one feedback listener per channel
            for channel in active_channels:
                listener = BroadcastFeedbackListener(
                    channel=channel,
                    port=self.config.feedback_port,
                    state_file=feedback_state_file
                )
                listener.start()
                self.feedback_listeners[channel] = listener
                logging.info(f"Feedback listener for channel '{channel}' on port {self.config.feedback_port}")

        except Exception as e:
            logging.warning(f"Could not start feedback listener: {e}")

        return True

    def rescan_devices(self) -> int:
        """Rescan for new USB keyboard devices. Returns count of newly added devices."""
        keyboards = find_keyboards_with_ports()
        new_count = 0

        for kb in keyboards:
            usb_phys = kb.get("usb_phys")
            if not usb_phys:
                continue

            # Skip if already active
            already_active = any(
                d.usb_phys == usb_phys for d in self.active_devices.values()
            )
            if already_active:
                continue

            # Find matching registration
            target = self.config.get_target_for_phys(usb_phys)

            try:
                input_device = InputDevice(kb["path"])
                input_device.grab()

                sender = None
                if target:
                    sender = BroadcastSender(
                        channel=target.channel,
                        port=self.config.broadcast_port
                    )
                    logging.info(f"Hot-plugged: {target.label} -> channel '{target.channel}'")

                    # Setup feedback listener for this channel if not already running
                    if target.channel not in self.feedback_listeners:
                        try:
                            feedback_state_file = "/var/run/rpi-osc-bridge/feedback.json"
                            listener = BroadcastFeedbackListener(
                                channel=target.channel,
                                port=self.config.feedback_port,
                                state_file=feedback_state_file
                            )
                            listener.start()
                            self.feedback_listeners[target.channel] = listener
                        except Exception as e:
                            logging.warning(f"Could not start feedback listener for {target.channel}: {e}")
                else:
                    logging.info(f"Hot-plugged: {kb['name']} -> (unregistered, available for registration)")

                active = ActiveDevice(
                    input_device=input_device,
                    usb_phys=usb_phys,
                    target=target,
                    broadcast_sender=sender
                )
                self.active_devices[input_device.fd] = active
                new_count += 1

            except Exception as e:
                logging.debug(f"Could not open device {kb['name']}: {e}")

        return new_count

    def check_registration_mode(self):
        """Check if registration mode is requested via file."""
        try:
            if os.path.exists(self.REGISTRATION_FILE):
                with open(self.REGISTRATION_FILE, 'r') as f:
                    data = json.load(f)
                    self.registration_mode = data.get("active", False)
        except Exception:
            self.registration_mode = False

    def write_registration_detection(self, usb_phys: str, device_name: str):
        """Write detected device info for registration."""
        try:
            os.makedirs(os.path.dirname(self.REGISTRATION_FILE), exist_ok=True)

            # Read existing data
            data = {}
            if os.path.exists(self.REGISTRATION_FILE):
                with open(self.REGISTRATION_FILE, 'r') as f:
                    data = json.load(f)

            # Update with detection
            data["detected_phys"] = usb_phys
            data["detected_name"] = device_name
            data["detected_at"] = datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z')

            with open(self.REGISTRATION_FILE, 'w') as f:
                json.dump(data, f, indent=2)

            logging.debug(f"Registration detection: {usb_phys}")

        except Exception as e:
            logging.error(f"Failed to write registration detection: {e}")

    def handle_key_event(self, event, active_device: ActiveDevice):
        """Process keyboard event and broadcast to device's channel."""
        if event.type != ecodes.EV_KEY:
            return

        # Only respond to key down events
        if event.value != 1:
            return

        # Ignore KEY_B (fires after arrow presses on Perfect Cue)
        if event.code == ecodes.KEY_B:
            return

        # Check for registration mode
        self.check_registration_mode()
        if self.registration_mode:
            self.write_registration_detection(
                active_device.usb_phys,
                active_device.input_device.name
            )
            # If device isn't registered, just record for registration
            if not active_device.target:
                return

        # Skip if device not registered (no broadcast sender)
        if not active_device.target or not active_device.broadcast_sender:
            logging.debug(f"Key from unregistered device: {active_device.usb_phys}")
            return

        # Send via device's broadcast sender
        cmd = None
        if event.code == ecodes.KEY_LEFT:
            active_device.broadcast_sender.send_prev()
            cmd = "prev"
        elif event.code == ecodes.KEY_RIGHT:
            active_device.broadcast_sender.send_next()
            cmd = "next"

        if cmd:
            channel = active_device.target.channel
            logging.info(f"{active_device.target.label} [{channel}]: {cmd}")

            # Update feedback state if we have a listener for this channel
            listener = self.feedback_listeners.get(channel)
            if listener:
                listener.update_last_command(f"/clicker/{channel}/{cmd}")

    def run(self):
        """Multi-device event loop."""
        if not self.setup():
            logging.error("Setup failed, exiting")
            return 1

        active_count = sum(1 for d in self.active_devices.values() if d.target)
        total_count = len(self.active_devices)

        logging.info(f"Bridge started: {active_count} registered, {total_count} total devices")

        self.running = True

        try:
            import select

            last_rescan_time = time.time()
            rescan_interval = 2.0  # Rescan every 2 seconds when needed

            while self.running:
                if not self.active_devices:
                    # No devices - rescan periodically for hot-plugged devices
                    time.sleep(rescan_interval)
                    new_count = self.rescan_devices()
                    if new_count > 0:
                        logging.info(f"Detected {len(self.active_devices)} device(s) after rescan")
                    continue

                # Check for registration mode and rescan if active
                self.check_registration_mode()
                if self.registration_mode:
                    now = time.time()
                    if now - last_rescan_time >= rescan_interval:
                        self.rescan_devices()
                        last_rescan_time = now

                # Get all file descriptors
                fds = [d.input_device for d in self.active_devices.values()]

                r, _, _ = select.select(fds, [], [], 0.5)

                for device in r:
                    active = self.active_devices.get(device.fd)
                    if not active:
                        continue

                    try:
                        for event in device.read():
                            self.handle_key_event(event, active)
                    except OSError as e:
                        # Device disconnected
                        logging.warning(f"Device disconnected: {active.input_device.name}")
                        del self.active_devices[device.fd]

        except Exception as e:
            logging.error(f"Error in event loop: {e}")
            return 1
        finally:
            self.cleanup()

        logging.info("Bridge stopped")
        return 0

    def cleanup(self):
        """Release all resources."""
        # Stop all feedback listeners
        for channel, listener in self.feedback_listeners.items():
            try:
                listener.stop()
            except Exception as e:
                logging.error(f"Error stopping feedback listener for {channel}: {e}")
        self.feedback_listeners.clear()

        # Close broadcast senders and release input devices
        for fd, active in list(self.active_devices.items()):
            # Close broadcast sender
            if active.broadcast_sender:
                try:
                    active.broadcast_sender.close()
                except Exception:
                    pass
            # Release input device
            try:
                active.input_device.ungrab()
            except Exception:
                pass
            try:
                active.input_device.close()
            except Exception:
                pass

        self.active_devices.clear()
        logging.info("Released all devices")


def setup_logging(log_level: str):
    """Configure logging"""
    numeric_level = getattr(logging, log_level.upper(), logging.INFO)

    # Log to both console and file
    log_format = '%(asctime)s - %(levelname)s - %(message)s'

    handlers = [logging.StreamHandler(sys.stdout)]

    # Add file handler if running as service
    log_file = "/var/log/rpi-osc-bridge.log"
    if os.access("/var/log", os.W_OK):
        handlers.append(logging.FileHandler(log_file))

    logging.basicConfig(
        level=numeric_level,
        format=log_format,
        handlers=handlers
    )


def main():
    """Entry point"""
    # Load multi-device config (handles both v1 and v2 formats)
    config = MultiDeviceConfig()
    setup_logging(config.log_level)

    logging.info("=" * 50)
    logging.info("RPi OSC Bridge starting (multi-device mode)...")
    logging.info("=" * 50)

    # Create and run multi-device bridge
    bridge = MultiDeviceBridge(config)
    exit_code = bridge.run()

    sys.exit(exit_code)


if __name__ == "__main__":
    main()
