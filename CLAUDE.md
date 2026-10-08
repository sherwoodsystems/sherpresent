# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

## Project Overview

**sher-present** is a cross-platform desktop application for remotely controlling presentation software (PowerPoint, Keynote, LibreOffice Impress) via OSC (Open Sound Control) protocol. Built with Tauri v2, SvelteKit, and Rust.

- Desktop app: `apps/desktop/`
- Bridge (single Tauri app; core engine in `src-tauri/src/bridge/`): `apps/bridge/`
- Shared discovery/OSC core: `crates/sherpresent-core/`
- Companion module: `apps/companion-module/` (git submodule)
- Specifications: `spec/`
- Documentation: `docs/`


## Build Commands

```bash
cd apps/desktop
bun install
bun tauri dev      # Development
bun tauri build    # Production
bun run check      # Type checking
bun run lint       # ESLint + Prettier check (`bun run format` to fix)
bun run test       # Vitest
bun run macos:test # Swift helper tests (`macos:format` to format them)
```

`scripts/check-all.sh` runs every check in the repo (Rust fmt/clippy/tests,
both frontends, Swift lint/tests). Run it before committing.

For Rust debugging: `RUST_LOG=debug bun tauri dev`

Linux also needs `libasound2-dev` at build time (cpal's ALSA backend, used by
live captions).

## Architecture

```
Frontend (SvelteKit + Svelte 5)     apps/desktop/src/routes/, src/lib/components/
        ↓ Tauri invoke/listen
Tauri Commands (Rust)              apps/desktop/src-tauri/src/lib.rs
        ↓
Presentation Adapters              apps/desktop/src-tauri/src/adapters/
  - PowerPoint (macOS/Windows)
  - Keynote (macOS)
  - LibreOffice (cross-platform)
        ↓
StateManager                       apps/desktop/src-tauri/src/osc/state_manager.rs
  - the one state cache and poller; UI, OSC and web commands all go through it
  - publishes `presentation-status` (UI), status_broadcast (web), subscribe() (OSC)
OSC Server                         apps/desktop/src-tauri/src/osc/
  - server.rs - UDP listener
  - messages.rs - OSC definitions
```

## Svelte 5 Runes

```svelte
let { prop } = $props();
let count = $state(0);
let doubled = $derived(count * 2);
$effect(() => { /* side effects */ });
```

## OSC Protocol

See `spec/osc-protocol.md` for full specification.

- **Receive port**: 9000
- **Feedback port**: 9001

| Command | Description |
|---------|-------------|
| `/clicker/next` | Next slide |
| `/clicker/prev` (or `/clicker/previous`) | Previous slide |
| `/clicker/goto` (int) | Jump to slide N |
| `/clicker/status` | Request state (sends all feedback) |
| `/clicker/refresh` | Re-read state from the presentation app |
| `/clicker/zoom` | Request the notes zoom level |
| `/clicker/zoomIn` | Increase notes zoom |
| `/clicker/zoomOut` | Decrease notes zoom |
| `/clicker/scrollUp`, `/clicker/scrollDown` | Scroll the notes views |
| `/clicker/notesPage` | Next screenful of notes (stage page + Syphon notes), wraps to top |

**Feedback:** `/clicker/state/presenting`, `/clicker/state/open`, `/clicker/slide/current`,
`/clicker/slide/total`, `/clicker/zoom/level`, plus `/clicker/slide/build` and
`/clicker/slide/builds` when the slide has builds.

### Bridge

USB presentation clicker → OSC bridge. It's a single Tauri app (`apps/bridge/`)
that speaks OSCPoint's `/oscpoint/...` schema (the desktop app doesn't listen for it);
the core engine (USB capture, OSC, mDNS discovery, config) lives in-crate under
`apps/bridge/src-tauri/src/bridge/`.

The Linux backend uses passive `evdev` input capture: devices are read without exclusive
`grab()`, so no root privileges or udev rules are required as long as the user is in the
`input` group (default on Raspberry Pi OS).

## Platform Notes

- **macOS**: AppleScript for PowerPoint/Keynote
- **Windows**: COM automation for PowerPoint
- **Linux**: LibreOffice Impress via TCP socket (port 2002)
