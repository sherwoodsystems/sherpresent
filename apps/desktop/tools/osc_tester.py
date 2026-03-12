#!/usr/bin/env python3
"""
OSC Broadcast Tester for sher-present

Sends broadcast OSC commands to test the app without a presentation.

Usage:
    python osc_tester.py                    # Interactive mode
    python osc_tester.py main next          # Send next to 'main' channel
    python osc_tester.py backup prev        # Send prev to 'backup' channel
    python osc_tester.py main goto 5        # Go to slide 5 on 'main'
    python osc_tester.py main status        # Request status on 'main'

Requirements:
    pip install python-osc
    pip install netifaces  # Optional, for better broadcast address detection
"""

import sys
import socket
import struct
import argparse

try:
    from pythonosc.osc_message_builder import OscMessageBuilder
except ImportError:
    print("ERROR: python-osc not installed. Run: pip install python-osc")
    sys.exit(1)

# Valid channel names (must match VALID_CHANNELS in types.ts)
VALID_CHANNELS = [
    "main", "backup",
    "keynote1", "keynote2", "keynote3", "keynote4", "keynote5",
    "keynote6", "keynote7", "keynote8", "keynote9",
    "aux1", "aux2", "aux3", "aux4", "aux5",
    "aux6", "aux7", "aux8", "aux9",
]

DEFAULT_PORT = 9002


def get_broadcast_address():
    """Calculate subnet broadcast address from local network interface."""
    try:
        import netifaces
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
                # Calculate broadcast: IP | ~netmask
                ip_int = struct.unpack('!I', socket.inet_aton(ip))[0]
                mask_int = struct.unpack('!I', socket.inet_aton(netmask))[0]
                broadcast_int = ip_int | (~mask_int & 0xFFFFFFFF)
                return socket.inet_ntoa(struct.pack('!I', broadcast_int))
    except ImportError:
        pass
    except Exception as e:
        print(f"Warning: Could not detect broadcast address: {e}")
    return "255.255.255.255"


class OscTester:
    """Simple OSC broadcast tester."""

    def __init__(self, port=DEFAULT_PORT):
        self.port = port
        self.broadcast_addr = get_broadcast_address()
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
        print(f"OSC Tester ready")
        print(f"  Broadcast address: {self.broadcast_addr}:{self.port}")

    def send(self, address, *args):
        """Send an OSC message."""
        builder = OscMessageBuilder(address=address)
        for arg in args:
            builder.add_arg(arg)
        msg = builder.build()
        self.sock.sendto(msg.dgram, (self.broadcast_addr, self.port))
        args_str = f" {list(args)}" if args else ""
        print(f"  Sent: {address}{args_str}")

    def send_next(self, channel):
        """Send next slide command."""
        self.send(f"/clicker/{channel}/next")

    def send_prev(self, channel):
        """Send previous slide command."""
        self.send(f"/clicker/{channel}/prev")

    def send_goto(self, channel, slide):
        """Send goto slide command."""
        self.send(f"/clicker/{channel}/goto", int(slide))

    def send_status(self, channel):
        """Request status."""
        self.send(f"/clicker/{channel}/status")

    def interactive(self):
        """Interactive mode for testing."""
        print("\n=== OSC Broadcast Tester ===")
        print(f"Broadcasting to: {self.broadcast_addr}:{self.port}")
        print("\nCommands:")
        print("  n [channel]        - Next slide (default: main)")
        print("  p [channel]        - Previous slide")
        print("  g <slide> [ch]     - Go to slide number")
        print("  s [channel]        - Request status")
        print("  c <channel>        - Change default channel")
        print("  l                  - List valid channels")
        print("  q                  - Quit")

        default_channel = "main"

        while True:
            try:
                cmd = input(f"\n[{default_channel}]> ").strip().lower()
                if not cmd:
                    continue

                parts = cmd.split()
                action = parts[0]

                if action == 'q':
                    print("Goodbye!")
                    break
                elif action == 'l':
                    print("Valid channels:")
                    for i, ch in enumerate(VALID_CHANNELS):
                        print(f"  {ch}", end="")
                        if (i + 1) % 5 == 0:
                            print()
                    print()
                elif action == 'n':
                    ch = parts[1] if len(parts) > 1 else default_channel
                    self.send_next(ch)
                elif action == 'p':
                    ch = parts[1] if len(parts) > 1 else default_channel
                    self.send_prev(ch)
                elif action == 'g':
                    if len(parts) < 2:
                        print("Usage: g <slide> [channel]")
                        continue
                    slide = int(parts[1])
                    ch = parts[2] if len(parts) > 2 else default_channel
                    self.send_goto(ch, slide)
                elif action == 's':
                    ch = parts[1] if len(parts) > 1 else default_channel
                    self.send_status(ch)
                elif action == 'c':
                    if len(parts) < 2:
                        print(f"Current channel: {default_channel}")
                        print(f"Usage: c <channel>")
                    elif parts[1] in VALID_CHANNELS:
                        default_channel = parts[1]
                        print(f"Channel set to: {default_channel}")
                    else:
                        print(f"Invalid channel: {parts[1]}")
                        print("Use 'l' to list valid channels")
                else:
                    print("Unknown command. Try: n, p, g, s, c, l, q")

            except KeyboardInterrupt:
                print("\nGoodbye!")
                break
            except ValueError as e:
                print(f"Error: {e}")

    def close(self):
        """Close the socket."""
        self.sock.close()


def main():
    parser = argparse.ArgumentParser(
        description="OSC Broadcast Tester for sher-present",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  python osc_tester.py                  # Interactive mode
  python osc_tester.py main next        # Send next to 'main' channel
  python osc_tester.py backup prev      # Send prev to 'backup' channel
  python osc_tester.py main goto 5      # Go to slide 5 on 'main'
        """
    )
    parser.add_argument(
        "channel", nargs="?", default=None,
        help="Channel name (main, backup, keynote1, etc.)"
    )
    parser.add_argument(
        "command", nargs="?", default=None,
        help="Command (next, prev, goto, status)"
    )
    parser.add_argument(
        "arg", nargs="?", default=None,
        help="Argument for goto command (slide number)"
    )
    parser.add_argument(
        "-p", "--port", type=int, default=DEFAULT_PORT,
        help=f"Broadcast port (default: {DEFAULT_PORT})"
    )

    args = parser.parse_args()

    tester = OscTester(port=args.port)

    try:
        if args.channel and args.command:
            # Command-line mode
            if args.channel not in VALID_CHANNELS:
                print(f"Invalid channel: {args.channel}")
                print(f"Valid channels: {', '.join(VALID_CHANNELS[:5])}...")
                return 1

            if args.command == "next":
                tester.send_next(args.channel)
            elif args.command == "prev":
                tester.send_prev(args.channel)
            elif args.command == "goto":
                if not args.arg:
                    print("goto requires slide number")
                    return 1
                tester.send_goto(args.channel, args.arg)
            elif args.command == "status":
                tester.send_status(args.channel)
            else:
                print(f"Unknown command: {args.command}")
                print("Valid commands: next, prev, goto, status")
                return 1
        else:
            # Interactive mode
            tester.interactive()

    finally:
        tester.close()

    return 0


if __name__ == "__main__":
    sys.exit(main())
