"""Configuration management for rpi-osc-bridge."""

import os
import json
import logging
import uuid
import socket
from typing import Optional, Dict, List
from dataclasses import dataclass

from constants import VALID_MODES, DEFAULT_SATELLITE_PORT, DEFAULT_FEEDBACK_PORT


@dataclass
class DeviceTarget:
    """Configuration for a single registered device with target assignment."""
    slot: str           # "usb_1", "usb_2", etc.
    label: str          # User-friendly name like "USB 1"
    usb_phys: str       # USB physical path for identification
    target: dict | None  # {host, port, instance_id?, name?} or None (no target)


class MultiDeviceConfig:
    """
    Configuration management with per-device target support (v5 format).
    Backward compatible with v1/v2/v3/v4 configs via migration.
    Supports direct mode (OSC unicast) and satellite mode (Companion).
    """
    CONFIG_VERSION = 5
    DEVICE_SLOTS = ["usb_1", "usb_2", "usb_3"]

    def __init__(self, config_path: str = "/etc/rpi-osc-bridge/config.json"):
        self.config_path = config_path
        self.version = self.CONFIG_VERSION
        self.log_level = "INFO"
        self.feedback_port = DEFAULT_FEEDBACK_PORT

        self.mode = "direct"
        self.satellite_host: Optional[str] = None
        self.satellite_port: int = DEFAULT_SATELLITE_PORT

        self.bridge_id = ""
        self.bridge_name = ""

        self.devices: Dict[str, Optional[DeviceTarget]] = {
            slot: None for slot in self.DEVICE_SLOTS
        }
        self._raw_data = {}

        self.load()

        # Auto-generate identity if missing
        dirty = False
        if not self.bridge_id:
            self.bridge_id = str(uuid.uuid4())
            dirty = True
        if not self.bridge_name:
            self.bridge_name = f"Bridge {self.bridge_id[:6]}"
            dirty = True
        if dirty:
            self.save()

    def load(self):
        """Load configuration, handling v1-v5 formats."""
        if not os.path.exists(self.config_path):
            logging.warning(f"Config file not found: {self.config_path}, using defaults")
            return

        try:
            with open(self.config_path, 'r') as f:
                data = json.load(f)
                self._raw_data = data

            version = data.get("version", 1)

            if version >= 5:
                self._load_v5(data)
            elif version >= 4:
                self._load_v4(data)
            elif version >= 3:
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
        """Load v2 config and migrate."""
        self.log_level = data.get("log_level", self.log_level)
        self.feedback_port = data.get("feedback_port", self.feedback_port)
        self.bridge_id = data.get("bridge_id", "")
        self.bridge_name = data.get("bridge_name", "")

        devices_data = data.get("devices", {})
        for slot in self.DEVICE_SLOTS:
            device_data = devices_data.get(slot)
            if device_data and isinstance(device_data, dict):
                self.devices[slot] = DeviceTarget(
                    slot=slot,
                    label=device_data.get("label", slot),
                    usb_phys=device_data.get("usb_phys", ""),
                    target=None  # No target in v2
                )
            else:
                self.devices[slot] = None

        logging.info("Migrated v2 config to v5 format")

    def _load_v3(self, data: Dict):
        """Load v3 per-device channel config (migrate to v5)."""
        self._load_legacy_devices_and_common(data)
        logging.info("Migrated v3 config to v5 format")

    def _load_v4(self, data: Dict):
        """Load v4 config with mode and satellite support (migrate to v5)."""
        self._load_legacy_devices_and_common(data)

        mode = data.get("mode", "broadcast")
        # Migrate broadcast -> direct
        if mode == "broadcast":
            mode = "direct"
        if mode not in VALID_MODES:
            logging.warning(f"Invalid mode '{mode}', defaulting to 'direct'")
            mode = "direct"
        self.mode = mode

        satellite = data.get("satellite", {})
        if isinstance(satellite, dict):
            self.satellite_host = satellite.get("host")
            self.satellite_port = satellite.get("port", DEFAULT_SATELLITE_PORT)

        logging.info("Migrated v4 config to v5 format")

    def _load_legacy_devices_and_common(self, data: Dict):
        """Load common fields and device registrations from v3/v4 (channel-based)."""
        self.log_level = data.get("log_level", self.log_level)
        self.feedback_port = data.get("feedback_port", self.feedback_port)
        self.bridge_id = data.get("bridge_id", "")
        self.bridge_name = data.get("bridge_name", "")

        devices_data = data.get("devices", {})
        for slot in self.DEVICE_SLOTS:
            device_data = devices_data.get(slot)
            if device_data and isinstance(device_data, dict):
                self.devices[slot] = DeviceTarget(
                    slot=slot,
                    label=device_data.get("label", slot),
                    usb_phys=device_data.get("usb_phys", ""),
                    target=None  # Channel-based devices get no target; must be reassigned
                )
            else:
                self.devices[slot] = None

    def _load_v5(self, data: Dict):
        """Load v5 config with per-device targets."""
        self.version = data.get("version", self.CONFIG_VERSION)
        self.log_level = data.get("log_level", self.log_level)
        self.feedback_port = data.get("feedback_port", self.feedback_port)
        self.bridge_id = data.get("bridge_id", "")
        self.bridge_name = data.get("bridge_name", "")

        mode = data.get("mode", "direct")
        if mode not in VALID_MODES:
            logging.warning(f"Invalid mode '{mode}', defaulting to 'direct'")
            mode = "direct"
        self.mode = mode

        satellite = data.get("satellite", {})
        if isinstance(satellite, dict):
            self.satellite_host = satellite.get("host")
            self.satellite_port = satellite.get("port", DEFAULT_SATELLITE_PORT)

        devices_data = data.get("devices", {})
        for slot in self.DEVICE_SLOTS:
            device_data = devices_data.get(slot)
            if device_data and isinstance(device_data, dict):
                target_data = device_data.get("target")
                self.devices[slot] = DeviceTarget(
                    slot=slot,
                    label=device_data.get("label", slot),
                    usb_phys=device_data.get("usb_phys", ""),
                    target=target_data  # {host, port, name?, instance_id?} or None
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

    def register_device(self, slot: str, usb_phys: str, label: str,
                       target: dict | None = None) -> bool:
        """Register a device to a slot with an optional target."""
        if slot not in self.DEVICE_SLOTS:
            logging.error(f"Invalid slot: {slot}")
            return False

        self.devices[slot] = DeviceTarget(
            slot=slot,
            label=label,
            usb_phys=usb_phys,
            target=target
        )

        return self.save()

    def update_device_target(self, slot: str, target: dict | None) -> bool:
        """Update the target for an already-registered device."""
        device = self.devices.get(slot)
        if device is None:
            return False
        device.target = target
        return self.save()

    def unregister_device(self, slot: str) -> bool:
        """Remove a device registration."""
        if slot not in self.DEVICE_SLOTS:
            logging.error(f"Invalid slot: {slot}")
            return False

        self.devices[slot] = None
        return self.save()

    def save(self) -> bool:
        """Save configuration to file in v5 format."""
        try:
            data = {
                "version": self.CONFIG_VERSION,
                "mode": self.mode,
                "feedback_port": self.feedback_port,
                "log_level": self.log_level,
                "bridge_id": self.bridge_id,
                "bridge_name": self.bridge_name,
                "satellite": {
                    "host": self.satellite_host,
                    "port": self.satellite_port,
                },
                "devices": {}
            }

            for slot, device in self.devices.items():
                if device:
                    data["devices"][slot] = {
                        "label": device.label,
                        "usb_phys": device.usb_phys,
                        "target": device.target
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
