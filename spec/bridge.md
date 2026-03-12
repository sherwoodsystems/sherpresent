# Bridge Specification

## Overview

Python service running on a Raspberry Pi that converts USB presentation clicker events into OSC messages.

## Input

- **Source**: Linux evdev (event device) subsystem
- **Detection**: Scans `/dev/input/event*` for keyboard-type devices
- **USB port tracking**: Identifies devices by physical USB port path for consistent slot assignment

### Key Mapping

| evdev Key | OSC Command |
|-----------|-------------|
| `KEY_RIGHT` | next |
| `KEY_LEFT` | prev |
| `KEY_PAGEDOWN` | next |
| `KEY_PAGEUP` | prev |

### Special Device Handling

- **DSan Perfect Cue**: Recognized by vendor/product ID, mapped to correct channels
- **HDMI CEC**: Filtered out to prevent phantom key events from HDMI input switching

## Operating Modes

### Broadcast Mode (Default)

- Sends commands to subnet broadcast address on configurable port (default 9002)
- Uses channel-prefixed addresses: `/clicker/{channel}/next`
- Receives feedback from any desktop app on the same broadcast port
- Supports per-device channel assignment

### Direct Mode (Legacy)

- Sends to a specific IP address on port 9000
- Uses non-prefixed addresses: `/clicker/next`
- Configured via `target_ip` and `target_port` in config

### Companion Mode (Planned)

- Integration with Bitfocus Companion for stream deck control

### Satellite Mode (Planned)

- One bridge relays to another bridge for extended USB reach

## Multi-Device Support

- 3 USB device slots: `usb_1`, `usb_2`, `usb_3`
- Each slot can be assigned a different channel
- Devices are identified by physical USB port path
- Hot-plug detection via udev monitoring

## Feedback Reception

- Listens for OSC feedback on the broadcast port
- Writes received state to `/var/run/rpi-osc-bridge/feedback.json`
- Format: `{ "channel": { "presenting": bool, "slide": int, "total": int } }`

## Web Configuration Server

HTTP server on port 80 for runtime configuration.

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | Static web UI |
| GET | `/api/config` | Current configuration |
| POST | `/api/config` | Update configuration |
| GET | `/api/devices` | List detected USB devices |
| GET | `/api/status` | Bridge status and feedback state |
| POST | `/api/test` | Send test OSC command |
| POST | `/api/restart` | Restart bridge service |

## Deployment

### systemd Services

- `rpi-osc-bridge.service` — main bridge process
- `config-server.service` — web configuration server

### Install Paths

```
/opt/rpi-osc-bridge/          # Application files
  bridge.py
  config-server.py
  web/index.html
/etc/rpi-osc-bridge/          # Configuration
  config.json
/var/run/rpi-osc-bridge/      # Runtime state (tmpfs)
  feedback.json
/var/log/rpi-osc-bridge.log   # Log file
```

### Self-Extracting Installer

Built with `makeself`. Bundles all files into a `.run` archive that extracts and runs `install.sh`.

## Performance Targets

- Latency: <50ms from key press to OSC message
- CPU: <5% on Raspberry Pi 3/4
- RAM: <20MB
