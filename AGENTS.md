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

The bridge has two entry points sharing `crates/sherpresent-bridge-core/`:

**Tauri GUI**: `apps/bridge/`
- `src-tauri/src/lib.rs` - Tauri setup, event forwarding
- `src-tauri/src/commands/` - Thin Tauri command wrappers
- `src/routes/+page.svelte` - Svelte configuration UI

**Headless binary**: `apps/bridge-headless/`
- `src/main.rs` - CLI entry point, signal handling
- `sherpresent-bridge.service` - systemd service file

**Shared core**: `crates/sherpresent-bridge-core/`
- `src/lib.rs` - `BridgeCore` coordinator
- `src/usb/linux.rs` - Passive `evdev` input capture (no grab, no udev rules)
- `src/osc/` - OSC sender and feedback listener
- `src/http/` - HTTP config API (axum)
- `src/config.rs` - Config persistence via `dirs`

**Build**:
- Tauri GUI: `cd apps/bridge && bun tauri dev`
- Headless: `cd apps/bridge-headless && cargo build --release`
