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
except ImportError:
    print("ERROR: python-osc not installed. Run: sudo pip3 install python-osc")
    sys.exit(1)


def get_usb_port_info(device_path: str) -> Optional[Dict[str, str]]:
    """
    Get USB port information for an input device.
    Returns: {"usb_port": "1.2", "bus": "usb1", "device_path": "/dev/input/event0"}
    or None if not a USB device or info unavailable.
    """
    try:
        # Extract event number from device path (e.g., "/dev/input/event0" -> "0")
        event_num = device_path.split("event")[-1]

        # Read sysfs uevent file for USB topology
        sysfs_path = f"/sys/class/input/event{event_num}/device/uevent"

        if not os.path.exists(sysfs_path):
            logging.debug(f"Sysfs path not found: {sysfs_path}")
            return None

        phys = None
        with open(sysfs_path, 'r') as f:
            uevent_content = f.read()
            logging.debug(f"Reading {sysfs_path}:\n{uevent_content}")

            for line in uevent_content.split('\n'):
                if line.startswith('PHYS='):
                    # Example: PHYS="usb-0000:01:00.0-1.2/input0"
                    phys = line.split('=', 1)[1].strip().strip('"')
                    logging.debug(f"Found PHYS: {phys}")

                    # Try multiple regex patterns for USB port extraction
                    # Pattern 1: Standard format like "usb-0000:01:00.0-1.2/input0"
                    match = re.search(r'-(\d+(?:\.\d+)+)/', phys)
                    if match:
                        usb_port = match.group(1)
                        bus = phys.split('-')[0]  # e.g., "usb"

                        logging.debug(f"Extracted USB port: {usb_port}, bus: {bus}")
                        return {
                            "usb_port": usb_port,
                            "bus": bus,
                            "device_path": device_path,
                            "phys": phys
                        }

                    # Pattern 2: Just port number like "usb-1.2/input0"
                    match = re.search(r'usb-(\d+(?:\.\d+)+)', phys)
                    if match:
                        usb_port = match.group(1)

                        logging.debug(f"Extracted USB port (simple): {usb_port}")
                        return {
                            "usb_port": usb_port,
                            "bus": "usb",
                            "device_path": device_path,
                            "phys": phys
                        }

                    logging.debug(f"No USB port pattern matched in PHYS: {phys}")

        if phys is None:
            logging.debug(f"No PHYS field found in {sysfs_path}")

    except Exception as e:
        logging.debug(f"Could not get USB port info for {device_path}: {e}")

    return None


def find_keyboards_with_ports() -> List[Dict[str, str]]:
    """
    Find all keyboards and their USB port assignments.
    Returns list of dicts with device info and USB port.
    """
    keyboards = []
    devices = [evdev.InputDevice(path) for path in evdev.list_devices()]

    for device in devices:
        caps = device.capabilities()
        if ecodes.EV_KEY in caps:
            keys = caps[ecodes.EV_KEY]
            # Check if device has arrow keys
            if ecodes.KEY_LEFT in keys and ecodes.KEY_RIGHT in keys:
                port_info = get_usb_port_info(device.path)
                keyboard_info = {
                    "name": device.name,
                    "path": device.path,
                    "usb_port": port_info["usb_port"] if port_info else "unknown",
                    "bus": port_info["bus"] if port_info else "unknown"
                }
                keyboards.append(keyboard_info)

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
        os.makedirs(os.path.dirname(self.state_file), exist_ok=True)

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
            logging.debug(f"Updated feedback state: {updates}")

        except Exception as e:
            logging.error(f"Failed to write feedback state: {e}")

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
        logging.info(f"OSC feedback listener started on port {self.port}")

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
        logging.info("OSC feedback listener stopped")


class Config:
    """Configuration management"""

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
    # Load config first to get log level
    config = Config()
    setup_logging(config.log_level)

    logging.info("=" * 50)
    logging.info("RPi OSC Bridge starting...")
    logging.info(f"Target: {config.osc_host}:{config.osc_port}")
    logging.info("=" * 50)

    # Create and run bridge
    bridge = KeyboardOSCBridge(config)
    exit_code = bridge.run()

    sys.exit(exit_code)


if __name__ == "__main__":
    main()
