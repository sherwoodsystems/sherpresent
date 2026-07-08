"""Tests for CompanionSatellite protocol client."""

import socket
import threading
import time
import unittest


class MockCompanionServer:
    """Minimal mock of a Companion Satellite TCP server for testing."""

    def __init__(self):
        self.server_sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.server_sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.server_sock.bind(("127.0.0.1", 0))
        self.server_sock.listen(1)
        self.port = self.server_sock.getsockname()[1]
        self.client_sock = None
        self.received_lines = []
        self._running = False
        self._thread = None
        self._lock = threading.Lock()

    def start(self):
        self._running = True
        self._thread = threading.Thread(target=self._accept_loop, daemon=True)
        self._thread.start()

    def _accept_loop(self):
        self.server_sock.settimeout(5.0)
        try:
            self.client_sock, _ = self.server_sock.accept()
            self.client_sock.settimeout(0.5)
            # Send BEGIN handshake
            self.client_sock.sendall(
                b"BEGIN CompanionVersion=3.4.0 ApiVersion=1.5.0\n"
            )
            # Read lines
            buf = b""
            while self._running:
                try:
                    data = self.client_sock.recv(4096)
                    if not data:
                        break
                    buf += data
                    while b"\n" in buf:
                        line, buf = buf.split(b"\n", 1)
                        decoded = line.decode("ascii", errors="replace").strip()
                        if decoded:
                            with self._lock:
                                self.received_lines.append(decoded)
                except socket.timeout:
                    continue
        except socket.timeout:
            pass

    def get_lines(self):
        with self._lock:
            return list(self.received_lines)

    def send_line(self, line):
        if self.client_sock:
            self.client_sock.sendall((line + "\n").encode("ascii"))

    def stop(self):
        self._running = False
        if self.client_sock:
            try:
                self.client_sock.close()
            except Exception:
                pass
        try:
            self.server_sock.close()
        except Exception:
            pass
        if self._thread:
            self._thread.join(timeout=3.0)


# Add source to path
import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__),
                                "..", "src", "rpi_osc_bridge"))
from satellite import CompanionSatellite


class TestCompanionSatellite(unittest.TestCase):

    def setUp(self):
        self.server = MockCompanionServer()
        self.server.start()
        self.satellite = CompanionSatellite(
            host="127.0.0.1", port=self.server.port
        )

    def tearDown(self):
        self.satellite.close()
        self.server.stop()

    def _wait_connected(self, timeout=5.0):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.satellite.connected:
                return True
            time.sleep(0.1)
        return False

    def _wait_for_line(self, prefix, timeout=5.0):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            for line in self.server.get_lines():
                if line.startswith(prefix):
                    return line
            time.sleep(0.1)
        return None

    def test_handshake_sends_add_device(self):
        self.satellite.start()
        self.assertTrue(self._wait_connected())
        line = self._wait_for_line("ADD-DEVICE")
        self.assertIsNotNone(line, "Expected ADD-DEVICE line")
        self.assertIn("DEVICEID=rpi-osc-bridge", line)
        self.assertIn("KEYS_TOTAL=2", line)
        self.assertIn("BITMAPS=0", line)

    def test_send_next_sends_key_press(self):
        self.satellite.start()
        self.assertTrue(self._wait_connected())
        time.sleep(0.2)

        self.satellite.send_next()
        time.sleep(0.3)

        lines = self.server.get_lines()
        key_lines = [l for l in lines if l.startswith("KEY-PRESS")]
        self.assertTrue(len(key_lines) >= 2, f"Expected 2 KEY-PRESS lines, got {key_lines}")
        self.assertIn("KEY=0 PRESSED=true", key_lines[0])
        self.assertIn("KEY=0 PRESSED=false", key_lines[1])

    def test_send_prev_sends_key_press(self):
        self.satellite.start()
        self.assertTrue(self._wait_connected())
        time.sleep(0.2)

        self.satellite.send_prev()
        time.sleep(0.3)

        lines = self.server.get_lines()
        key_lines = [l for l in lines if l.startswith("KEY-PRESS")]
        self.assertTrue(len(key_lines) >= 2)
        self.assertIn("KEY=1 PRESSED=true", key_lines[0])
        self.assertIn("KEY=1 PRESSED=false", key_lines[1])

    def test_keepalive_sends_ping(self):
        self.satellite.start()
        self.assertTrue(self._wait_connected())
        # Wait for at least one PING cycle (2s interval + margin)
        time.sleep(3.5)

        lines = self.server.get_lines()
        pings = [l for l in lines if l == "PING"]
        self.assertTrue(len(pings) >= 1, f"Expected at least 1 PING, got {pings}")

    def test_responds_to_server_ping(self):
        self.satellite.start()
        self.assertTrue(self._wait_connected())
        time.sleep(0.3)

        self.server.send_line("PING")
        time.sleep(1.0)

        lines = self.server.get_lines()
        pongs = [l for l in lines if l == "PONG"]
        self.assertTrue(len(pongs) >= 1, f"Expected PONG response, got lines: {lines}")

    def test_clean_shutdown_sends_remove_and_quit(self):
        self.satellite.start()
        self.assertTrue(self._wait_connected())
        time.sleep(0.3)

        self.satellite.close()
        time.sleep(0.5)

        lines = self.server.get_lines()
        remove_lines = [l for l in lines if l.startswith("REMOVE-DEVICE")]
        quit_lines = [l for l in lines if l == "QUIT"]
        self.assertTrue(len(remove_lines) >= 1, f"Expected REMOVE-DEVICE, got {lines}")
        self.assertTrue(len(quit_lines) >= 1, f"Expected QUIT, got {lines}")

    def test_reconnects_after_disconnect(self):
        self.satellite.start()
        self.assertTrue(self._wait_connected())

        # Kill the server connection
        self.server.stop()
        time.sleep(1.0)
        self.assertFalse(self.satellite.connected)

        # Start a new server on same port
        server2 = MockCompanionServer()
        # Need to bind to the same port
        server2.server_sock.close()
        server2.server_sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        server2.server_sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        server2.server_sock.bind(("127.0.0.1", self.server.port))
        server2.server_sock.listen(1)
        server2.port = self.server.port
        server2.start()

        # Wait for reconnection (5s reconnect interval + connection time)
        deadline = time.monotonic() + 10.0
        while time.monotonic() < deadline:
            if self.satellite.connected:
                break
            time.sleep(0.5)

        self.assertTrue(self.satellite.connected, "Should reconnect after disconnect")

        # Verify it re-registered
        line = None
        deadline = time.monotonic() + 3.0
        while time.monotonic() < deadline:
            for l in server2.get_lines():
                if l.startswith("ADD-DEVICE"):
                    line = l
                    break
            if line:
                break
            time.sleep(0.2)

        self.assertIsNotNone(line, "Expected ADD-DEVICE after reconnect")
        server2.stop()

    def test_drops_commands_when_disconnected(self):
        # Don't start the satellite (no connection)
        self.satellite._running = True
        self.satellite.send_next()  # Should not raise
        self.satellite.send_prev()  # Should not raise


class TestConfigMigration(unittest.TestCase):
    """Test config v3 -> v4 migration."""

    def test_v3_config_migrates_to_v4_defaults(self):
        import tempfile
        import json

        v3_config = {
            "version": 3,
            "broadcast_port": 9002,
            "feedback_port": 9002,
            "log_level": "INFO",
            "bridge_id": "test-id",
            "bridge_name": "test-bridge",
            "devices": {
                "usb_1": {
                    "label": "Clicker A",
                    "usb_phys": "/usb/1",
                    "channel": "main"
                },
                "usb_2": None,
                "usb_3": None
            }
        }

        with tempfile.NamedTemporaryFile(mode='w', suffix='.json', delete=False) as f:
            json.dump(v3_config, f)
            tmp_path = f.name

        try:
            from config import MultiDeviceConfig
            config = MultiDeviceConfig(config_path=tmp_path)

            # Should default to broadcast mode
            self.assertEqual(config.mode, "broadcast")
            self.assertIsNone(config.satellite_host)
            self.assertEqual(config.satellite_port, 16622)

            # Existing fields preserved
            self.assertEqual(config.broadcast_port, 9002)
            self.assertEqual(config.bridge_name, "test-bridge")
            self.assertIsNotNone(config.devices["usb_1"])
            self.assertEqual(config.devices["usb_1"].channel, "main")

            # Save should write v4
            config.save()
            with open(tmp_path) as f:
                saved = json.load(f)
            self.assertEqual(saved["version"], 4)
            self.assertEqual(saved["mode"], "broadcast")
            self.assertIn("satellite", saved)
        finally:
            os.unlink(tmp_path)


if __name__ == "__main__":
    unittest.main()
