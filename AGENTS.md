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

The bridge is a single Tauri app: `apps/bridge/`. The core engine lives
in-crate under `src-tauri/src/bridge/` (no separate crate, no headless binary).

**Tauri layer**: `apps/bridge/src-tauri/`
- `src/lib.rs` - Tauri setup, event forwarding
- `src/commands/` - Thin Tauri command wrappers around `BridgeCore`
- `src/state.rs` - Tauri-managed `BridgeState`

**Core engine**: `apps/bridge/src-tauri/src/bridge/`
- `mod.rs` - `BridgeCore` coordinator, `BridgeEvent`, service startup
- `state.rs` - `CoreState` shared services
- `usb/linux.rs` - Passive `evdev` input capture (no grab, no udev rules)
- `osc/` - OSC sender and feedback listener
- `config.rs` - Config persistence via `dirs`

**Frontend**: `apps/bridge/src/`
- `routes/+page.svelte` - Orchestrator (state, listeners, invoke calls)
- `lib/components/` - Extracted UI cards
- `lib/usePeers.svelte.ts` - mDNS peer store

**Build**: `cd apps/bridge && bun tauri dev`
