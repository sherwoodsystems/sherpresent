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

try:
    from zeroconf import Zeroconf, ServiceBrowser, ServiceInfo, ServiceStateChange
except ImportError:
    Zeroconf = None
    ServiceBrowser = None
    ServiceInfo = None
    ServiceStateChange = None

import uuid as _uuid

from constants import DEFAULT_BROADCAST_PORT, MDNS_SERVICE_TYPE, BRIDGE_VERSION_STRING


class BroadcastSender:
    """
    Sends OSC commands via UDP broadcast with channel addressing.
    Uses subnet broadcast address when available, falls back to 255.255.255.255.
    """

    def __init__(self, channel: str, port: int = DEFAULT_BROADCAST_PORT,
                 peer_discovery: 'PeerDiscovery' = None):
        self.channel = channel
        self.port = port
        self.broadcast_addr = self._get_broadcast_address()
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
        self.peer_discovery = peer_discovery
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
        """Send OSC message to broadcast address and unicast peers, 3x for reliability."""
        try:
            builder = OscMessageBuilder(address=address)
            for arg in args:
                builder.add_arg(arg)
            msg = builder.build()

            # Collect all targets: broadcast + unicast peers
            targets = [(self.broadcast_addr, self.port)]
            if self.peer_discovery:
                for peer_addr, peer_port in self.peer_discovery.get_desktop_peers():
                    targets.append((peer_addr, peer_port))

            # Send 3x to all targets for reliability
            for i in range(3):
                for addr, port in targets:
                    self.socket.sendto(msg.dgram, (addr, port))
                if i < 2:
                    time.sleep(0.010)  # 10ms between retries

            logging.debug(f"Sent (3x) to {len(targets)} target(s): {address}")
        except Exception as e:
            logging.error(f"Broadcast send failed: {e}")

    def close(self):
        """Close the broadcast socket."""
        try:
            self.socket.close()
        except Exception:
            pass


class PeerDiscovery:
    """
    Discovers desktop peers via mDNS browsing.
    Maintains a set of (ip, port) for discovered desktop instances.
    Filters out version=bridge peers (other bridges).
    """

    def __init__(self, broadcast_port: int = DEFAULT_BROADCAST_PORT):
        self.broadcast_port = broadcast_port
        self._peers = {}  # name -> (ip, port)
        self._lock = threading.Lock()
        self._zeroconf = None
        self._browser = None

    def start(self):
        """Start browsing for desktop peers."""
        if Zeroconf is None:
            logging.warning("zeroconf not available, peer discovery disabled")
            return

        try:
            self._zeroconf = Zeroconf()
            self._browser = ServiceBrowser(
                self._zeroconf,
                MDNS_SERVICE_TYPE,
                handlers=[self._on_service_state_change]
            )
            logging.info("mDNS peer discovery started")
        except Exception as e:
            logging.warning(f"Could not start mDNS peer discovery: {e}")

    def _on_service_state_change(self, zeroconf, service_type, name, state_change):
        """Handle mDNS service state changes."""
        if state_change == ServiceStateChange.Added or state_change == ServiceStateChange.Updated:
            info = zeroconf.get_service_info(service_type, name)
            if info is None:
                return

            # Filter out bridge peers
            properties = {k.decode(): v.decode() if isinstance(v, bytes) else v
                         for k, v in info.properties.items()}
            if properties.get("version") == "bridge":
                return

            addresses = info.parsed_addresses()
            if addresses:
                ip = addresses[0]
                # Desktop listens on broadcast port for broadcast commands
                with self._lock:
                    self._peers[name] = (ip, self.broadcast_port)
                logging.debug(f"Discovered desktop peer: {name} at {ip}:{self.broadcast_port}")

        elif state_change == ServiceStateChange.Removed:
            with self._lock:
                if name in self._peers:
                    del self._peers[name]
                    logging.debug(f"Desktop peer removed: {name}")

    def get_desktop_peers(self):
        """Return list of (ip, port) tuples for discovered desktop peers."""
        with self._lock:
            return list(self._peers.values())

    def stop(self):
        """Stop browsing."""
        if self._browser:
            self._browser.cancel()
            self._browser = None
        if self._zeroconf:
            self._zeroconf.close()
            self._zeroconf = None
        logging.debug("mDNS peer discovery stopped")


class MdnsAnnouncer:
    """
    Registers the bridge as a _sher-present._udp.local. mDNS service.
    Desktop instances browsing for this service will see the bridge appear.
    """

    def __init__(self, channel: str, port: int = DEFAULT_BROADCAST_PORT,
                 bridge_id: str = "", bridge_name: str = ""):
        self.channel = channel
        self.port = port
        self._instance_id = bridge_id or str(_uuid.uuid4())
        self._bridge_name = bridge_name or socket.gethostname()
        self._zeroconf = None
        self._service_info = None

    def start(self):
        """Register the mDNS service."""
        if Zeroconf is None or ServiceInfo is None:
            logging.warning("zeroconf not available, mDNS announcer disabled")
            return

        try:
            local_ip = self._get_local_ip()
            hostname = socket.gethostname()
            self._zeroconf = Zeroconf()

            self._service_info = ServiceInfo(
                MDNS_SERVICE_TYPE,
                f"bridge-{self.channel}-{self._instance_id[:8]}.{MDNS_SERVICE_TYPE}",
                port=self.port,
                addresses=[socket.inet_aton(local_ip)] if local_ip else None,
                properties={
                    "version": BRIDGE_VERSION_STRING,
                    "channel": self.channel,
                    "instance": self._instance_id,
                    "name": self._bridge_name,
                },
                server=f"{hostname}.local.",
            )

            self._zeroconf.register_service(self._service_info)
            logging.info(
                f"mDNS service registered: bridge-{self.channel} "
                f"name={self._bridge_name} (instance={self._instance_id[:8]}...)"
            )
        except Exception as e:
            logging.warning(f"Could not register mDNS service: {e}")

    def _get_local_ip(self) -> str:
        """Find non-loopback local IP via UDP connect trick."""
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            s.connect(("8.8.8.8", 80))
            ip = s.getsockname()[0]
            s.close()
            return ip
        except Exception:
            return "127.0.0.1"

    def stop(self):
        """Unregister the mDNS service."""
        if self._zeroconf and self._service_info:
            try:
                self._zeroconf.unregister_service(self._service_info)
            except Exception:
                pass
        if self._zeroconf:
            try:
                self._zeroconf.close()
            except Exception:
                pass
            self._zeroconf = None
        logging.debug(f"mDNS announcer for channel '{self.channel}' stopped")


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
