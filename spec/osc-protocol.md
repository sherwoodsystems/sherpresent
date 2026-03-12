# OSC Protocol Specification

## Transport

- Protocol: OSC 1.0 over UDP
- Encoding: `rosc` (Rust), `python-osc` (Python)
- All addresses use the `/clicker/` namespace prefix

## Direct Mode (Default)

Point-to-point communication between a single controller and a desktop app.

- **Receive port**: 9000 (desktop app listens here)
- **Feedback port**: 9001 (desktop app sends feedback here)

### Incoming Commands

Received by the desktop app on port 9000.

| Address | Arguments | Description |
|---------|-----------|-------------|
| `/clicker/next` | none | Advance to next slide |
| `/clicker/prev` | none | Go to previous slide |
| `/clicker/previous` | none | Alias for `/clicker/prev` |
| `/clicker/zoom` | none | Query current notes zoom level |
| `/clicker/zoomIn` | none | Increase notes zoom level |
| `/clicker/zoomOut` | none | Decrease notes zoom level |
| `/clicker/status` | none | Request full state update |
| `/clicker/refresh` | none | Force state re-sync from presentation app |

### Outgoing Feedback

Sent by the desktop app to the feedback address on port 9001.

| Address | Arguments | Description |
|---------|-----------|-------------|
| `/clicker/state/presenting` | int (0\|1) | Is slideshow active? |
| `/clicker/state/open` | int (0\|1) | Is presentation file open? |
| `/clicker/slide/current` | int | Current slide number (0 if not presenting) |
| `/clicker/slide/total` | int | Total slide count |
| `/clicker/zoom/level` | int | Current zoom percentage (0 if unavailable) |

## Broadcast Mode (Multi-Device)

Multiple devices on a shared channel, communicating via subnet broadcast.

- **Port**: 9002 (configurable)
- All messages are broadcast to the subnet broadcast address

### Valid Channels

- `main`
- `backup`
- `keynote1` through `keynote9`
- `aux1` through `aux9`

### Channel Commands

Sent to port 9002 (broadcast). Format: `/clicker/{channel}/{command}`

| Address Pattern | Arguments | Description |
|-----------------|-----------|-------------|
| `/clicker/{channel}/next` | none | Next slide |
| `/clicker/{channel}/prev` | none | Previous slide |
| `/clicker/{channel}/previous` | none | Alias for prev |
| `/clicker/{channel}/goto` | int slide_number | Jump to slide N |
| `/clicker/{channel}/status` | none | Request state |
| `/clicker/{channel}/refresh` | none | Force state re-sync |
| `/clicker/{channel}/black` | none | Black screen (placeholder) |
| `/clicker/{channel}/white` | none | White screen (placeholder) |
| `/clicker/{channel}/resume` | none | Resume from black/white (placeholder) |

### Channel Feedback

Sent by the desktop app in broadcast mode.

| Address Pattern | Arguments | Description |
|-----------------|-----------|-------------|
| `/clicker/{channel}/state/presenting` | int (0\|1) | Is slideshow active? |
| `/clicker/{channel}/state/open` | int (0\|1) | Is presentation file open? |
| `/clicker/{channel}/state/slide` | int current, int total | Current and total slides |
| `/clicker/{channel}/state/zoom` | int | Zoom percentage |

## Peer Discovery

Used for channel synchronization between desktop app instances.

| Address | Arguments | Description |
|---------|-----------|-------------|
| `/clicker/channel/announce` | string instance_id, string channel, string ip, int port | Announce presence |
| `/clicker/channel/leave` | string instance_id | Announce departure |
| `/clicker/channel/heartbeat` | string instance_id | Keep-alive |
| `/clicker/channel/cmd/next` | string origin_id | Forwarded next command |
| `/clicker/channel/cmd/prev` | string origin_id | Forwarded prev command |
| `/clicker/channel/cmd/goto` | string origin_id, int slide | Forwarded goto command |

## Bridge-Specific Behavior

The RPi bridge translates USB HID events to OSC:

- **Broadcast mode** (default): Sends `/clicker/{channel}/next` and `/clicker/{channel}/prev` to the broadcast port
- **Direct mode** (legacy): Sends `/clicker/next` and `/clicker/prev` to a specific IP on port 9000
- Each USB device slot can be assigned a different channel
- Bridge listens for feedback on the broadcast port and writes state to `/var/run/rpi-osc-bridge/feedback.json`
