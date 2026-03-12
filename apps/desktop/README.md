# Sher Present

A desktop application for controlling presentation software (PowerPoint, Keynote, LibreOffice Impress) via OSC (Open Sound Control). Perfect for integrating with Bitfocus Companion, QLab, or any OSC-capable control system.

## Supported Platforms

- **Windows**: Microsoft PowerPoint (via COM automation)
- **macOS**: Microsoft PowerPoint and Apple Keynote (via AppleScript)
- **Linux**: LibreOffice Impress (via Impress Remote Protocol)

## OSC Protocol Reference

### Network Configuration

| Setting | Value |
|---------|-------|
| **Listen Port** | 9000 (incoming commands) |
| **Feedback Port** | 9001 (outgoing state) |
| **Protocol** | UDP |

### Incoming Commands (send to port 9000)

| OSC Address | Description |
|-------------|-------------|
| `/clicker/next` | Advance to next slide |
| `/clicker/prev` | Go to previous slide |
| `/clicker/previous` | Go to previous slide (alias) |
| `/clicker/status` | Request full state update |
| `/clicker/refresh` | Force state re-sync from presentation app |
| `/clicker/zoom` | Query current notes zoom level |
| `/clicker/zoomIn` | Increase notes zoom (macOS only) |
| `/clicker/zoomOut` | Decrease notes zoom (macOS only) |

### Outgoing Feedback (sent on port 9001)

| OSC Address | Type | Description |
|-------------|------|-------------|
| `/clicker/state/presenting` | int | `1` if slideshow is active, `0` otherwise |
| `/clicker/state/open` | int | `1` if presentation file is open, `0` otherwise |
| `/clicker/slide/current` | int | Current slide number (1-indexed) |
| `/clicker/slide/total` | int | Total number of slides |
| `/clicker/zoom/level` | int | Notes zoom percentage (100, 150, 200, 300, 400) or `0` if unavailable |

### Example: Bitfocus Companion Setup

1. Add an **OSC** connection pointing to the machine running Sher Present
2. Set the **Target Port** to `9000`
3. Set the **Feedback Port** to `9001`
4. Create buttons with actions:
   - Next slide: Send `/clicker/next`
   - Previous slide: Send `/clicker/prev`
5. Create feedbacks to display slide info:
   - Subscribe to `/clicker/slide/current` for current slide number
   - Subscribe to `/clicker/state/presenting` for slideshow status

## Multi-Presenter Mode (Broadcast)

For setups with multiple USB presentation clickers or backup systems, enable broadcast mode:

### Configuration

1. Open Settings and enable **Channel sync**
2. Enable **Broadcast mode**
3. Select a channel name (e.g., `main`, `backup`)
4. Default broadcast port: **9002**

### Channel-Based OSC Commands (port 9002)

| OSC Address | Description |
|-------------|-------------|
| `/clicker/<channel>/next` | Advance to next slide |
| `/clicker/<channel>/prev` | Go to previous slide |
| `/clicker/<channel>/goto` | Jump to slide N |
| `/clicker/<channel>/status` | Request state update |

### Channel Feedback (broadcast on port 9002)

| OSC Address | Type | Description |
|-------------|------|-------------|
| `/clicker/<channel>/state/presenting` | int | `1` if slideshow active |
| `/clicker/<channel>/state/open` | int | `1` if presentation open |
| `/clicker/<channel>/state/slide` | int, int | Current slide, total slides |
| `/clicker/<channel>/state/zoom` | int | Notes zoom level |

Valid channels: `main`, `backup`, `keynote1-9`, `aux1-9`

### rpi-osc-bridge Integration

Sher Present works with the `rpi-osc-bridge` Python tool to convert USB presentation clickers into OSC commands. When a bridge device sends commands, it appears in the UI with an orange "Bridge" badge.

See the `rpi-osc-bridge/` directory in the parent repository for setup instructions.

## Development

### Prerequisites

- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://rustup.rs/)
- Platform-specific requirements:
  - **Windows**: Visual Studio C++ Build Tools
  - **macOS**: Xcode Command Line Tools
  - **Linux**: `libgtk-3-dev`, `libwebkit2gtk-4.0-dev`, `libappindicator3-dev`

### Running in Development

```bash
npm install
npm run tauri dev
```

### Building for Production

```bash
npm run tauri build
```

## Platform-Specific Notes

### Windows
- PowerPoint must be running for the app to detect presentations
- Uses COM automation (same as VBA macros)

### macOS
- Grant accessibility permissions when prompted
- Works with both PowerPoint and Keynote

### Linux (LibreOffice Impress)
- Enable remote control: **Slide Show > Slide Show Settings > Enable remote control**
- A slideshow must be running for the connection to work
- Note: Zoom control is not available for LibreOffice

## License

MIT
