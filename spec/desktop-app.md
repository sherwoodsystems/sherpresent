# Desktop App Specification

## Overview

Tauri v2 desktop application that receives OSC commands and controls local presentation software.

## Supported Software

| Software | Platform | Adapter |
|----------|----------|---------|
| Microsoft PowerPoint | macOS | AppleScript |
| Microsoft PowerPoint | Windows | COM automation |
| Keynote | macOS | AppleScript |
| LibreOffice Impress | Linux | TCP socket (port 2002) |

## OSC Server Lifecycle

1. App starts → binds UDP socket on receive port (default 9000)
2. If broadcast mode enabled → also binds on broadcast port (default 9002)
3. Listens for incoming OSC commands
4. On command → dispatches to the selected presentation adapter
5. State changes → sends feedback messages to feedback port
6. On shutdown → sends channel leave announcement, closes sockets

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
- **Presenting**: Slideshow active. `is_open=true`, `is_presenting=true`, slide numbers valid

## State Caching

The `StateManager` caches presentation state to avoid redundant queries to the presentation software. State is refreshed:

- After every slide navigation command
- On `/clicker/status` request
- On `/clicker/refresh` command (forces full re-query)
- Periodically (configurable polling interval)

## Channel Sync

When broadcast mode is enabled:

1. App announces presence via `/clicker/channel/announce`
2. Sends periodic heartbeats via `/clicker/channel/heartbeat`
3. Forwards received commands to peers via `/clicker/channel/cmd/*`
4. Filters incoming channel commands by configured channel name
5. Sends channel-aware feedback (`/clicker/{channel}/state/*`)
