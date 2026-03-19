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
import threading
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse

# Import bridge utilities
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
try:
    from devices import find_keyboards_with_ports
    from config import MultiDeviceConfig
    from osc import DirectSender
    from constants import VALID_MODES, DEFAULT_FEEDBACK_PORT, \
        DEFAULT_SATELLITE_PORT, CONFIG_FILE, REGISTRATION_FILE, \
        FEEDBACK_STATE_FILE, PEERS_STATE_FILE, SATELLITE_STATUS_FILE, DEVICE_SLOTS
    from file_utils import safe_read_json, safe_write_json
except ImportError:
    print("WARNING: Could not import bridge utilities")
    find_keyboards_with_ports = None
    MultiDeviceConfig = None
    DirectSender = None
    safe_read_json = None
    safe_write_json = None
    VALID_MODES = ["direct", "satellite"]
    DEFAULT_FEEDBACK_PORT = 9001
    DEFAULT_SATELLITE_PORT = 16622
    CONFIG_FILE = "/etc/rpi-osc-bridge/config.json"
    REGISTRATION_FILE = "/var/run/rpi-osc-bridge/registration.json"
    FEEDBACK_STATE_FILE = "/var/run/rpi-osc-bridge/feedback.json"
    PEERS_STATE_FILE = "/var/run/rpi-osc-bridge/peers.json"
    SATELLITE_STATUS_FILE = "/var/run/rpi-osc-bridge/satellite.json"
    DEVICE_SLOTS = ["usb_1", "usb_2", "usb_3"]

WEB_ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "web")
PORT = 80


def validate_port(port: int) -> bool:
    """Validate port number."""
    return 1 <= port <= 65535


def get_global_config() -> dict:
    """Get global configuration."""
    if not MultiDeviceConfig:
        return {
            "mode": "direct",
            "feedback_port": DEFAULT_FEEDBACK_PORT,
            "log_level": "INFO",
            "satellite": {"host": None, "port": DEFAULT_SATELLITE_PORT},
            "valid_modes": list(VALID_MODES),
            "error": "MultiDeviceConfig not available"
        }

    try:
        config = MultiDeviceConfig()
        return {
            "mode": config.mode,
            "feedback_port": config.feedback_port,
            "log_level": config.log_level,
            "bridge_id": config.bridge_id,
            "bridge_name": config.bridge_name,
            "satellite": {
                "host": config.satellite_host,
                "port": config.satellite_port,
            },
            "valid_modes": list(VALID_MODES),
        }
    except Exception as e:
        return {
            "mode": "direct",
            "feedback_port": DEFAULT_FEEDBACK_PORT,
            "log_level": "INFO",
            "satellite": {"host": None, "port": DEFAULT_SATELLITE_PORT},
            "valid_modes": list(VALID_MODES),
            "error": str(e)
        }


def save_global_config(feedback_port: int, log_level: str,
                       bridge_name: str = None, mode: str = None,
                       satellite_host: str = None,
                       satellite_port: int = None) -> tuple[bool, str]:
    """Save global configuration."""
    if not MultiDeviceConfig:
        return False, "MultiDeviceConfig not available"

    if not validate_port(feedback_port):
        return False, "Invalid feedback port number"

    if log_level not in ["DEBUG", "INFO", "WARNING", "ERROR"]:
        return False, "Invalid log level"

    if mode is not None and mode not in VALID_MODES:
        return False, f"Invalid mode: {mode}"

    if satellite_port is not None and not validate_port(satellite_port):
        return False, "Invalid satellite port number"

    try:
        config = MultiDeviceConfig()
        config.feedback_port = feedback_port
        config.log_level = log_level
        if bridge_name is not None:
            config.bridge_name = bridge_name
        if mode is not None:
            config.mode = mode
        if satellite_host is not None:
            config.satellite_host = satellite_host if satellite_host else None
        if satellite_port is not None:
            config.satellite_port = satellite_port

        if config.save():
            try:
                subprocess.run(
                    ["systemctl", "restart", "rpi-osc-bridge"],
                    check=True,
                    capture_output=True
                )
            except (subprocess.CalledProcessError, FileNotFoundError):
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


def get_peers() -> dict:
    """Read discovered desktop peers from file."""
    try:
        if os.path.exists(PEERS_STATE_FILE):
            with open(PEERS_STATE_FILE, 'r') as f:
                return json.load(f)
    except Exception as e:
        print(f"Error reading peers: {e}", file=sys.stderr)

    return {"peers": {}, "updated_at": None}


def get_satellite_status() -> dict:
    """Read satellite connection status from file."""
    try:
        if os.path.exists(SATELLITE_STATUS_FILE):
            with open(SATELLITE_STATUS_FILE, 'r') as f:
                return json.load(f)
    except Exception as e:
        print(f"Error reading satellite status: {e}", file=sys.stderr)

    return {
        "connected": False,
        "host": None,
        "port": None,
        "companion_version": None,
        "api_version": None,
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
    except FileNotFoundError:
        return ["(journalctl not available in this environment)"]
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
    """Get registered devices from config (v5 format with per-device targets)."""
    if not MultiDeviceConfig:
        return {"devices": {}, "error": "MultiDeviceConfig not available"}

    try:
        config = MultiDeviceConfig()
        result = {}
        for slot in DEVICE_SLOTS:
            device = config.devices.get(slot)
            if device:
                result[slot] = {
                    "label": device.label,
                    "usb_phys": device.usb_phys,
                    "target": device.target
                }
            else:
                result[slot] = None
        return {
            "devices": result,
            "mode": config.mode,
            "log_level": config.log_level,
            "feedback_port": config.feedback_port,
            "bridge_id": config.bridge_id,
            "bridge_name": config.bridge_name,
            "valid_modes": list(VALID_MODES),
        }
    except Exception as e:
        print(f"Error loading registered devices: {e}", file=sys.stderr)
        return {"devices": {}, "error": str(e)}


def start_registration(slot: str) -> tuple[bool, str]:
    """Start registration mode for a device slot."""
    if slot not in DEVICE_SLOTS:
        return False, f"Invalid slot: {slot}"

    try:
        data = {
            "active": True,
            "target_slot": slot,
            "detected_phys": None,
            "detected_name": None,
            "started_at": None
        }

        safe_write_json(REGISTRATION_FILE, data)

        return True, f"Registration started for {slot}"

    except Exception as e:
        return False, str(e)


def get_registration_status() -> dict:
    """Get current registration status."""
    try:
        data = safe_read_json(REGISTRATION_FILE)
        if data is not None:
            return data
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


def confirm_registration(slot: str, usb_phys: str,
                        label: str, target: dict | None = None) -> tuple[bool, str]:
    """Confirm device registration with optional target assignment."""
    if not MultiDeviceConfig:
        return False, "MultiDeviceConfig not available"

    if slot not in DEVICE_SLOTS:
        return False, f"Invalid slot: {slot}"

    try:
        config = MultiDeviceConfig()
        success = config.register_device(slot, usb_phys, label, target=target)

        if success:
            cancel_registration()

            try:
                subprocess.run(
                    ["systemctl", "restart", "rpi-osc-bridge"],
                    check=True,
                    capture_output=True
                )
            except (subprocess.CalledProcessError, FileNotFoundError):
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
            except (subprocess.CalledProcessError, FileNotFoundError):
                return True, "Device unregistered but service restart failed"

            return True, "Device unregistered successfully"
        else:
            return False, "Failed to unregister device"

    except Exception as e:
        return False, str(e)


def update_device_target(slot: str, target: dict | None) -> tuple[bool, str]:
    """Update the target for a registered device."""
    if not MultiDeviceConfig:
        return False, "MultiDeviceConfig not available"

    if slot not in DEVICE_SLOTS:
        return False, f"Invalid slot: {slot}"

    try:
        config = MultiDeviceConfig()
        success = config.update_device_target(slot, target)

        if success:
            try:
                subprocess.run(
                    ["systemctl", "restart", "rpi-osc-bridge"],
                    check=True,
                    capture_output=True
                )
            except (subprocess.CalledProcessError, FileNotFoundError):
                return True, "Target updated but service restart failed"

            return True, "Target updated"
        else:
            return False, "Device not found in slot"

    except Exception as e:
        return False, str(e)


def send_test_command(slot: str, command: str) -> tuple[bool, str]:
    """Send test OSC command for a specific device's target."""
    if not MultiDeviceConfig:
        return False, "MultiDeviceConfig not available"

    if not DirectSender:
        return False, "DirectSender not available"

    try:
        config = MultiDeviceConfig()
        device = config.devices.get(slot)

        if not device:
            return False, f"Device {slot} not registered"

        if not device.target:
            return False, f"Device {slot} has no target assigned"

        sender = DirectSender(host=device.target["host"], port=device.target["port"])

        if command == "next":
            sender.send_next()
        elif command == "prev":
            sender.send_prev()
        else:
            sender.close()
            return False, f"Unknown command: {command}"

        sender.close()
        return True, f"Sent {command} to {device.target.get('name', device.target['host'])}:{device.target['port']}"

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

        elif path == "/peers":
            self._send_json(get_peers())

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

        elif path == "/satellite/status":
            self._send_json(get_satellite_status())

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
                label = data.get("label", slot)
                target = data.get("target")  # {host, port, name?, instance_id?} or None

                success, message = confirm_registration(slot, usb_phys, label, target=target)
                self._send_json(
                    {"success": success, "message": message},
                    200 if success else 400
                )
            except json.JSONDecodeError:
                self.send_error(400, "Invalid JSON")
            except Exception as e:
                self.send_error(500, f"Server error: {e}")

        # Update device target
        elif path.startswith("/devices/") and path.endswith("/target"):
            try:
                parts = path.split("/")
                slot = parts[2] if len(parts) >= 3 else None

                data = self._read_json_body()
                target = data.get("target")  # {host, port, name?, instance_id?} or None

                if slot not in DEVICE_SLOTS:
                    self._send_json({"success": False, "message": f"Invalid slot: {slot}"}, 400)
                    return

                # Validate target if provided
                if target is not None:
                    if not isinstance(target, dict) or "host" not in target or "port" not in target:
                        self._send_json({"success": False, "message": "Target must have host and port"}, 400)
                        return
                    if not validate_port(target["port"]):
                        self._send_json({"success": False, "message": "Invalid target port"}, 400)
                        return

                success, message = update_device_target(slot, target)
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

        # Test command for a specific device's target
        elif path.startswith("/devices/") and "/test/" in path:
            try:
                parts = path.split("/")
                slot = parts[2] if len(parts) >= 4 else None
                cmd = parts[4] if len(parts) >= 5 else None

                if cmd not in ["next", "prev"]:
                    self.send_error(400, "Invalid test command")
                    return

                success, message = send_test_command(slot, cmd)
                self._send_json(
                    {"success": success, "message": message},
                    200 if success else 400
                )
            except Exception as e:
                self.send_error(500, f"Server error: {e}")

        # Save global configuration
        elif path == "/config/global":
            try:
                data = self._read_json_body()

                feedback_port = data.get("feedback_port", DEFAULT_FEEDBACK_PORT)
                log_level = data.get("log_level", "INFO")
                bridge_name = data.get("bridge_name")
                mode = data.get("mode")

                satellite = data.get("satellite", {})
                satellite_host = satellite.get("host") if isinstance(satellite, dict) else None
                satellite_port = satellite.get("port") if isinstance(satellite, dict) else None

                success, message = save_global_config(
                    feedback_port, log_level, bridge_name,
                    mode=mode, satellite_host=satellite_host,
                    satellite_port=satellite_port
                )
                self._send_json(
                    {"success": success, "message": message},
                    200 if success else 400
                )
            except json.JSONDecodeError:
                self.send_error(400, "Invalid JSON")
            except Exception as e:
                self.send_error(500, f"Server error: {e}")

        # Shutdown the Pi
        elif path == "/shutdown":
            self._send_json({"success": True, "message": "Shutting down..."})
            # Delay shutdown so HTTP response is delivered before the process dies
            threading.Timer(1.0, lambda: subprocess.Popen(["sudo", "shutdown", "-h", "now"])).start()

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
