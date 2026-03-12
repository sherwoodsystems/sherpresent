"""Configuration management for rpi-osc-bridge."""

import os
import json
import logging
from typing import Optional, Dict, List
from dataclasses import dataclass

from constants import VALID_CHANNELS, DEFAULT_BROADCAST_PORT


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
        logging.info("Loaded v1 config - devices need to be registered via web UI")

    def _load_v2(self, data: Dict):
        """Load v2 config and migrate to v3 format."""
        self.log_level = data.get("log_level", self.log_level)
        self.feedback_port = data.get("feedback_port", self.feedback_port)
        self.broadcast_port = data.get("broadcast_port", self.broadcast_port)

        global_channel = data.get("channel", "main")
        if global_channel not in VALID_CHANNELS:
            global_channel = "main"

        devices_data = data.get("devices", {})
        for slot in self.DEVICE_SLOTS:
            device_data = devices_data.get(slot)
            if device_data and isinstance(device_data, dict):
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
