# SherPresent Bridge — Tauri GUI

Cross-platform bridge configuration UI for SherPresent. This Tauri app wraps the
shared bridge core (`crates/sherpresent-bridge-core/`) and provides a native
webview interface for pairing USB presentation clickers with OSC targets.

For Raspberry Pi and headless/server deployments, use `apps/bridge-headless/`
instead.

## Development

```bash
cd apps/bridge
bun install
bun tauri dev
```

## Build

```bash
cd apps/bridge
bun tauri build
```

## Architecture

- `src-tauri/src/lib.rs` — Tauri setup, starts `BridgeCore`, forwards core events
  to the Svelte frontend via Tauri events.
- `src-tauri/src/commands/` — Thin wrappers around `BridgeCore` methods.
- `src/routes/+page.svelte` — Main configuration UI.
- `src/lib/types.ts` — TypeScript interfaces matching the Rust config schema.

The bridge core handles USB device detection, OSC send/receive, mDNS discovery,
and the HTTP config API.
