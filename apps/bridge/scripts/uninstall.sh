#!/bin/bash
set -e

echo "=========================================="
echo "RPi OSC Bridge - Uninstallation Script"
echo "=========================================="
echo ""

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo "ERROR: Please run as root (sudo bash uninstall.sh)"
    exit 1
fi

echo "[1/4] Force killing services..."
systemctl kill -s SIGKILL rpi-osc-bridge 2>/dev/null || true
systemctl kill -s SIGKILL config-server 2>/dev/null || true
sleep 1
echo "Services killed"

echo "[2/4] Resetting and disabling services..."
systemctl reset-failed rpi-osc-bridge 2>/dev/null || true
systemctl reset-failed config-server 2>/dev/null || true
systemctl disable rpi-osc-bridge 2>/dev/null || true
systemctl disable config-server 2>/dev/null || true
echo "Services disabled"

echo "[3/4] Removing service files..."
rm -f /etc/systemd/system/rpi-osc-bridge.service
rm -f /etc/systemd/system/config-server.service
systemctl daemon-reload
echo "Service files removed"

echo "[4/4] Removing installation files..."
rm -rf /opt/rpi-osc-bridge
rm -rf /etc/rpi-osc-bridge
rm -f /var/log/rpi-osc-bridge.log
rm -rf /var/run/rpi-osc-bridge
echo "Installation files removed (including venv and all dependencies)"

echo ""
echo "=========================================="
echo "Uninstallation complete!"
echo "=========================================="
echo ""
echo "To reinstall, run:"
echo "  curl -sSL https://<host>/install-remote.sh | sudo bash"
echo ""
echo "=========================================="
