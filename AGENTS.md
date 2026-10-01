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

**Test**: `cargo test --workspace` (from the repo root); everything: `scripts/check-all.sh`

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

Styling (`OverlaySettings`: size, lines, safe area, width, key colour, shadow, all caps,
CC box and its colour, clear timeout) is pushed live over the socket as a `settings` message
whenever Settings changes — sliders call `preview_caption_overlay` (one per
animation frame) ahead of the debounced save. `OverlaySettings::clamped` is
the only place ranges live. `lines` means visual rows: the caption box is
exactly that many rows tall and long sentences roll up out of the top.

Pin any value per-URL (a pinned value ignores live updates):
`/captions?bg=00b140&size=64&lines=2&safe=5&width=80&shadow=1&caps=1&box=1&boxcolor=222`, plus
`bg=transparent` for OBS, `text=source|both` and `clean=1`. The clear timeout
is deliberately not pinnable: it's one app-wide timer (see below). Pins are applied server-side by
`OverlaySettings::with_overrides` — the page forwards its query string on the
socket — so the page itself never parses styling params.

The silence timeout is owned by Rust (`captions::run_silence_clear`): it
empties the replay buffer and broadcasts `clear`, so every output blanks
together and a reconnecting overlay doesn't resurrect stale lines. Renderers
just handle the message. Opening messages (settings, status, replay) come from
`CaptionSinks::opening_messages` for the overlay socket and native outputs
alike.

**Native outputs** (`src/output/`): optional Syphon video sources, each a
helper process with its own server, fed the *same* NDJSON as the matching web
page (`output/feed.rs`) so they match it line for line:
- **captions** (`captions.outputs.syphon` and `syphon2`): the overlay
  socket's messages (segments, replay, status, `OverlaySettings`), transparent
  frame. Each has a `text` of `translated|source|both` (the helper's `--text`,
  same as the overlay's `?text=`), so one source per language can be keyed
  separately; `syphon2` defaults to the original.
- **notes** (`webServer.syphon`): the stage view's `/api/ws` `status` + `notes`,
  plus `timer` from `src/ontime.rs` (a Rust Ontime client reading the same
  `runtime-data` the stage page does). Opaque frame: current slide's notes,
  auto-fitted, with a timer strip when an Ontime host is set.
- **slideshow** (`webServer.slideshowSyphon`): PowerPoint's slide show
  window (never Presenter View), captured by the helper itself with
  ScreenCaptureKit while a show runs. It's fed nothing and holds the last slide
  between shows. Needs Screen Recording permission. Its `capture` state
  (`waiting|capturing|denied`) becomes the output's status message.

`Outputs::reconcile` starts/stops/renames them on startup and every
`save_config`; they run independently of the caption engine and the
presentation so receivers stay wired up between talks. Status is pushed as
`outputs-status`.

- `output/syphon.rs` drives `sherpresent-output`
  (`apps/desktop/sidecars/caption-output-macos/`, `--content captions|notes|slideshow`):
  Core Text render onto a 1920x1080 IOSurface, published via a vendored, source-built
  Syphon (`Sources/Syphon/VENDORED.md` — one patch, shaders compiled at
  runtime so no Xcode is needed). macOS 13+, Apple Silicon.
  `SHERPRESENT_OUTPUT_BIN` overrides discovery.
- Adding NDI: a `FrameSink` in the helper (or a Rust sender for
  cross-platform) plus a sibling config entry. See the helper README.
- `syphon-probe` (dev-only target in that package) receives a frame and prints
  alpha stats — the end-to-end check.

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

**Straight captions**: target = source language (region ignored) means no
translation — `CaptionsConfig::translates`. Apple skips its
`TranslationSession` (`--no-translate`), and the engine drops any `translated`
text so every renderer shows the verbatim transcript.

**`Delta` vs `Replace`**: `ProviderEvent::Delta` appends (Gemini);
`ProviderEvent::Replace` assigns the whole line (Apple). On-device recognizers
emit *revisable* hypotheses — "hello word" becomes "hello world" — so diffing
them into appends corrupts text. A provider must pick one and stay with it.
`Replace` translations are shown only up to the words two consecutive
results agree on (`SettledText` in `captions/mod.rs`), with the full text on
turn completion — so live translation grows word by word instead of
rewriting words the audience is reading.

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
