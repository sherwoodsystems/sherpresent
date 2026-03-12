# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

## Project Overview

**sher-present** is a cross-platform desktop application for remotely controlling presentation software (PowerPoint, Keynote, LibreOffice Impress) via OSC (Open Sound Control) protocol. Built with Tauri v2, SvelteKit, and Rust.

- Desktop app: `apps/desktop/`
- USB clicker bridge: `apps/bridge/`
- Companion module: `apps/companion-module/` (git submodule)
- Specifications: `spec/`
- Documentation: `docs/`
- Legacy code: `archive/`

## Build Commands

```bash
cd apps/desktop
bun install
bun tauri dev      # Development
bun tauri build    # Production
bun run check      # Type checking
```

For Rust debugging: `RUST_LOG=debug bun tauri dev`

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
OSC Server                         apps/desktop/src-tauri/src/osc/
  - server.rs - UDP listener
  - state_manager.rs - State cache
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

### Direct Mode (default)
- **Receive port**: 9000
- **Feedback port**: 9001

| Command | Description |
|---------|-------------|
| `/clicker/next` | Next slide |
| `/clicker/prev` | Previous slide |
| `/clicker/goto` (int) | Jump to slide N |
| `/clicker/status` | Request state |
| `/clicker/zoom/in` | Increase notes zoom |
| `/clicker/zoom/out` | Decrease notes zoom |

**Feedback:** `/clicker/slide/current`, `/clicker/slide/total`, `/clicker/state/presenting`, `/clicker/state/open`, `/clicker/state/zoom`

### Broadcast Mode (multi-device)
- **Port**: 9002 (configurable)
- **Channels**: `main`, `backup`, `keynote1-9`, `aux1-9`

Commands use channel prefix: `/clicker/<channel>/next`, `/clicker/<channel>/prev`, etc.

Feedback: `/clicker/<channel>/state/presenting`, `/clicker/<channel>/state/slide` (current, total)

### rpi-osc-bridge

Python bridge for USB presentation clickers → OSC. Devices appear in UI with "Bridge" badge.

## Platform Notes

- **macOS**: AppleScript for PowerPoint/Keynote
- **Windows**: COM automation for PowerPoint
- **Linux**: LibreOffice Impress via TCP socket (port 2002)
