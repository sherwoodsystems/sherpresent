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

# Install Python dependencies
echo "[1/5] Installing Python dependencies..."
apt-get update -qq
apt-get install -y python3 python3-pip
pip3 install --break-system-packages evdev python-osc || pip3 install evdev python-osc

# Create installation directory
echo "[2/5] Creating installation directory..."
mkdir -p "$INSTALL_DIR"

if [ "$SCRIPT_DIR" != "$INSTALL_DIR" ]; then
    echo "Copying files from $SCRIPT_DIR to $INSTALL_DIR..."
    cp "$SCRIPT_DIR/bridge.py" "$INSTALL_DIR/"
    cp "$SCRIPT_DIR/config-server.py" "$INSTALL_DIR/" 2>/dev/null || true
    cp "$SCRIPT_DIR/config.example.json" "$INSTALL_DIR/" 2>/dev/null || true
    cp "$SCRIPT_DIR/rpi-osc-bridge.service" "$INSTALL_DIR/" 2>/dev/null || true
    cp "$SCRIPT_DIR/config-server.service" "$INSTALL_DIR/" 2>/dev/null || true
    cp -r "$SCRIPT_DIR/web" "$INSTALL_DIR/" 2>/dev/null || true
    chmod +x "$INSTALL_DIR/bridge.py"
    chmod +x "$INSTALL_DIR/config-server.py" 2>/dev/null || true
else
    echo "Already in installation directory ($INSTALL_DIR), skipping file copy"
    chmod +x "$INSTALL_DIR/bridge.py"
    chmod +x "$INSTALL_DIR/config-server.py" 2>/dev/null || true
fi

# Create config directory and file
echo "[3/5] Setting up configuration..."
mkdir -p "$CONFIG_DIR"

if [ ! -f "$CONFIG_FILE" ]; then
    cp "$SCRIPT_DIR/config.example.json" "$CONFIG_FILE"
    echo "Created config file at: $CONFIG_FILE"
    echo "IMPORTANT: Edit this file to set your presenter PC IP address!"
else
    echo "Config file already exists: $CONFIG_FILE"
fi

# Install systemd services
echo "[4/5] Installing systemd services..."
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

# Create log file with proper permissions
touch /var/log/rpi-osc-bridge.log
chmod 644 /var/log/rpi-osc-bridge.log

# Create runtime state directory for feedback
mkdir -p /var/run/rpi-osc-bridge
chmod 755 /var/run/rpi-osc-bridge

echo "[5/5] Installation complete!"
echo ""
echo "=========================================="
echo "Next Steps:"
echo "=========================================="
echo ""
echo "Configure via Web UI (RECOMMENDED):"
echo "  Open your browser and go to:"
echo "  http://$(hostname -I | awk '{print $1}'):8080"
echo "  or"
echo "  http://$(hostname).local:8080"
echo ""
echo "OR configure via command line:"
echo "  1. Edit: sudo nano $CONFIG_FILE"
echo "  2. Set 'osc_host' to your presenter PC IP"
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
echo "  sudo tail -f /var/log/rpi-osc-bridge.log"
echo ""
echo "=========================================="
