# SherPresent Bridge — Tauri GUI

Cross-platform bridge for SherPresent: a single Tauri app that pairs USB
presentation clickers with OSC targets. The bridge engine (USB capture, OSC
send/receive, mDNS discovery, config) lives in-crate under
`src-tauri/src/bridge/`, wrapped by a native webview configuration UI.

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
- `src-tauri/src/bridge/` — The bridge engine: USB detection, OSC send/receive,
  mDNS discovery, and config persistence.
- `src-tauri/src/commands/` — Thin wrappers around `BridgeCore` methods.
- `src/routes/+page.svelte` — Main configuration UI.
- `src/lib/components/` — Extracted UI cards (bridge info, feedback, peers,
  settings, USB devices).
- `src/lib/types.ts` — TypeScript interfaces matching the Rust config schema.
