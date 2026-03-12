#!/usr/bin/env python3
"""
RPi OSC Bridge - Keyboard to OSC converter
Listens for USB keyboard key presses and broadcasts OSC commands per channel.
"""

import os
import sys
import json
import time
import signal
import logging
from datetime import datetime, timezone
from typing import Optional, Dict
from dataclasses import dataclass

try:
    import evdev
    from evdev import InputDevice, ecodes
except ImportError:
    print("ERROR: evdev not installed. Run: sudo pip3 install evdev")
    sys.exit(1)

from constants import FEEDBACK_STATE_FILE, REGISTRATION_FILE
from config import MultiDeviceConfig, DeviceTarget
from devices import find_keyboards_with_ports, get_usb_port_info
from osc import BroadcastSender, FeedbackListener


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

    def __init__(self, config: MultiDeviceConfig):
        self.config = config
        self.running = False
        self.active_devices: Dict[int, ActiveDevice] = {}  # fd -> ActiveDevice
        self.feedback_listeners: Dict[str, FeedbackListener] = {}  # channel -> listener
        self.registration_mode = False

        signal.signal(signal.SIGINT, self.signal_handler)
        signal.signal(signal.SIGTERM, self.signal_handler)

    def signal_handler(self, signum, frame):
        """Handle shutdown signals."""
        logging.info(f"Received signal {signum}, shutting down...")
        self.running = False

    def setup(self) -> bool:
        """Initialize all registered devices with their broadcast senders."""
        keyboards = find_keyboards_with_ports()

        logging.info(f"Found {len(keyboards)} keyboard device(s):")
        for kb in keyboards:
            pc_str = " [Perfect Cue]" if kb.get("is_perfect_cue") else ""
            logging.info(f"  - {kb['name']} at {kb['path']}{pc_str}")
            logging.info(f"    PHYS: {kb.get('usb_phys', 'unknown')}")

        active_channels = set()

        registered = self.config.get_registered_devices()
        if not registered:
            logging.warning("No devices registered - use web UI to register devices")

        for kb in keyboards:
            usb_phys = kb.get("usb_phys")
            if not usb_phys:
                continue

            target = self.config.get_target_for_phys(usb_phys)
            if not target:
                logging.debug(f"Device {kb['name']} not registered")
                continue

            try:
                input_device = InputDevice(kb["path"])
                input_device.grab()

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
        for kb in keyboards:
            usb_phys = kb.get("usb_phys")
            if not usb_phys:
                continue

            already_active = any(
                d.usb_phys == usb_phys for d in self.active_devices.values()
            )
            if already_active:
                continue

            try:
                input_device = InputDevice(kb["path"])
                input_device.grab()

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
        try:
            os.makedirs(os.path.dirname(FEEDBACK_STATE_FILE), exist_ok=True)

            for channel in active_channels:
                listener = FeedbackListener(
                    channel=channel,
                    port=self.config.feedback_port,
                    state_file=FEEDBACK_STATE_FILE
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

            already_active = any(
                d.usb_phys == usb_phys for d in self.active_devices.values()
            )
            if already_active:
                continue

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

                    if target.channel not in self.feedback_listeners:
                        try:
                            listener = FeedbackListener(
                                channel=target.channel,
                                port=self.config.feedback_port,
                                state_file=FEEDBACK_STATE_FILE
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
            if os.path.exists(REGISTRATION_FILE):
                with open(REGISTRATION_FILE, 'r') as f:
                    data = json.load(f)
                    self.registration_mode = data.get("active", False)
        except Exception:
            self.registration_mode = False

    def write_registration_detection(self, usb_phys: str, device_name: str):
        """Write detected device info for registration."""
        try:
            os.makedirs(os.path.dirname(REGISTRATION_FILE), exist_ok=True)

            data = {}
            if os.path.exists(REGISTRATION_FILE):
                with open(REGISTRATION_FILE, 'r') as f:
                    data = json.load(f)

            data["detected_phys"] = usb_phys
            data["detected_name"] = device_name
            data["detected_at"] = datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z')

            with open(REGISTRATION_FILE, 'w') as f:
                json.dump(data, f, indent=2)

            logging.debug(f"Registration detection: {usb_phys}")

        except Exception as e:
            logging.error(f"Failed to write registration detection: {e}")

    def handle_key_event(self, event, active_device: ActiveDevice):
        """Process keyboard event and broadcast to device's channel."""
        if event.type != ecodes.EV_KEY:
            return

        if event.value != 1:
            return

        # Ignore KEY_B (fires after arrow presses on Perfect Cue)
        if event.code == ecodes.KEY_B:
            return

        self.check_registration_mode()
        if self.registration_mode:
            self.write_registration_detection(
                active_device.usb_phys,
                active_device.input_device.name
            )
            if not active_device.target:
                return

        if not active_device.target or not active_device.broadcast_sender:
            logging.debug(f"Key from unregistered device: {active_device.usb_phys}")
            return

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
            rescan_interval = 2.0

            while self.running:
                if not self.active_devices:
                    time.sleep(rescan_interval)
                    new_count = self.rescan_devices()
                    if new_count > 0:
                        logging.info(f"Detected {len(self.active_devices)} device(s) after rescan")
                    continue

                self.check_registration_mode()
                if self.registration_mode:
                    now = time.time()
                    if now - last_rescan_time >= rescan_interval:
                        self.rescan_devices()
                        last_rescan_time = now

                fds = [d.input_device for d in self.active_devices.values()]

                r, _, _ = select.select(fds, [], [], 0.5)

                for device in r:
                    active = self.active_devices.get(device.fd)
                    if not active:
                        continue

                    try:
                        for event in device.read():
                            self.handle_key_event(event, active)
                    except OSError:
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
        for channel, listener in self.feedback_listeners.items():
            try:
                listener.stop()
            except Exception as e:
                logging.error(f"Error stopping feedback listener for {channel}: {e}")
        self.feedback_listeners.clear()

        for fd, active in list(self.active_devices.items()):
            if active.broadcast_sender:
                try:
                    active.broadcast_sender.close()
                except Exception:
                    pass
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
    """Configure logging."""
    numeric_level = getattr(logging, log_level.upper(), logging.INFO)

    log_format = '%(asctime)s - %(levelname)s - %(message)s'

    handlers = [logging.StreamHandler(sys.stdout)]

    log_file = "/var/log/rpi-osc-bridge.log"
    if os.access("/var/log", os.W_OK):
        handlers.append(logging.FileHandler(log_file))

    logging.basicConfig(
        level=numeric_level,
        format=log_format,
        handlers=handlers
    )


def main():
    """Entry point."""
    config = MultiDeviceConfig()
    setup_logging(config.log_level)

    logging.info("=" * 50)
    logging.info("RPi OSC Bridge starting (multi-device mode)...")
    logging.info("=" * 50)

    bridge = MultiDeviceBridge(config)
    exit_code = bridge.run()

    sys.exit(exit_code)


if __name__ == "__main__":
    main()
