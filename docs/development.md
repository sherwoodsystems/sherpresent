# Development

## Desktop App

```bash
cd apps/desktop
bun install
bun tauri dev          # Dev mode with hot reload
bun run check          # TypeScript/Svelte type checking
RUST_LOG=debug bun tauri dev  # With Rust debug logging
```

### Rust Tests

```bash
cd apps/desktop/src-tauri
cargo test
```

### Building for Production

```bash
cd apps/desktop
bun tauri build
```

## Bridge

The bridge runs on a Raspberry Pi. For local development:

```bash
cd apps/bridge
pip install -r requirements.txt
python src/rpi_osc_bridge/bridge.py
```

### Building the Installer

```bash
cd apps/bridge/scripts
./build-installer.sh
# Produces rpi-osc-bridge-installer.run
```

## Companion Module

```bash
cd apps/companion-module
yarn install
yarn build
```
