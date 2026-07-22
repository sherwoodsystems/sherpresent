# Getting Started

## Desktop App

### Prerequisites

- [Bun](https://bun.sh) runtime
- [Rust](https://rustup.rs) toolchain
- Tauri v2 system dependencies ([see Tauri docs](https://v2.tauri.app/start/prerequisites/))

### Quick Start

```bash
cd apps/desktop
bun install
bun tauri dev
```

The app will open and start listening for OSC commands on port 9000.

## Bridge (Raspberry Pi)

### Prerequisites

- Raspberry Pi with Raspberry Pi OS
- Rust toolchain (for building) or a pre-built `bridge-headless` binary
- USB presentation clicker

### Build and deploy

```bash
cd apps/bridge-headless
cargo build --release --target aarch64-unknown-linux-gnu

# Copy to Pi and install
scp target/aarch64-unknown-linux-gnu/release/bridge-headless pi@<PI_IP>:/tmp/
scp sherpresent-bridge.service pi@<PI_IP>:/tmp/
ssh pi@<PI_IP> '
  sudo mkdir -p /opt/sherpresent-bridge
  sudo mv /tmp/bridge-headless /opt/sherpresent-bridge/
  sudo mv /tmp/sherpresent-bridge.service /etc/systemd/system/
  sudo systemctl daemon-reload
  sudo systemctl enable --now sherpresent-bridge
'
```

The bridge runs as an unprivileged service and reads USB input devices via the `input`
group (already configured on Raspberry Pi OS).

Configure via the desktop app Bridges page or the HTTP API at `http://<pi-ip>:8080/status`.
