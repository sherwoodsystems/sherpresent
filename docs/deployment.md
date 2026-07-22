# Deployment

## Bridge Deployment to Raspberry Pi

The bridge is a single Rust binary (`bridge-headless`) built from `apps/bridge-headless/`.
It runs as an unprivileged systemd service under the `pi` user, which is already in the
`input` group on Raspberry Pi OS.

### Prerequisites (on the Pi)

- Raspberry Pi OS (Lite or Desktop)
- The default `pi` user is in the `input` group (default on Raspberry Pi OS)

### Build the binary

From your dev machine (or cross-compile for `aarch64-unknown-linux-gnu`):

```bash
cd apps/bridge-headless
cargo build --release
```

For cross-compilation to a Pi, you can also use:

```bash
cargo build --release --target aarch64-unknown-linux-gnu
```

### Deploy

```bash
# Copy the binary and service file to the Pi
scp target/release/bridge-headless pi@<PI_IP>:/tmp/
scp apps/bridge-headless/sherpresent-bridge.service pi@<PI_IP>:/tmp/

# Install and start on the Pi
ssh pi@<PI_IP> '
  sudo mkdir -p /opt/sherpresent-bridge
  sudo mv /tmp/bridge-headless /opt/sherpresent-bridge/
  sudo mv /tmp/sherpresent-bridge.service /etc/systemd/system/
  sudo systemctl daemon-reload
  sudo systemctl enable --now sherpresent-bridge
'
```

### Installed Layout

```
/opt/sherpresent-bridge/
└── bridge-headless

~/.config/systems.sherwood.presenter-bridge/
└── config.json

/etc/systemd/system/
└── sherpresent-bridge.service
```

### Service Management

```bash
sudo systemctl start sherpresent-bridge
sudo systemctl enable sherpresent-bridge   # Start on boot
sudo systemctl status sherpresent-bridge
sudo journalctl -u sherpresent-bridge -f   # View logs
```

### Configuration

The bridge is configured through the same config file used by the desktop bridge UI:

```bash
# On the Pi, as the pi user:
nano ~/.config/systems.sherwood.presenter-bridge/config.json
sudo systemctl restart sherpresent-bridge
```

Or configure remotely from the desktop app's Bridges page, which will connect to the
bridge's HTTP API on port 8080.

### Uninstalling

```bash
sudo systemctl stop sherpresent-bridge
sudo systemctl disable sherpresent-bridge
sudo rm /etc/systemd/system/sherpresent-bridge.service
sudo rm -rf /opt/sherpresent-bridge
```
