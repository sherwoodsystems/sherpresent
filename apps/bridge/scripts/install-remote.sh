#!/bin/bash
# Remote installer for RPi OSC Bridge
# Usage: curl -sSL https://<host>/install-remote.sh | sudo bash
set -euo pipefail

echo "RPi OSC Bridge - Remote Installer"
echo ""

# Check root
if [ "$(id -u)" -ne 0 ]; then
    echo "ERROR: Run with sudo"
    echo "  curl -sSL https://<host>/install-remote.sh | sudo bash"
    exit 1
fi

# Check architecture
ARCH=$(uname -m)
if [ "$ARCH" != "aarch64" ] && [ "$ARCH" != "x86_64" ] && [ "$ARCH" != "armv7l" ]; then
    echo "ERROR: Unsupported architecture: $ARCH"
    echo "Supported: aarch64, armv7l, x86_64"
    exit 1
fi

# Download latest .run from GitHub Releases
REPO="sherwood/sherpresent"
echo "Fetching latest release from $REPO..."

ASSET_URL=$(curl -sSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep -o '"browser_download_url": *"[^"]*\.run"' \
    | head -1 | cut -d'"' -f4)

if [ -z "$ASSET_URL" ]; then
    echo "ERROR: No .run release found at https://github.com/$REPO/releases"
    exit 1
fi

echo "Downloading: $ASSET_URL"
TMPFILE=$(mktemp /tmp/rpi-osc-bridge-XXXX.run)
curl -sSL -o "$TMPFILE" "$ASSET_URL"
chmod +x "$TMPFILE"

echo "Running installer..."
echo ""
bash "$TMPFILE"
rm -f "$TMPFILE"
