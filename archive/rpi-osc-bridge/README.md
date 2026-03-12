# SherPresent OSC Bridge

Lightweight keyboard-to-OSC bridge for Raspberry Pi. Converts arrow key presses into OSC commands to control **sher-present** presentation software remotely.

## Features

- **Ultra-lightweight**: < 20MB memory, < 5% CPU usage
- **Fast**: < 50ms latency from keypress to OSC send
- **Web Configuration UI**: Easy browser-based setup (no SSH/terminal needed)
- **OSC Feedback**: Receives and displays real-time presentation status
- **Test Commands**: Built-in Previous/Next buttons for testing without keyboard
- **USB Port Tracking**: Identifies keyboards by physical USB port for future A/B functionality
- **Auto-start**: Systemd service starts on boot
- **Auto-recovery**: Automatically restarts on failure
- **Simple config**: Single JSON file or web interface
- **Headless**: Perfect for Raspbian OS Lite (no GUI needed)

## Hardware Requirements

- Raspberry Pi 5, 4, Zero, or Zero 2W
- USB keyboard (e.g., Perfect Cue Mini, USB presenter remote)
- Network connection (Ethernet or WiFi)
- Optional: USB/PoE hat for production deployment

## Software Requirements

- Raspbian OS (Lite or Desktop)
- Python 3.7+
- Network access to presenter PC running sher-present

## Quick Start

### 1. Transfer Files to Raspberry Pi

```bash
# Option A: Clone repository (if using git)
cd /opt
sudo git clone <repository-url> rpi-osc-bridge

# Option B: Manual copy via SCP
scp -r rpi-osc-bridge/ pi@<rpi-ip>:/home/pi/
ssh pi@<rpi-ip>
sudo mv /home/pi/rpi-osc-bridge /opt/
```

### 2. Run Installation Script

```bash
cd /opt/rpi-osc-bridge
sudo bash install.sh
```

The installer will:
- Install Python dependencies (`evdev`, `python-osc`)
- Copy files to `/opt/rpi-osc-bridge`
- Create config file at `/etc/rpi-osc-bridge/config.json`
- Install and start systemd services (bridge + web UI)

### 3. Configure Target Host

**Option A: Web UI (Recommended)**

Open your browser and navigate to:
```
http://rpi-cam.local:8080
```
or
```
http://<raspberry-pi-ip>:8080
```

The web interface will let you:
- Enter your presenter PC's IP address
- Change OSC port (default: 9000)
- Adjust log level
- See service status
- Save and restart with one click

**Option B: Command Line**

Edit the configuration file manually:

```bash
sudo nano /etc/rpi-osc-bridge/config.json
```

Set `osc_host` to your presenter PC's IP:

```json
{
  "osc_host": "192.168.1.100",
  "osc_port": 9000,
  "keyboard_device": "auto",
  "log_level": "INFO"
}
```

Save and exit (Ctrl+X, Y, Enter), then restart:
```bash
sudo systemctl restart rpi-osc-bridge
```

### 4. Start the Service

```bash
# Start now
sudo systemctl start rpi-osc-bridge

# Enable auto-start on boot
sudo systemctl enable rpi-osc-bridge

# Check status
sudo systemctl status rpi-osc-bridge
```

### 5. Test

Press the **left arrow** or **right arrow** keys on your connected USB keyboard. Check logs to verify OSC messages are being sent:

```bash
# View live logs
sudo journalctl -u rpi-osc-bridge -f

# Or check log file
sudo tail -f /var/log/rpi-osc-bridge.log
```

You should see messages like:
```
2025-12-10 10:30:45 - INFO - RIGHT arrow pressed → /clicker/next
2025-12-10 10:30:47 - INFO - LEFT arrow pressed → /clicker/prev
```

## Configuration Options

Edit `/etc/rpi-osc-bridge/config.json`:

| Option | Description | Default |
|--------|-------------|---------|
| `osc_host` | IP address of presenter PC running sher-present | `192.168.1.100` |
| `osc_port` | OSC receive port on presenter PC | `9000` |
| `feedback_port` | OSC feedback port for receiving presentation status | `9001` |
| `keyboard_device` | Path to keyboard device, or `"auto"` to auto-detect | `auto` |
| `log_level` | Logging verbosity: `DEBUG`, `INFO`, `WARNING`, `ERROR` | `INFO` |

After changing config, restart the service:
```bash
sudo systemctl restart rpi-osc-bridge
```

## Manual Testing (Without Service)

For debugging, run the bridge script directly:

```bash
# Stop service first
sudo systemctl stop rpi-osc-bridge

# Run manually
cd /opt/rpi-osc-bridge
sudo python3 bridge.py
```

Press Ctrl+C to stop.

## Troubleshooting

### No keyboard detected

Check connected USB devices:
```bash
sudo python3 -c "import evdev; print([d.name for d in [evdev.InputDevice(p) for p in evdev.list_devices()]])"
```

If your keyboard isn't detected, try specifying the device path manually in config:
```json
{
  "keyboard_device": "/dev/input/event0"
}
```

### OSC messages not reaching presenter PC

1. **Check network connectivity**:
   ```bash
   ping <presenter-pc-ip>
   ```

2. **Verify sher-present is running** and OSC server is started (check sher-present UI)

3. **Check firewall** on presenter PC (port 9000 UDP must be open)

4. **Test OSC manually** from another device:
   ```bash
   # Install oscpy (for testing)
   pip3 install oscpy
   python3 -c "from oscpy.client import OSCClient; c = OSCClient('192.168.1.100', 9000); c.send_message(b'/ppt/next', [])"
   ```

### Service won't start

```bash
# Check service status
sudo systemctl status rpi-osc-bridge

# View detailed logs
sudo journalctl -u rpi-osc-bridge -n 50

# Check for Python errors
sudo journalctl -u rpi-osc-bridge | grep ERROR
```

### Permission errors

The service runs as root to access `/dev/input/` devices. If you see permission errors, ensure:
- Files in `/opt/rpi-osc-bridge` are readable: `sudo chmod +r /opt/rpi-osc-bridge/*`
- Config file exists: `ls -l /etc/rpi-osc-bridge/config.json`

## Web Configuration UI

The web interface runs automatically on port 8080 and provides an easy way to configure the bridge without SSH access.

### Accessing the Web UI

From any device on your network:
```
http://rpi-cam.local:8080
http://<raspberry-pi-ip>:8080
```

### Features

- **Live Status**: See if the bridge service is running
- **Configuration Form**: Set presenter PC IP, OSC port, feedback port, log level
- **Test Commands**: Previous/Next slide buttons for testing without keyboard
- **Presentation Status**: Real-time display of current slide, presenting state, zoom level
- **Connected Devices**: Shows all keyboards with their USB port assignments
- **Recent Logs**: Live view of service logs
- **Input Validation**: Prevents invalid IP addresses or port numbers
- **Auto-Restart**: Saves config and restarts bridge service automatically
- **Auto-Refresh**: Status, feedback, and logs update automatically

### Managing the Web Server

```bash
# Check web server status
sudo systemctl status config-server

# Restart web server
sudo systemctl restart config-server

# Stop web server
sudo systemctl stop config-server

# View web server logs
sudo journalctl -u config-server -f
```

## Service Management

### Bridge Service

```bash
# Start service
sudo systemctl start rpi-osc-bridge

# Stop service
sudo systemctl stop rpi-osc-bridge

# Restart service
sudo systemctl restart rpi-osc-bridge

# Enable auto-start on boot
sudo systemctl enable rpi-osc-bridge

# Disable auto-start
sudo systemctl disable rpi-osc-bridge

# View status
sudo systemctl status rpi-osc-bridge

# View logs (live)
sudo journalctl -u rpi-osc-bridge -f

# View logs (last 50 lines)
sudo journalctl -u rpi-osc-bridge -n 50
```

### Both Services

```bash
# Check both services
sudo systemctl status rpi-osc-bridge config-server

# Restart both
sudo systemctl restart rpi-osc-bridge config-server

# View both logs
sudo journalctl -u rpi-osc-bridge -u config-server -f
```

## Uninstall

```bash
# Stop and disable services
sudo systemctl stop rpi-osc-bridge config-server
sudo systemctl disable rpi-osc-bridge config-server

# Remove files
sudo rm /etc/systemd/system/rpi-osc-bridge.service
sudo rm /etc/systemd/system/config-server.service
sudo rm -rf /opt/rpi-osc-bridge
sudo rm -rf /etc/rpi-osc-bridge
sudo rm /var/log/rpi-osc-bridge.log

# Reload systemd
sudo systemctl daemon-reload
```

## Architecture

```
USB Keyboard → evdev (Python) → python-osc → UDP → Presenter PC (sher-present)
```

The bridge:
1. Listens for keyboard input events via Linux `evdev` subsystem
2. Detects left/right arrow key presses
3. Sends corresponding OSC commands via UDP:
   - Left Arrow → `/clicker/prev`
   - Right Arrow → `/clicker/next`

## OSC Commands Sent

| Key | OSC Address | Description |
|-----|-------------|-------------|
| Left Arrow | `/clicker/prev` | Previous slide |
| Right Arrow | `/clicker/next` | Next slide |

## OSC Feedback Received

The bridge listens for feedback from sher-present on port 9001 (configurable) to display real-time presentation status in the web UI:

| OSC Address | Argument | Description |
|-------------|----------|-------------|
| `/clicker/state/presenting` | int (0/1) | Presentation mode active |
| `/clicker/state/open` | int (0/1) | Presentation file open |
| `/clicker/slide/current` | int | Current slide number |
| `/clicker/slide/total` | int | Total number of slides |
| `/clicker/zoom/level` | int | Zoom percentage |

Feedback state is written to `/var/run/rpi-osc-bridge/feedback.json` for monitoring and debugging.

## Performance

Tested on Raspberry Pi 5:
- **Latency**: ~30ms (keypress to OSC send)
- **CPU Usage**: 2-3% idle, 5% during keypress
- **Memory**: 15MB
- **Boot Time**: Service ready in 8 seconds

## Production Deployment

For RPi Zero with USB/PoE hat:

1. **Configure static IP** (optional but recommended):
   ```bash
   sudo nano /etc/dhcpcd.conf
   ```
   Add:
   ```
   interface eth0
   static ip_address=192.168.1.50/24
   static routers=192.168.1.1
   static domain_name_servers=192.168.1.1
   ```

2. **Disable unnecessary services** to reduce resource usage:
   ```bash
   sudo systemctl disable bluetooth
   sudo systemctl disable avahi-daemon
   ```

3. **Test long-term stability** (leave running for 24+ hours)

4. **Label the device** with its IP address for easy identification

## License

MIT License - See repository for details

## Support

For issues with:
- **This bridge**: Check logs, open issue in repository
- **sher-present**: See sher-present documentation
- **Hardware**: Verify USB keyboard works on regular PC first
