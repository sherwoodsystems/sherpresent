# SherPresent Roadmap

## Strategic Direction

SherPresent's unique value is the **RPi bridge**: turning dumb USB presentation clickers into networked, OSC-speaking controllers. The Windows PowerPoint side is already solved by [OSCPoint](https://github.com/phuvf/oscpoint), a mature MIT-licensed VSTO add-in. We should not compete with it — we should interoperate with it.

**Core thesis**: SherPresent becomes "OSCPoint for macOS (and LibreOffice/Canva)" while the RPi bridge remains the universal glue that connects clickers to either OSCPoint (Windows) or SherPresent desktop (macOS/Linux).

---

## Phase 1: Adopt the oscpoint OSC Schema

**Goal**: Make SherPresent speak the same `/oscpoint/...` address space as OSCPoint so the bridge works with either target transparently.

### Tasks
- Replace `/clicker/...` addresses with oscpoint equivalents:
  - `/clicker/next` → `/oscpoint/next`
  - `/clicker/prev` / `/clicker/previous` → `/oscpoint/previous`
  - `/clicker/goto <n>` → `/oscpoint/goto/slide <n>`
  - `/clicker/status` / `/clicker/refresh` → feedback query handled via `/oscpoint/state/...`
- Update outgoing feedback to match oscpoint paths:
  - `/oscpoint/slideshow/currentslide`
  - `/oscpoint/slideshow/slidecount`
  - `/oscpoint/presentation/name`
  - `/oscpoint/slideshow/notes`
  - `/oscpoint/v2/event` for lifecycle events
- Update desktop OSC server (`osc/server.rs`, `osc/messages.rs`, `osc/state_manager.rs`)
- Update RPi bridge OSC sender (`bridge/osc.py`)
- Update Bitfocus Companion module (`companion-module-sherpresent/osc.ts`, `actions.ts`)
- **Decision**: clean cutover, no backward-compatible `/clicker/` alias.

---

## Phase 2: RPi Bridge Production Polish

**Goal**: The bridge is the product. Make it discoverable, reliable, and trivial to configure.

### Tasks
- **Auto-discovery**: detect both OSCPoint instances (ports 35550/35551) and SherPresent desktops (`_sher-present._udp.local.`)
- **Multiple targets**: send commands from each USB slot to one or more targets simultaneously
- **Resilience**: reconnect logic, per-target status, clear error reporting in the web UI
- **Web config UI**: show discovered targets, allow one-click assignment per USB slot, display online/offline status
- **Setup documentation**: end-user guide for clicker-base → RPi → network → target app
- **Test matrix**:
  - bridge → OSCPoint on Windows
  - bridge → SherPresent on macOS
  - bridge → SherPresent + OSCPoint simultaneously
  - bridge failover between targets

---

## Phase 3: macOS PowerPoint Adapter Parity

**Goal**: Fill the gap OSCPoint explicitly leaves — there is no macOS version.

### Tasks
- **Re-enable presenter notes** on macOS PowerPoint; the new NSAppleScript FFI path is ~5ms vs the old `osascript -e` ~160ms, so the prior "too slow" concern should be gone
- **Slideshow control**: black screen, white screen, pause/resume
- **Section awareness**: expose PowerPoint sections in state/feedback if AppleScript exposes them
- **Event feedback**: emit `/oscpoint/v2/event` for `presentation_open`, `slideshow_begin`, `slideshow_end`, `slideshow_next_slide`
- **JSON presentation state**: match OSCPoint's `/oscpoint/v2/presentations` format

---

## Phase 4: Maintain the Windows PowerPoint Adapter as a Fallback

**Goal**: Keep the existing Windows COM adapter alive for users who cannot install OSCPoint, but do not expand it.

### Tasks
- Make it speak the oscpoint schema (Phase 1 covers this)
- Update docs to state OSCPoint is recommended; SherPresent's Windows adapter is fallback only
- Fix known gaps only if trivial:
  - Notes zoom (`PresenterViewWindow.PresenterTool.NotesZoom`)

---

## Phase 5: Companion Module Polish

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

---

## Open Questions

1. Should SherPresent's desktop app listen on OSCPoint's default port (35551) as an option, or keep its own configurable port?
2. How much of OSCPoint's JSON presentation state can macOS AppleScript actually populate?
3. Should the RPi bridge ship a one-button "pair with discovered target" flow, or keep per-slot manual assignment?
4. Do we keep the `/clicker/` Companion module brand name or rename it to `/oscpoint/` to match the schema?
