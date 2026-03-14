# Architecture

## System Overview

SherPresent controls presentation software remotely via OSC (Open Sound Control) over UDP.

```
USB Clicker → [Bridge (RPi)] → OSC/UDP → [Desktop App] → Presentation Software
                                              ↑
                              Companion/other OSC sources
```

## Desktop App (`apps/desktop/`)

Tauri v2 application with a SvelteKit frontend and Rust backend.

```
Frontend (SvelteKit + Svelte 5)     src/routes/, src/lib/components/
        ↓ Tauri invoke/listen
Tauri Commands (Rust)              src-tauri/src/lib.rs
        ↓
Presentation Adapters              src-tauri/src/adapters/
  - PowerPoint (macOS/Windows)
  - Keynote (macOS)
  - LibreOffice (cross-platform)
        ↓
OSC Server                         src-tauri/src/osc/
  - server.rs   — UDP listener
  - state_manager.rs — State cache
  - messages.rs — OSC message definitions
```

### Platform Adapters

Each adapter implements the `PresentationAdapter` trait:

| Platform | Adapter | Method |
|----------|---------|--------|
| macOS | `powerpoint.rs`, `keynote.rs` | AppleScript |
| Windows | `powerpoint_windows.rs` | COM automation |
| Linux | `libreoffice.rs` | TCP socket (port 1599) |
| All | `canva.rs` | Webview + WebSocket |

All adapters support retrieving presenter notes for the current slide via `get_presenter_notes()`. Notes are displayed in the StatusDisplay component whenever available.

## Bridge (`apps/bridge/`)

Python service running on a Raspberry Pi that converts USB HID events from presentation clickers into OSC messages.

- Detects USB keyboards via evdev
- Maps KEY_LEFT/KEY_RIGHT to prev/next
- Supports up to 3 simultaneous USB devices with per-device channel assignment
- Broadcasts or direct-sends OSC messages
- Web config UI on port 80

## Communication

All components communicate via the OSC protocol. See [spec/osc-protocol.md](../spec/osc-protocol.md) for the full specification.

### Operating Modes

- **Direct Mode**: Point-to-point, bridge sends to a single desktop app (ports 9000/9001)
- **Broadcast Mode**: Multi-device, all participants on a shared port (default 9002)
