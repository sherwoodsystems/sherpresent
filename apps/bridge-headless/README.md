# SherPresent Bridge — Headless

Headless bridge binary for Raspberry Pi and other server-style deployments.
Shares all core logic with the Tauri GUI bridge via `crates/sherpresent-bridge-core/`.

## Features

- No GUI required — runs as a systemd service
- Passive `evdev` input capture (no exclusive `grab()`, no udev rules)
- Runs as the `pi` user on Raspberry Pi OS (already in the `input` group)
- HTTP config API on port 8080
- mDNS service discovery

## Build

```bash
cd apps/bridge-headless
cargo build --release
```

For Raspberry Pi (cross-compile):

```bash
cargo build --release --target aarch64-unknown-linux-gnu
```

## Deploy

```bash
scp target/release/bridge-headless pi@<PI_IP>:/tmp/
scp sherpresent-bridge.service pi@<PI_IP>:/tmp/
ssh pi@<PI_IP> '
  sudo mkdir -p /opt/sherpresent-bridge
  sudo mv /tmp/bridge-headless /opt/sherpresent-bridge/
  sudo mv /tmp/sherpresent-bridge.service /etc/systemd/system/
  sudo systemctl daemon-reload
  sudo systemctl enable --now sherpresent-bridge
'
```

## Service management

```bash
sudo systemctl status sherpresent-bridge
sudo journalctl -u sherpresent-bridge -f
```

## Configuration

Config is stored at:

- `~/.config/systems.sherwood.presenter-bridge/config.json` (when running as a user)

Edit and restart the service, or configure remotely from the desktop app's Bridges page.
