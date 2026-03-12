# AGENTS.md

Specialized knowledge areas for this codebase.

## Rust/Tauri

**Scope**: `apps/desktop/src-tauri/`

**Key Files**:
- `src/lib.rs` - Tauri commands
- `src/adapters/mod.rs` - `PresentationAdapter` trait
- `src/osc/server.rs` - OSC UDP server
- `src/osc/state_manager.rs` - State caching

**Patterns**:
- `#[tauri::command]` for frontend-callable functions
- `#[cfg(target_os = "...")]` for platform-specific code
- `Result<T, String>` at Tauri boundaries

**Test**: `cd apps/desktop/src-tauri && cargo test`

## Frontend

**Scope**: `apps/desktop/src/`

**Key Files**:
- `routes/+page.svelte` - Main page
- `lib/components/` - UI components
- `lib/types.ts` - TypeScript interfaces

**Tauri calls**:
```typescript
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
await invoke('command_name', { arg: value });
await listen('event-name', (event) => { /* handle */ });
```

## Platform Adapters

| Platform | Adapter | Method |
|----------|---------|--------|
| macOS | `powerpoint.rs`, `keynote.rs` | AppleScript |
| Windows | `powerpoint_windows.rs` | COM |
| Linux | `libreoffice.rs` | TCP socket |

## Bridge

**Scope**: `apps/bridge/`

**Key Files**:
- `src/rpi_osc_bridge/bridge.py` - Main bridge logic, device detection, OSC sending
- `src/rpi_osc_bridge/config_server.py` - Web config UI server
- `config/config.example.json` - Config template
- `scripts/install.sh` - Deployment installer
- `scripts/build-installer.sh` - Self-extracting installer builder

**Build**: `cd apps/bridge/scripts && ./build-installer.sh`
