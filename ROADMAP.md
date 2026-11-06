# SherPresent Roadmap

## Strategic Direction

SherPresent's unique value is the **cross-platform bridge**: turning dumb USB presentation clickers into networked, OSC-speaking controllers. The Windows PowerPoint side is already solved by [OSCPoint](https://github.com/phuvf/oscpoint), a mature MIT-licensed VSTO add-in. We should not compete with it — we should interoperate with it.

**Core thesis**: SherPresent becomes "OSCPoint for macOS (and LibreOffice/Canva)" while the bridge remains the universal glue that connects clickers to either OSCPoint (Windows) or SherPresent desktop (macOS/Linux).

**Bridge strategy**: A Tauri-based cross-platform bridge replaces the Linux-only Python bridge. It runs on Raspberry Pi, Windows stick PCs, Mac minis, or the presenter's own laptop. The bridge core (`crates/sherpresent-bridge-core/`) is shared between a Tauri GUI app (`apps/bridge/`) and a headless binary (`apps/bridge-headless/`) for Raspberry Pi / server deployments. The Python bridge has been removed.

---

## Phase 1: Tauri Bridge + Oscpoint Schema

**Goal**: A cross-platform bridge that speaks oscpoint natively. Replace the Python bridge with a Tauri app sharing code with the existing desktop app.

### 1a: Cross-Platform Tauri Bridge App

- **Platform-native USB HID detection**: replace Linux-only `evdev` with Rust HID crates
  - Linux: evdev port to Rust (priority, reaches parity first)
  - Windows: Win32 HID API
  - macOS: IOKit HID

- **Shared Rust core**: extract into a workspace-level `sherpresent-core` crate
  - `mdns-sd` service registration and browsing (`_sher-present._udp.local.`)
  - OSC message types and schema (oscpoint paths)
  - Network interface resolution
  - Peer list data structures

- **Native webview config UI**: Svelte-based, same stack as the desktop app
  - Show discovered targets (OSCPoint instances on :35550/:35551, SherPresent desktops)
  - Per-USB-slot one-click target assignment
  - Online/offline status per target
  - No separate HTTP server or `config_server.py` — everything in one binary

- **System tray integration**: headless operation with launch-on-login, background service lifecycle, cross-platform config dirs via Tauri path resolver

- **Discovery and resilience**: auto-detect targets via mDNS, send commands from each USB slot to one or more targets simultaneously, reconnect logic, per-target status, clear error reporting in the UI

### 1b: Oscpoint Schema Migration

- Replace `/clicker/...` addresses with oscpoint equivalents:
  - `/clicker/next` → `/oscpoint/next`
  - `/clicker/prev` / `/clicker/previous` → `/oscpoint/previous`
  - `/clicker/goto <n>` → `/oscpoint/goto/slide <n>`
  - `/clicker/status` / `/clicker/refresh` → feedback via `/oscpoint/state/...`

- Update outgoing feedback to match oscpoint paths:
  - `/oscpoint/slideshow/currentslide`
  - `/oscpoint/slideshow/slidecount`
  - `/oscpoint/presentation/name`
  - `/oscpoint/slideshow/notes`
  - `/oscpoint/v2/event` for lifecycle events

- Update desktop OSC server (`osc/server.rs`, `osc/messages.rs`, `osc/state_manager.rs`)
- Update Companion module (`companion-module-sherpresent/osc.ts`, `actions.ts`)
- **Decision**: clean cutover, no backward-compatible `/clicker/` alias. Tauri bridge speaks oscpoint from day one.

### 1c: Linux-First Rollout and Python Bridge Deprecation

- **Linux first**: get the Tauri bridge working on Raspberry Pi as the primary target
- **Then expand**: add Windows and macOS support once Linux is solid
- **Headless binary**: `apps/bridge-headless/` runs as a systemd service on Raspberry Pi OS without a GUI

### 1d: Testing and Documentation

- **Test matrix**:
  - Tauri bridge (Linux) → OSCPoint on Windows
  - Tauri bridge (Linux) → SherPresent desktop on macOS
  - Tauri bridge (Linux) → SherPresent desktop on Linux
  - Tauri bridge → multiple targets simultaneously
  - Tauri bridge failover between targets

- **Setup documentation**: end-user guide for clicker → bridge device → network → target app, covering Raspberry Pi, Windows stick PC, and macOS bridge hosts

---

## Phase 2: macOS PowerPoint Adapter Parity

**Goal**: Fill the gap OSCPoint explicitly leaves — there is no macOS version.

### Tasks
- **Re-enable presenter notes** on macOS PowerPoint; the new NSAppleScript FFI path is ~5ms vs the old `osascript -e` ~160ms, so the prior "too slow" concern should be gone
- **Slideshow control**: black screen, white screen, pause/resume
- **Section awareness**: expose PowerPoint sections in state/feedback if AppleScript exposes them
- **Event feedback**: emit `/oscpoint/v2/event` for `presentation_open`, `slideshow_begin`, `slideshow_end`, `slideshow_next_slide`
- **JSON presentation state**: match OSCPoint's `/oscpoint/v2/presentations` format

---

## Phase 3: Maintain the Windows PowerPoint Adapter as a Fallback

**Goal**: Keep the existing Windows COM adapter alive for users who cannot install OSCPoint, but do not expand it.

### Tasks
- Make it speak the oscpoint schema (Phase 1 covers this)
- Update docs to state OSCPoint is recommended; SherPresent's Windows adapter is fallback only
- Fix known gaps only if trivial:
  - Notes zoom (`PresenterViewWindow.PresenterTool.NotesZoom`)

---

## Phase 4: Companion Module Polish

**Goal**: Make the Bitfocus Companion module production-ready for Stream Deck users.

### Tasks
- Verify actions/feedbacks match oscpoint schema
- Refresh presets to mirror OSCPoint's Companion module patterns
- Test against both OSCPoint (Windows) and SherPresent (macOS) targets
- Add feedback-driven button states: current slide, total slides, presenting yes/no

---

## What We Will Not Build

Avoid reinventing the wheel. OSCPoint already owns these for Windows PowerPoint:

| Skip | Reason |
|------|--------|
| Full Windows PowerPoint OSC bridge | OSCPoint exists and is better at this |
| Media control (play/pause/goto time) | Requires running inside PowerPoint's process; out of scope for an external adapter |
| File handling (open/close presentations from folder) | Complex, requires in-process add-in; niche use case |
| `/clicker/{channel}/...` broadcast addressing | oscpoint doesn't use it; direct addressing is enough |
| Canva adapter expansion | Low priority; stage view export already covers notes use case |
| P2P networking layer (Iroh, QUIC, DHT, relay) | OSC-over-UDP on a single subnet is sufficient; revisit only if WAN or multi-subnet becomes a requirement |
| Python bridge maintenance | Removed — replaced by Rust bridge core + headless binary |

---

## Open Questions

1. Should SherPresent's desktop app listen on OSCPoint's default port (35551) as an option, or keep its own configurable port?
2. How much of OSCPoint's JSON presentation state can macOS AppleScript actually populate?
3. Should the Tauri bridge ship a one-button "pair with discovered target" flow, or keep per-slot manual assignment?
4. Do we keep the `/clicker/` Companion module brand name or rename it to `/oscpoint/` to match the schema?
5. Should the shared code live in a workspace-level `sherpresent-core` crate, or stay co-located in the desktop app's `src-tauri/` until the bridge needs it?
6. ~~What's the deprecation timeline for the Python bridge: remove immediately after Linux parity, or keep one release as an escape hatch?~~ (Done: removed.)
