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

The bridge comes in two flavors sharing `crates/sherpresent-bridge-core`:

- **Tauri GUI**: `apps/bridge/` — desktop app with a Svelte UI.
- **Headless binary**: `apps/bridge-headless/` — single binary for Raspberry Pi / server deployments.

### Run the Tauri bridge locally

```bash
cd apps/bridge
bun install
bun tauri dev
```

### Run the headless bridge locally

```bash
cd apps/bridge-headless
cargo run
```

### Build the headless release binary

```bash
cd apps/bridge-headless
cargo build --release
# Binary: target/release/bridge-headless
```

## Companion Module

```bash
cd apps/companion-module
yarn install
yarn build
```
