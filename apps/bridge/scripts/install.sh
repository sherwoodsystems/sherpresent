#!/bin/bash
set -e

echo "=========================================="
echo "RPi OSC Bridge - Installation Script"
echo "=========================================="
echo ""

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo "ERROR: Please run as root (sudo bash install.sh)"
    exit 1
fi

# Detect script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_DIR="/opt/rpi-osc-bridge"
CONFIG_DIR="/etc/rpi-osc-bridge"
CONFIG_FILE="$CONFIG_DIR/config.json"

echo "Installing from: $SCRIPT_DIR"
echo "Installing to: $INSTALL_DIR"
echo ""

# Clean up any existing installation
echo "[0/6] Cleaning up previous installation..."
systemctl stop rpi-osc-bridge 2>/dev/null || true
systemctl stop config-server 2>/dev/null || true
systemctl disable rpi-osc-bridge 2>/dev/null || true
systemctl disable config-server 2>/dev/null || true
rm -f /etc/systemd/system/rpi-osc-bridge.service 2>/dev/null || true
rm -f /etc/systemd/system/config-server.service 2>/dev/null || true
rm -rf "$INSTALL_DIR" 2>/dev/null || true
rm -rf /var/run/rpi-osc-bridge 2>/dev/null || true
systemctl daemon-reload
echo "Previous installation cleaned up (if any)"
echo ""

# Install system dependencies and create venv
echo "[1/6] Installing dependencies..."
apt-get update -qq
apt-get install -y python3 python3-venv

# Create installation directory
echo "[2/6] Creating installation directory..."
mkdir -p "$INSTALL_DIR"

if [ "$SCRIPT_DIR" != "$INSTALL_DIR" ]; then
    echo "Copying files from $SCRIPT_DIR to $INSTALL_DIR..."
    cp "$SCRIPT_DIR/constants.py" "$INSTALL_DIR/"
    cp "$SCRIPT_DIR/config.py" "$INSTALL_DIR/"
    cp "$SCRIPT_DIR/devices.py" "$INSTALL_DIR/"
    cp "$SCRIPT_DIR/osc.py" "$INSTALL_DIR/"
    cp "$SCRIPT_DIR/file_utils.py" "$INSTALL_DIR/"
    cp "$SCRIPT_DIR/bridge.py" "$INSTALL_DIR/"
    cp "$SCRIPT_DIR/satellite.py" "$INSTALL_DIR/"
    cp "$SCRIPT_DIR/config-server.py" "$INSTALL_DIR/" 2>/dev/null || true
    cp "$SCRIPT_DIR/config.example.json" "$INSTALL_DIR/" 2>/dev/null || true
    cp "$SCRIPT_DIR/rpi-osc-bridge.service" "$INSTALL_DIR/" 2>/dev/null || true
    cp "$SCRIPT_DIR/config-server.service" "$INSTALL_DIR/" 2>/dev/null || true
    cp "$SCRIPT_DIR/VERSION" "$INSTALL_DIR/" 2>/dev/null || true
    cp -r "$SCRIPT_DIR/web" "$INSTALL_DIR/" 2>/dev/null || true
    chmod +x "$INSTALL_DIR/bridge.py"
    chmod +x "$INSTALL_DIR/config-server.py" 2>/dev/null || true
else
    echo "Already in installation directory ($INSTALL_DIR), skipping file copy"
    chmod +x "$INSTALL_DIR/bridge.py"
    chmod +x "$INSTALL_DIR/config-server.py" 2>/dev/null || true
fi

# Create venv and install Python dependencies
echo "Creating Python virtual environment..."
python3 -m venv "$INSTALL_DIR/venv"
"$INSTALL_DIR/venv/bin/pip" install evdev python-osc zeroconf

# Create config directory and file
echo "[3/6] Setting up configuration..."
mkdir -p "$CONFIG_DIR"

if [ ! -f "$CONFIG_FILE" ]; then
    cp "$SCRIPT_DIR/config.example.json" "$CONFIG_FILE"
    echo "Created config file at: $CONFIG_FILE"
    echo "IMPORTANT: Edit this file to set your presenter PC IP address!"
else
    echo "Config file already exists: $CONFIG_FILE"
fi

# Install systemd services
echo "[4/6] Installing systemd services..."
cp "$SCRIPT_DIR/rpi-osc-bridge.service" /etc/systemd/system/
if [ -f "$SCRIPT_DIR/config-server.service" ]; then
    cp "$SCRIPT_DIR/config-server.service" /etc/systemd/system/
fi
systemctl daemon-reload

# Enable and start config server
if [ -f "/etc/systemd/system/config-server.service" ]; then
    systemctl enable config-server
    systemctl start config-server
    echo "Web configuration server enabled and started"
fi

# Create log file and runtime directory
echo "[5/6] Setting up log and runtime directories..."
touch /var/log/rpi-osc-bridge.log
chmod 644 /var/log/rpi-osc-bridge.log
mkdir -p /var/run/rpi-osc-bridge
chmod 755 /var/run/rpi-osc-bridge

VERSION=$(cat "$INSTALL_DIR/VERSION" 2>/dev/null || echo "unknown")
echo "[6/6] Installation complete! (v${VERSION})"
echo ""
echo "=========================================="
echo "Next Steps:"
echo "=========================================="
echo ""
echo "BROADCAST MODE (default):"
echo "  The bridge is pre-configured for broadcast mode on channel 'main'."
echo "  Just make sure sher-present on your presenter PC is also set to:"
echo "    - Channel sync: enabled"
echo "    - Broadcast mode: enabled"
echo "    - Channel: main"
echo "    - Broadcast port: 9002"
echo ""
echo "Configure via Web UI:"
echo "  http://$(hostname -I | awk '{print $1}')/"
echo "  or http://$(hostname).local/"
echo ""
echo "Start the bridge service:"
echo "  sudo systemctl start rpi-osc-bridge"
echo "  sudo systemctl enable rpi-osc-bridge"
echo ""
echo "Check status:"
echo "  sudo systemctl status rpi-osc-bridge"
echo "  sudo systemctl status config-server"
echo ""
echo "View logs:"
echo "  sudo journalctl -u rpi-osc-bridge -f"
echo ""
echo "=========================================="
