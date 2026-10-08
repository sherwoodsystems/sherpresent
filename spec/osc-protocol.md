# OSC Protocol Specification

## Transport

- Protocol: OSC 1.0 over UDP
- Encoding: `rosc` (Rust), `python-osc` (Python)
- All addresses use the `/clicker/` namespace prefix

## Direct Mode (Default)

Point-to-point communication between a single controller and a desktop app.

- **Receive port**: 9000 (desktop app listens here)
- **Feedback port**: 9001 (desktop app sends feedback here)

### Incoming Commands

Received by the desktop app on port 9000.

| Address | Arguments | Description |
|---------|-----------|-------------|
| `/clicker/next` | none | Advance to next slide |
| `/clicker/prev` | none | Go to previous slide |
| `/clicker/previous` | none | Alias for `/clicker/prev` |
| `/clicker/zoom` | none | Query current notes zoom level |
| `/clicker/zoomIn` | none | Increase notes zoom level |
| `/clicker/zoomOut` | none | Decrease notes zoom level |
| `/clicker/status` | none | Request full state update |
| `/clicker/refresh` | none | Force state re-sync from presentation app |
| `/clicker/scrollUp` | none | Scroll the notes views (stage page, Syphon notes) up |
| `/clicker/scrollDown` | none | Scroll the notes views down |
| `/clicker/notesPage` | none | Teleprompter: next screenful of the current slide's notes in every notes view, wrapping to the top after the last; resets on slide change |

### Outgoing Feedback

Sent by the desktop app to the feedback address on port 9001.

| Address | Arguments | Description |
|---------|-----------|-------------|
| `/clicker/state/presenting` | int (0\|1) | Is slideshow active? |
| `/clicker/state/open` | int (0\|1) | Is presentation file open? |
| `/clicker/slide/current` | int | Current slide number (0 if not presenting) |
| `/clicker/slide/total` | int | Total slide count |
| `/clicker/zoom/level` | int | Current zoom percentage (0 if unavailable) |
