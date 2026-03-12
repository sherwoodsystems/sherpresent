"""Companion Satellite protocol client for rpi-osc-bridge.

Registers as a 2-button virtual surface in Bitfocus Companion via the
Satellite TCP protocol (ASCII line protocol on port 16622).

Key mapping: KEY=0 = next, KEY=1 = prev.
"""

import json
import logging
import os
import socket
import threading
import time
from typing import Optional


SATELLITE_STATUS_FILE = "/var/run/rpi-osc-bridge/satellite.json"

# Key indices on the virtual surface
KEY_NEXT = 0
KEY_PREV = 1

DEVICE_ID = "rpi-osc-bridge"
PRODUCT_NAME = "USB Clicker"
KEYS_TOTAL = 2
KEYS_PER_ROW = 2
PING_INTERVAL = 2.0
RECONNECT_INTERVAL = 5.0


class CompanionSatellite:
    """Companion Satellite TCP client presenting a 2-button virtual surface.

    Duck-typed to match BroadcastSender interface: send_next(), send_prev(), close().
    Thread-safe: main thread calls send_next/send_prev, background thread handles
    recv loop and keepalive pings.
    """

    def __init__(self, host: str, port: int = 16622):
        self.host = host
        self.port = port

        self._sock: Optional[socket.socket] = None
        self._connected = False
        self._running = False
        self._write_lock = threading.Lock()
        self._thread: Optional[threading.Thread] = None
        self._companion_version: Optional[str] = None
        self._api_version: Optional[str] = None

    def start(self):
        """Start the background thread for connection, recv loop, and keepalive."""
        if self._running:
            return
        self._running = True
        self._thread = threading.Thread(target=self._run_loop, daemon=True)
        self._thread.start()
        logging.info(f"Companion satellite starting, target {self.host}:{self.port}")

    def _run_loop(self):
        """Background thread: connect, recv, ping, reconnect."""
        while self._running:
            if not self._connected:
                self._try_connect()
                if not self._connected:
                    self._write_status()
                    time.sleep(RECONNECT_INTERVAL)
                    continue

            try:
                self._recv_and_ping()
            except Exception as e:
                logging.warning(f"Satellite connection error: {e}")
                self._disconnect()

    def _try_connect(self):
        """Attempt TCP connection and perform handshake."""
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.settimeout(5.0)
            sock.connect((self.host, self.port))
            sock.settimeout(1.0)

            # Read BEGIN line
            line = self._readline(sock)
            if not line or not line.startswith("BEGIN "):
                logging.warning(f"Unexpected handshake: {line!r}")
                sock.close()
                return

            # Parse BEGIN CompanionVersion=X.X.X ApiVersion=X.X.X
            params = self._parse_params(line[len("BEGIN "):])
            self._companion_version = params.get("CompanionVersion")
            self._api_version = params.get("ApiVersion")
            logging.info(
                f"Connected to Companion {self._companion_version} "
                f"(API {self._api_version})"
            )

            self._sock = sock
            self._connected = True

            # Register device
            self._send_line(
                f"ADD-DEVICE DEVICEID={DEVICE_ID} "
                f'PRODUCT_NAME="{PRODUCT_NAME}" '
                f"KEYS_TOTAL={KEYS_TOTAL} KEYS_PER_ROW={KEYS_PER_ROW} "
                f"BITMAPS=0 COLORS=false TEXT=false"
            )
            logging.info(f"Registered satellite device: {DEVICE_ID}")
            self._write_status()

        except (OSError, ConnectionRefusedError, TimeoutError) as e:
            logging.debug(f"Satellite connect failed: {e}")
            self._connected = False

    def _recv_and_ping(self):
        """Receive loop with interleaved keepalive pings."""
        last_ping = time.monotonic()

        while self._running and self._connected:
            # Try to read a line (1s timeout)
            try:
                line = self._readline(self._sock)
                if line is None:
                    # Connection closed
                    logging.warning("Companion closed connection")
                    self._disconnect()
                    return
                if line:
                    self._handle_line(line)
            except socket.timeout:
                pass
            except OSError:
                self._disconnect()
                return

            # Send PING if due
            now = time.monotonic()
            if now - last_ping >= PING_INTERVAL:
                try:
                    self._send_line("PING")
                except OSError:
                    self._disconnect()
                    return
                last_ping = now

    def _handle_line(self, line: str):
        """Handle a received line from Companion."""
        if line.startswith("PING"):
            try:
                self._send_line("PONG")
            except OSError:
                self._disconnect()
        elif line.startswith("KEY-STATE") or line.startswith("KEYS-CLEAR") or \
                line.startswith("BRIGHTNESS"):
            # No display — ignore silently
            pass
        elif line.startswith("PONG"):
            pass
        else:
            logging.debug(f"Satellite recv: {line}")

    def _send_line(self, line: str):
        """Send a line to Companion. Must hold _write_lock or be in background thread."""
        with self._write_lock:
            if self._sock:
                self._sock.sendall((line + "\n").encode("ascii"))

    def _readline(self, sock: socket.socket) -> Optional[str]:
        """Read one \\n-terminated line. Returns None on EOF, '' on timeout."""
        buf = b""
        while True:
            try:
                chunk = sock.recv(1)
                if not chunk:
                    return None  # EOF
                if chunk == b"\n":
                    return buf.decode("ascii", errors="replace").strip()
                buf += chunk
            except socket.timeout:
                return ""

    def _parse_params(self, text: str) -> dict:
        """Parse KEY=VALUE pairs from a protocol line."""
        params = {}
        # Simple parser: split on spaces, handle quoted values
        parts = text.strip().split()
        for part in parts:
            if "=" in part:
                key, _, value = part.partition("=")
                params[key] = value.strip('"')
        return params

    def _disconnect(self):
        """Close socket and mark disconnected."""
        self._connected = False
        if self._sock:
            try:
                self._sock.close()
            except Exception:
                pass
            self._sock = None
        self._write_status()

    def _send_key_press(self, key: int):
        """Send a key press then release for the given key index."""
        if not self._connected:
            logging.warning(f"Satellite not connected, dropping key press {key}")
            return
        try:
            self._send_line(
                f"KEY-PRESS DEVICEID={DEVICE_ID} KEY={key} PRESSED=true"
            )
            self._send_line(
                f"KEY-PRESS DEVICEID={DEVICE_ID} KEY={key} PRESSED=false"
            )
        except OSError as e:
            logging.error(f"Failed to send key press: {e}")
            self._disconnect()

    def send_next(self):
        """Send next slide key press (KEY=0)."""
        self._send_key_press(KEY_NEXT)

    def send_prev(self):
        """Send previous slide key press (KEY=1)."""
        self._send_key_press(KEY_PREV)

    def close(self):
        """Cleanly disconnect from Companion."""
        self._running = False
        if self._connected and self._sock:
            try:
                self._send_line(f"REMOVE-DEVICE DEVICEID={DEVICE_ID}")
                self._send_line("QUIT")
            except OSError:
                pass
        self._disconnect()
        if self._thread:
            self._thread.join(timeout=3.0)
        self._write_status()
        logging.info("Companion satellite stopped")

    def _write_status(self):
        """Write satellite connection status to file for the web UI."""
        status = {
            "connected": self._connected,
            "host": self.host,
            "port": self.port,
            "companion_version": self._companion_version,
            "api_version": self._api_version,
            "device_id": DEVICE_ID,
            "keys_total": KEYS_TOTAL,
        }
        try:
            os.makedirs(os.path.dirname(SATELLITE_STATUS_FILE), exist_ok=True)
            tmp = SATELLITE_STATUS_FILE + ".tmp"
            with open(tmp, "w") as f:
                json.dump(status, f, indent=2)
            os.rename(tmp, SATELLITE_STATUS_FILE)
        except Exception as e:
            logging.debug(f"Could not write satellite status: {e}")

    @property
    def connected(self) -> bool:
        return self._connected
