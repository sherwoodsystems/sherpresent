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
- Python 3.9+
- USB presentation clicker

### Quick Start

```bash
cd apps/bridge
sudo bash scripts/install.sh
sudo systemctl start rpi-osc-bridge
```

Configure via the web UI at `http://<pi-ip>/`.
