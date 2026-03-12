#!/bin/bash
#
# Build self-extracting installer for RPi OSC Bridge
#
# Prerequisites (on your dev machine, NOT the Pi):
#   Ubuntu/Debian: sudo apt install makeself
#   macOS: brew install makeself
#   Arch: sudo pacman -S makeself
#
# Usage:
#   ./build-installer.sh
#
# Output:
#   rpi-osc-bridge-installer.run
#
# Deploy to Pi:
#   scp rpi-osc-bridge-installer.run pi@<PI_IP>:/tmp/
#   ssh pi@<PI_IP> 'sudo bash /tmp/rpi-osc-bridge-installer.run'
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BRIDGE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUTPUT_FILE="rpi-osc-bridge-installer.run"
LABEL="RPi OSC Bridge Installer v2.1.0"

# Check for makeself
if ! command -v makeself &> /dev/null; then
    echo "ERROR: makeself not found."
    echo ""
    echo "Install it first:"
    echo "  Ubuntu/Debian: sudo apt install makeself"
    echo "  macOS:         brew install makeself"
    echo "  Arch (AUR):    yay -S makeself"
    echo "                 (or: git clone https://aur.archlinux.org/makeself.git && cd makeself && makepkg -si)"
    exit 1
fi

echo "Building self-extracting installer..."
echo "  Source: $SCRIPT_DIR"
echo "  Output: $OUTPUT_FILE"
echo ""

# Create a temp directory with only the files we need
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

# Copy required files (from new monorepo layout into flat deploy layout)
cp "$BRIDGE_DIR/src/rpi_osc_bridge/bridge.py" "$TEMP_DIR/"
cp "$BRIDGE_DIR/src/rpi_osc_bridge/config_server.py" "$TEMP_DIR/config-server.py"
cp "$BRIDGE_DIR/config/config.example.json" "$TEMP_DIR/"
cp "$SCRIPT_DIR/install.sh" "$TEMP_DIR/"
cp "$BRIDGE_DIR/config/rpi-osc-bridge.service" "$TEMP_DIR/"
cp "$BRIDGE_DIR/config/config-server.service" "$TEMP_DIR/"
cp -r "$BRIDGE_DIR/web" "$TEMP_DIR/" 2>/dev/null || true

# Build the self-extracting archive
makeself --gzip \
    "$TEMP_DIR" \
    "$SCRIPT_DIR/$OUTPUT_FILE" \
    "$LABEL" \
    ./install.sh

echo ""
echo "=========================================="
echo "Build complete!"
echo "=========================================="
echo ""
echo "Installer: $SCRIPT_DIR/$OUTPUT_FILE"
echo "Size: $(du -h "$SCRIPT_DIR/$OUTPUT_FILE" | cut -f1)"
echo ""
echo "To deploy to Raspberry Pi:"
echo "  scp $OUTPUT_FILE pi@<PI_IP>:/tmp/"
echo "  ssh pi@<PI_IP> 'sudo bash /tmp/$OUTPUT_FILE'"
echo ""
