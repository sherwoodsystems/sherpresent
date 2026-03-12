"""OSC broadcast sender and feedback listener for rpi-osc-bridge."""

import os
import json
import time
import fcntl
import logging
import threading
from datetime import datetime, timezone
from typing import Dict

try:
    from pythonosc import dispatcher
    from pythonosc import osc_server
    from pythonosc.osc_message_builder import OscMessageBuilder
except ImportError:
    print("ERROR: python-osc not installed. Run: sudo pip3 install python-osc")
    import sys
    sys.exit(1)

try:
    import socket
    import struct
    import netifaces
except ImportError:
    netifaces = None

from constants import DEFAULT_BROADCAST_PORT


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

                    if ip.startswith('127.') or ip.startswith('169.254.'):
                        continue

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


class FeedbackListener:
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
            "peers": []
        }
        self._last_error_logged = 0

        self.dispatcher = dispatcher.Dispatcher()
        self.dispatcher.map(f"/clicker/{channel}/state/presenting", self.handle_presenting)
        self.dispatcher.map(f"/clicker/{channel}/state/open", self.handle_open)
        self.dispatcher.map(f"/clicker/{channel}/state/slide", self.handle_slide)
        self.dispatcher.map(f"/clicker/{channel}/state/zoom", self.handle_zoom)
        self.dispatcher.map("/clicker/*/state/*", self.handle_any_state)

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
        if args:
            self.update_state({"presenting": bool(args[0])})

    def handle_open(self, address, *args):
        if args:
            self.update_state({"open": bool(args[0])})

    def handle_slide(self, address, *args):
        if len(args) >= 2:
            self.update_state({
                "current_slide": int(args[0]),
                "total_slides": int(args[1])
            })
        elif args:
            self.update_state({"current_slide": int(args[0])})

    def handle_zoom(self, address, *args):
        if args:
            self.update_state({"zoom_level": int(args[0])})

    def handle_any_state(self, address, *args):
        """Handle any state message for peer discovery."""
        parts = address.split('/')
        if len(parts) >= 4 and parts[1] == 'clicker' and parts[3] == 'state':
            peer_channel = parts[2]
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
        logging.debug(f"Feedback listener on port {self.port} for channel '{self.channel}'")

    def _run(self):
        try:
            self.server.serve_forever()
        except Exception as e:
            logging.error(f"Feedback listener error: {e}")

    def stop(self):
        """Stop listening for feedback."""
        if not self.running:
            return

        self.running = False
        self.server.shutdown()
        if self.thread:
            self.thread.join(timeout=2.0)
        logging.debug("Feedback listener stopped")
