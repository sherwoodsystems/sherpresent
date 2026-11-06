# Desktop App Specification

## Overview

Tauri v2 desktop application that receives OSC commands and controls local presentation software.

## Supported Software

| Software | Platform | Adapter |
|----------|----------|---------|
| Microsoft PowerPoint | macOS | AppleScript |
| Microsoft PowerPoint | Windows | COM automation |
| Keynote | macOS | AppleScript |
| LibreOffice Impress | All | TCP socket (port 1599) |
| Canva | All | Webview + WebSocket |

## OSC Server Lifecycle

1. App starts → binds UDP socket on receive port (default 9000)
2. Listens for incoming OSC commands
3. On command → dispatches to the selected presentation adapter
4. State changes → sends feedback messages to feedback port
5. On shutdown → closes sockets

## State Machine

```
   ┌─────┐     file opened     ┌──────┐     slideshow started     ┌────────────┐
   │ Idle │ ──────────────────→ │ Open │ ────────────────────────→ │ Presenting │
   └─────┘                     └──────┘                           └────────────┘
      ↑                            ↑                                    │
      │     file closed            │       slideshow ended              │
      └────────────────────────────┘←───────────────────────────────────┘
```

- **Idle**: No presentation file open. `is_open=false`, `is_presenting=false`
- **Open**: Presentation file open but slideshow not running. `is_open=true`, `is_presenting=false`
- **Presenting**: Slideshow active. `is_open=true`, `is_presenting=true`, slide numbers valid, presenter notes available

## State Caching

The `StateManager` caches presentation state to avoid redundant queries to the presentation software. State is refreshed:

- After every slide navigation command
- On `/clicker/status` request
- On `/clicker/refresh` command (forces full re-query)
- Periodically (configurable polling interval)
