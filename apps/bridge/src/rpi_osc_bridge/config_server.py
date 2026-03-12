#!/usr/bin/env python3
"""
SherPresent OSC Bridge - Web Configuration Server
Simple HTTP server for configuring the bridge via web UI
"""

import os
import sys
import json
import signal
import subprocess
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse

# Import bridge utilities
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
try:
    from devices import find_keyboards_with_ports
    from config import MultiDeviceConfig
    from osc import BroadcastSender
    from constants import VALID_CHANNELS, DEFAULT_BROADCAST_PORT, CONFIG_FILE, \
        REGISTRATION_FILE, FEEDBACK_STATE_FILE, DEVICE_SLOTS
except ImportError:
    print("WARNING: Could not import bridge utilities")
    find_keyboards_with_ports = None
    MultiDeviceConfig = None
    BroadcastSender = None
    VALID_CHANNELS = ["main", "backup"]
    DEFAULT_BROADCAST_PORT = 9002
    CONFIG_FILE = "/etc/rpi-osc-bridge/config.json"
    REGISTRATION_FILE = "/var/run/rpi-osc-bridge/registration.json"
    FEEDBACK_STATE_FILE = "/var/run/rpi-osc-bridge/feedback.json"
    DEVICE_SLOTS = ["usb_1", "usb_2", "usb_3"]

WEB_ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "web")
PORT = 80


def validate_port(port: int) -> bool:
    """Validate port number."""
    return 1 <= port <= 65535


def validate_channel(channel: str) -> bool:
    """Validate channel name."""
    return channel in VALID_CHANNELS


def get_global_config() -> dict:
    """Get global configuration (ports, log level, valid channels)."""
    if not MultiDeviceConfig:
        return {
            "broadcast_port": DEFAULT_BROADCAST_PORT,
            "feedback_port": DEFAULT_BROADCAST_PORT,
            "log_level": "INFO",
            "valid_channels": list(VALID_CHANNELS),
            "error": "MultiDeviceConfig not available"
        }

    try:
        config = MultiDeviceConfig()
        return {
            "broadcast_port": config.broadcast_port,
            "feedback_port": config.feedback_port,
            "log_level": config.log_level,
            "valid_channels": list(VALID_CHANNELS)
        }
    except Exception as e:
        return {
            "broadcast_port": DEFAULT_BROADCAST_PORT,
            "feedback_port": DEFAULT_BROADCAST_PORT,
            "log_level": "INFO",
            "valid_channels": list(VALID_CHANNELS),
            "error": str(e)
        }


def save_global_config(broadcast_port: int, feedback_port: int, log_level: str) -> tuple[bool, str]:
    """Save global configuration (ports, log level)."""
    if not MultiDeviceConfig:
        return False, "MultiDeviceConfig not available"

    if not validate_port(broadcast_port):
        return False, "Invalid broadcast port number"

    if not validate_port(feedback_port):
        return False, "Invalid feedback port number"

    if log_level not in ["DEBUG", "INFO", "WARNING", "ERROR"]:
        return False, "Invalid log level"

    try:
        config = MultiDeviceConfig()
        config.broadcast_port = broadcast_port
        config.feedback_port = feedback_port
        config.log_level = log_level

        if config.save():
            try:
                subprocess.run(
                    ["systemctl", "restart", "rpi-osc-bridge"],
                    check=True,
                    capture_output=True
                )
            except subprocess.CalledProcessError:
                return True, "Config saved but service restart failed"

            return True, "Configuration saved"
        else:
            return False, "Failed to save configuration"

    except Exception as e:
        return False, str(e)


def get_service_status() -> bool:
    """Check if rpi-osc-bridge service is running."""
    try:
        result = subprocess.run(
            ["systemctl", "is-active", "rpi-osc-bridge"],
            capture_output=True,
            text=True
        )
        return result.returncode == 0 and result.stdout.strip() == "active"
    except Exception:
        return False


def get_feedback_state() -> dict:
    """Read OSC feedback state from file."""
    try:
        if os.path.exists(FEEDBACK_STATE_FILE):
            with open(FEEDBACK_STATE_FILE, 'r') as f:
                return json.load(f)
    except Exception as e:
        print(f"Error reading feedback: {e}", file=sys.stderr)

    return {
        "presenting": False,
        "open": False,
        "current_slide": 0,
        "total_slides": 0,
        "zoom_level": 0,
        "last_updated": None,
        "last_command": None,
        "last_command_time": None
    }


def get_recent_logs(lines: int = 50) -> list:
    """Get recent log lines from bridge service."""
    try:
        result = subprocess.run(
            ["journalctl", "-u", "rpi-osc-bridge", "-n", str(lines), "--no-pager"],
            capture_output=True,
            text=True,
            timeout=5
        )
        if result.returncode == 0:
            log_lines = result.stdout.strip().split('\n')
            return [line for line in log_lines if line.strip()]
    except Exception as e:
        print(f"Error reading logs: {e}", file=sys.stderr)

    return []


def get_devices() -> list:
    """Get list of connected keyboards with USB port info."""
    if not find_keyboards_with_ports:
        return []

    try:
        return find_keyboards_with_ports()
    except Exception as e:
        print(f"Error listing devices: {e}", file=sys.stderr)
        return []


def get_registered_devices() -> dict:
    """Get registered devices from config (v3 format with per-device channels)."""
    if not MultiDeviceConfig:
        return {"devices": {}, "error": "MultiDeviceConfig not available"}

    try:
        config = MultiDeviceConfig()
        result = {}
        for slot in DEVICE_SLOTS:
            target = config.devices.get(slot)
            if target:
                result[slot] = {
                    "label": target.label,
                    "usb_phys": target.usb_phys,
                    "channel": target.channel
                }
            else:
                result[slot] = None
        return {
            "devices": result,
            "log_level": config.log_level,
            "feedback_port": config.feedback_port,
            "broadcast_port": config.broadcast_port,
            "valid_channels": list(VALID_CHANNELS)
        }
    except Exception as e:
        print(f"Error loading registered devices: {e}", file=sys.stderr)
        return {"devices": {}, "error": str(e)}


def start_registration(slot: str) -> tuple[bool, str]:
    """Start registration mode for a device slot."""
    if slot not in DEVICE_SLOTS:
        return False, f"Invalid slot: {slot}"

    try:
        os.makedirs(os.path.dirname(REGISTRATION_FILE), exist_ok=True)

        data = {
            "active": True,
            "target_slot": slot,
            "detected_phys": None,
            "detected_name": None,
            "started_at": None
        }

        with open(REGISTRATION_FILE, 'w') as f:
            json.dump(data, f, indent=2)

        return True, f"Registration started for {slot}"

    except Exception as e:
        return False, str(e)


def get_registration_status() -> dict:
    """Get current registration status."""
    try:
        if os.path.exists(REGISTRATION_FILE):
            with open(REGISTRATION_FILE, 'r') as f:
                return json.load(f)
    except Exception as e:
        print(f"Error reading registration status: {e}", file=sys.stderr)

    return {
        "active": False,
        "target_slot": None,
        "detected_phys": None,
        "detected_name": None
    }


def cancel_registration() -> tuple[bool, str]:
    """Cancel registration mode."""
    try:
        if os.path.exists(REGISTRATION_FILE):
            os.remove(REGISTRATION_FILE)
        return True, "Registration cancelled"
    except Exception as e:
        return False, str(e)


def confirm_registration(slot: str, usb_phys: str, channel: str,
                        label: str) -> tuple[bool, str]:
    """Confirm device registration with channel assignment."""
    if not MultiDeviceConfig:
        return False, "MultiDeviceConfig not available"

    if slot not in DEVICE_SLOTS:
        return False, f"Invalid slot: {slot}"

    if not validate_channel(channel):
        return False, f"Invalid channel: {channel}"

    try:
        config = MultiDeviceConfig()
        success = config.register_device(slot, usb_phys, channel, label)

        if success:
            cancel_registration()

            try:
                subprocess.run(
                    ["systemctl", "restart", "rpi-osc-bridge"],
                    check=True,
                    capture_output=True
                )
            except subprocess.CalledProcessError:
                return True, "Device registered but service restart failed"

            return True, "Device registered successfully"
        else:
            return False, "Failed to save registration"

    except Exception as e:
        return False, str(e)


def unregister_device(slot: str) -> tuple[bool, str]:
    """Unregister a device from a slot."""
    if not MultiDeviceConfig:
        return False, "MultiDeviceConfig not available"

    if slot not in DEVICE_SLOTS:
        return False, f"Invalid slot: {slot}"

    try:
        config = MultiDeviceConfig()
        success = config.unregister_device(slot)

        if success:
            try:
                subprocess.run(
                    ["systemctl", "restart", "rpi-osc-bridge"],
                    check=True,
                    capture_output=True
                )
            except subprocess.CalledProcessError:
                return True, "Device unregistered but service restart failed"

            return True, "Device unregistered successfully"
        else:
            return False, "Failed to unregister device"

    except Exception as e:
        return False, str(e)


def send_test_broadcast(slot: str, command: str) -> tuple[bool, str]:
    """Send test broadcast command for a specific device's channel."""
    if not MultiDeviceConfig:
        return False, "MultiDeviceConfig not available"

    if not BroadcastSender:
        return False, "BroadcastSender not available"

    try:
        config = MultiDeviceConfig()
        target = config.devices.get(slot)

        if not target:
            return False, f"Device {slot} not registered"

        sender = BroadcastSender(channel=target.channel, port=config.broadcast_port)

        if command == "next":
            sender.send_next()
        elif command == "prev":
            sender.send_prev()
        else:
            sender.close()
            return False, f"Unknown command: {command}"

        sender.close()
        return True, f"Broadcast {command} on channel '{target.channel}'"

    except Exception as e:
        return False, str(e)


class ConfigServerHandler(BaseHTTPRequestHandler):
    """HTTP request handler for configuration server."""

    def log_message(self, format, *args):
        print(f"[{self.address_string()}] {format % args}")

    def _send_json(self, data, status=200):
        """Send a JSON response."""
        response = json.dumps(data).encode('utf-8')
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", len(response))
        self.end_headers()
        self.wfile.write(response)

    def _read_json_body(self) -> dict:
        """Read and parse JSON request body."""
        content_length = int(self.headers.get('Content-Length', 0))
        body = self.rfile.read(content_length)
        return json.loads(body.decode('utf-8'))

    def do_GET(self):
        """Handle GET requests."""
        parsed = urlparse(self.path)
        path = parsed.path

        if path == "/" or path == "/index.html":
            try:
                html_path = os.path.join(WEB_ROOT, "index.html")
                with open(html_path, 'rb') as f:
                    content = f.read()

                self.send_response(200)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Content-Length", len(content))
                self.end_headers()
                self.wfile.write(content)
            except Exception as e:
                self.send_error(500, f"Failed to load page: {e}")

        elif path == "/status":
            self._send_json({"running": get_service_status()})

        elif path == "/feedback":
            self._send_json(get_feedback_state())

        elif path == "/logs":
            self._send_json({"logs": get_recent_logs()})

        elif path == "/devices":
            self._send_json({"devices": get_devices()})

        elif path == "/devices/registered":
            self._send_json(get_registered_devices())

        elif path == "/registration/status":
            self._send_json(get_registration_status())

        elif path == "/config/global":
            self._send_json(get_global_config())

        else:
            self.send_error(404, "Not found")

    def do_POST(self):
        """Handle POST requests."""
        parsed = urlparse(self.path)
        path = parsed.path

        # Start registration for a slot
        if path == "/registration/start":
            try:
                data = self._read_json_body()
                slot = data.get("slot")
                success, message = start_registration(slot)
                self._send_json(
                    {"success": success, "message": message},
                    200 if success else 400
                )
            except json.JSONDecodeError:
                self.send_error(400, "Invalid JSON")
            except Exception as e:
                self.send_error(500, f"Server error: {e}")

        # Cancel registration
        elif path == "/registration/cancel":
            try:
                success, message = cancel_registration()
                self._send_json({"success": success, "message": message})
            except Exception as e:
                self.send_error(500, f"Server error: {e}")

        # Confirm registration
        elif path == "/registration/confirm":
            try:
                data = self._read_json_body()
                slot = data.get("slot")
                usb_phys = data.get("usb_phys")
                channel = data.get("channel", "main")
                label = data.get("label", slot)

                success, message = confirm_registration(slot, usb_phys, channel, label)
                self._send_json(
                    {"success": success, "message": message},
                    200 if success else 400
                )
            except json.JSONDecodeError:
                self.send_error(400, "Invalid JSON")
            except Exception as e:
                self.send_error(500, f"Server error: {e}")

        # Unregister a device
        elif path.startswith("/devices/") and path.endswith("/unregister"):
            try:
                parts = path.split("/")
                slot = parts[2] if len(parts) >= 3 else None

                success, message = unregister_device(slot)
                self._send_json(
                    {"success": success, "message": message},
                    200 if success else 400
                )
            except Exception as e:
                self.send_error(500, f"Server error: {e}")

        # Test broadcast for a specific device's channel
        elif path.startswith("/devices/") and "/test/" in path:
            try:
                parts = path.split("/")
                slot = parts[2] if len(parts) >= 4 else None
                cmd = parts[4] if len(parts) >= 5 else None

                if cmd not in ["next", "prev"]:
                    self.send_error(400, "Invalid test command")
                    return

                success, message = send_test_broadcast(slot, cmd)
                self._send_json(
                    {"success": success, "message": message},
                    200 if success else 400
                )
            except Exception as e:
                self.send_error(500, f"Server error: {e}")

        # Save global configuration (ports, log level)
        elif path == "/config/global":
            try:
                data = self._read_json_body()

                broadcast_port = data.get("broadcast_port", DEFAULT_BROADCAST_PORT)
                feedback_port = data.get("feedback_port", DEFAULT_BROADCAST_PORT)
                log_level = data.get("log_level", "INFO")

                success, message = save_global_config(broadcast_port, feedback_port, log_level)
                self._send_json(
                    {"success": success, "message": message},
                    200 if success else 400
                )
            except json.JSONDecodeError:
                self.send_error(400, "Invalid JSON")
            except Exception as e:
                self.send_error(500, f"Server error: {e}")

        else:
            self.send_error(404, "Not found")


def main():
    """Entry point."""
    print("=" * 50)
    print("RPi OSC Bridge - Configuration Server")
    print("=" * 50)

    if not os.path.exists(WEB_ROOT):
        print(f"ERROR: Web directory not found: {WEB_ROOT}")
        print("Make sure index.html exists in the web/ directory")
        sys.exit(1)

    server = HTTPServer(('0.0.0.0', PORT), ConfigServerHandler)

    print(f"Server running on port {PORT}")
    print(f"Web root: {WEB_ROOT}")
    print(f"Config file: {CONFIG_FILE}")
    print("")
    print("Access the web interface:")
    print(f"  http://localhost/")
    print(f"  http://<raspberry-pi-ip>/")
    print("")
    print("Press Ctrl+C to stop")
    print("=" * 50)

    def signal_handler(signum, frame):
        print("\nShutting down server...")
        server.shutdown()
        sys.exit(0)

    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down server...")
        server.shutdown()


if __name__ == "__main__":
    main()
