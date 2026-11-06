# OSC Protocol Specification

## Transport

- Protocol: OSC 1.0 over UDP
- Encoding: `rosc` (Rust), `python-osc` (Python)
- All addresses use the `/clicker/` namespace prefix

## Direct Mode (Default)

Point-to-point communication between a single controller and a desktop app.

- **Port**: 9000. The desktop app receives commands here and sends feedback from here.
- **Connecting**: send any command. The sender gets feedback at the address and port it
  sent from, until 15 seconds pass without a command from it. Clients send
  `/clicker/status` every few seconds to stay connected; replies also tell them the
  desktop app is alive. Use one socket to send and receive.
- **Listen-only devices**: add them to the configured feedback destinations, which
  always get feedback (default port 9001).

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
| `/clicker/discover` | none | Find desktop apps: send as a UDP broadcast to the subnet (e.g. `192.168.1.255:9000`). Replied to with `/clicker/here`, and doesn't connect |
| `/oscpoint/next`, `/oscpoint/previous` | none | Aliases of next and previous, as sent by the bridge |

### Outgoing Feedback

Sent by the desktop app to connected clients and configured destinations.

| Address | Arguments | Description |
|---------|-----------|-------------|
| `/clicker/state/presenting` | int (0\|1) | Is slideshow active? |
| `/clicker/state/open` | int (0\|1) | Is presentation file open? |
| `/clicker/slide/current` | int | Current slide number (0 if not presenting) |
| `/clicker/slide/total` | int | Total slide count |
| `/clicker/zoom/level` | int | Current zoom percentage (0 if unavailable) |
| `/clicker/here` | string | The desktop app's name; sent only to the sender of `/clicker/discover` |
