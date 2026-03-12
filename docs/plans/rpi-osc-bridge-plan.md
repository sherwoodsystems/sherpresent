# RPi OSC Bridge - Project Plan

## Overview

**rpi-osc-bridge** is a lightweight background service for Raspberry Pi that converts keyboard input (arrow keys) into OSC commands to control sher-present running on a presenter computer.

## Use Case

- **Hardware**: Perfect Cue Mini (or any USB keyboard) → Raspberry Pi 5 (testing) → RPi Zero with USB/PoE hat (production)
- **Software**: Raspbian OS Lite (headless, command-line only)
- **Network**: SSH access for management
- **Goal**: Fast, reliable, zero-configuration keyboard-to-OSC bridge

## Architecture

```
┌──────────────────┐
│ Perfect Cue Mini │  USB keyboard input
│  (USB Keyboard)  │
└────────┬─────────┘
         │ Left/Right Arrow Keys
┌────────▼─────────┐
│  Raspberry Pi    │
│  rpi-osc-bridge  │  Python service listening for keypresses
│  (evdev + OSC)   │
└────────┬─────────┘
         │ UDP OSC packets (port 9000)
         │ Network (Ethernet/WiFi)
┌────────▼─────────┐
│ Presenter PC     │
│  sher-present    │  Tauri app receives OSC commands
│  (Tauri + Rust)  │
└──────────────────┘
```

## Technical Design

### Input Handling
- **Library**: `evdev` (Python) - direct access to Linux input devices
- **Permissions**: Script runs with appropriate permissions to read `/dev/input/event*`
- **Keys Monitored**:
  - `KEY_LEFT` (arrow left) → `/ppt/prev`
  - `KEY_RIGHT` (arrow right) → `/ppt/next`

### OSC Output
- **Library**: `python-osc` - lightweight OSC client
- **Protocol**: UDP
- **Default Target**: Configurable IP:port (default: `192.168.1.100:9000`)
- **Commands**:
  - Left Arrow → `/ppt/prev` (no args)
  - Right Arrow → `/ppt/next` (no args)

### Configuration
Simple JSON or INI config file:
```json
{
  "osc_host": "192.168.1.100",
  "osc_port": 9000,
  "keyboard_device": "auto"
}
```

### Deployment
- **Systemd service**: Auto-start on boot, restart on failure
- **Installation script**: One-command setup (`sudo bash install.sh`)
- **Minimal dependencies**: Python 3 + 2 pip packages

## Implementation Plan

### Phase 1: Core Script (MVP)
1. Python script using `evdev` to capture keyboard events
2. OSC client to send commands to sher-present
3. Auto-detect USB keyboard device
4. Basic error handling and logging

### Phase 2: Service Integration
1. Systemd service file for auto-start
2. Installation script for easy deployment
3. Configuration file support
4. Logging to `/var/log/rpi-osc-bridge.log`

### Phase 3: Production Hardening
1. Graceful shutdown handling
2. Connection monitoring (heartbeat to presenter PC)
3. LED status indicators (if GPIO available)
4. Watchdog timer for crash recovery

## File Structure

```
rpi-osc-bridge/
├── bridge.py              # Main service script
├── config.example.json    # Example configuration
├── install.sh             # Installation script
├── rpi-osc-bridge.service # Systemd service file
├── requirements.txt       # Python dependencies
└── README.md              # Setup and usage instructions
```

## Installation Steps (Target)

On Raspberry Pi:
```bash
# Clone or copy files to RPi
cd /opt
sudo git clone <repo> rpi-osc-bridge
cd rpi-osc-bridge

# Run installer
sudo bash install.sh

# Edit config (set presenter PC IP)
sudo nano /etc/rpi-osc-bridge/config.json

# Start service
sudo systemctl start rpi-osc-bridge
sudo systemctl enable rpi-osc-bridge

# Check status
sudo systemctl status rpi-osc-bridge
```

## Testing Plan

### Local Testing (RPi 5)
1. Connect Perfect Cue Mini via USB
2. Start bridge manually: `sudo python3 bridge.py`
3. Verify OSC packets received by sher-present (check logs)
4. Test left/right arrow presses

### Service Testing
1. Install systemd service
2. Reboot RPi
3. Verify auto-start
4. Test failure recovery (kill process, check restart)

### Production Testing (RPi Zero)
1. Deploy to RPi Zero with USB/PoE hat
2. Stress test (rapid key presses)
3. Network stability test (disconnect/reconnect)
4. Long-term reliability test (24+ hour uptime)

## Performance Targets

- **Latency**: < 50ms from keypress to OSC send
- **CPU Usage**: < 5% idle, < 10% active
- **Memory**: < 20MB
- **Boot Time**: Service ready in < 10 seconds after boot

## Security Considerations

- Service runs as dedicated user (not root after setup)
- Config file permissions: 644 (readable by service user)
- No external network exposure (outbound OSC only)
- Minimal attack surface (no web UI, no incoming connections)

## Future Enhancements

- Web UI for remote configuration (optional)
- Multiple keyboard support (zones/regions)
- Configurable key mappings
- MIDI support (in addition to OSC)
- Status feedback (LED blink on successful send)
