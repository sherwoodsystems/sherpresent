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

## Live Captions

Microphone → streaming translation → chroma-key overlay served on the LAN.

**Scope**: `apps/desktop/src-tauri/src/captions/`

- `mod.rs` - `CaptionEngine`, `CaptionSegment`, `CaptionSinks` fan-out
- `audio.rs` - cpal capture on a dedicated OS thread, downmix + rubato resample to 16 kHz mono i16
- `provider/mod.rs` - `CaptionProvider` trait + `build()` selection
- `provider/gemini.rs` - Gemini Live `BidiGenerateContent` WSS client
- `provider/apple.rs` - macOS 26+ on-device provider driving a Swift sidecar
- `../commands/captions.rs` - Tauri command wrappers
- `../../assets/captions.html` - overlay page, served by `webserver.rs` at `/captions`

**Delivery**: the overlay is a page on the existing axum server, not a Tauri
window. `/captions` renders the HTML; `/api/captions/ws` is a separate socket
from `/api/ws` so the overlay never receives slide/notes traffic.

Retune without rebuilding via query params:
`/captions?bg=00b140&size=64&lines=2&safe=5`, plus `bg=transparent` for OBS,
`text=source|both` and `clean=1`.

**Providers**: `apple` is the default on a capable Mac (macOS 26+, Apple
Silicon; free/offline/no key) — `config::default_caption_provider` picks it via
`apple::is_platform_supported`. `gemini` (cross-platform, billed) is the
fallback everywhere else. `openai` is a stub.

**Apple provider**: `SpeechAnalyzer` + `TranslationSession` are Swift-only with
no C ABI, so `apple.rs` drives a helper process built from
`apps/desktop/sidecars/speech-macos/` (see its README). Rust pipes the existing
cpal PCM into the helper's stdin — the helper never opens the mic, so there is
still one device claim and one TCC prompt. `bun run macos:sidecar` builds it;
the script no-ops off macOS so Linux `cargo build` stays clean.

**`Delta` vs `Replace`**: `ProviderEvent::Delta` appends (Gemini);
`ProviderEvent::Replace` assigns the whole line (Apple). On-device recognizers
emit *revisable* hypotheses — "hello word" becomes "hello world" — so diffing
them into appends corrupts text. A provider must pick one and stay with it.

**Gotchas**:
- cpal `Stream` is `!Send` — it lives on its own `std::thread`, never in `AppState`.
- Apple translation language packs cannot be installed programmatically; the
  provider fails with a Settings deeplink instead. Speech models *do* download
  on their own via `AssetInventory`.
- `SHERPRESENT_SPEECH_BIN` overrides sidecar discovery and skips the OS/arch
  preflight — how the Linux tests drive a scripted fake helper.
- Provider sessions die every ~10 min by design; reconnection is the normal
  path, driven by `sessionResumption` handles. Test runs of 25+ min.
- API keys are stored **plaintext** in `config.json` (deliberate).
- Linux builds need `libasound2-dev` for cpal's ALSA backend.
- macOS needs `NSMicrophoneUsageDescription` (`src-tauri/Info.plist`) and the
  `com.apple.security.device.audio-input` entitlement for signed builds.

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
