# sherpresent-output

Video output helper: renders a 1920×1080 frame and publishes it to native
video sinks. Today that's **Syphon** (macOS, same machine). The `FrameSink`
protocol is the seam for NDI or others later.

`--content` picks what it draws (a `FrameContent`):
- `captions` (default): the live captions on a transparent frame.
- `notes`: the current slide's notes on an opaque frame, styled like the
  stage view, with an Ontime timer strip when the app sends a timer.

Driven by `apps/desktop/src-tauri/src/output/syphon.rs`; one process per
output.

## Build

```bash
cd apps/desktop
bun run macos:sidecar   # builds both helpers into src-tauri/binaries/
```

Command Line Tools are enough — no Xcode. Syphon is vendored as source
(`Sources/Syphon/VENDORED.md`).

## Protocol (version 2)

- **stdin**: NDJSON, EOF = graceful stop. For `captions`, exactly the
  messages the web overlay's `/api/captions/ws` socket carries: `settings`
  (`OverlaySettings`), `status`, `replay`, `segment`, `clear` (the app's
  silence timeout fired). For `notes`, the stage view's `/api/ws` messages
  (`{"type":"status"|"notes","payload":…}`) plus
  `{"type":"timer","payload":{connected,current,playback,title}|null}`.
- **stdout**: NDJSON — `ready` (with sink list), `sinks` (whenever a
  receiver connects or disconnects), `error` (`fatal` decides whether Rust
  respawns).
- **stderr**: plain-text logs.

Frames are only rendered and published when the visible text or styling
changes; idle costs nothing.

## Layout

`CaptionLayout` mirrors `assets/captions.html`: `fontSize` px at 1920 wide,
1.22 line height, `width`% block, balanced wrapping, `safeArea`% bottom
inset, and a window of exactly `maxLines` rows keeping the newest.
`CaptionState` is a port of the overlay's line handling, so both outputs
show the same text.

## Checking it

Offline render, no Syphon:

```bash
printf '%s\n' \
  '{"type":"settings","settings":{"fontSize":56,"maxLines":2,"safeArea":5,"width":80,"chromaColor":"#00B140","shadow":false}}' \
  '{"type":"segment","segment":{"id":1,"source":"Hello there","translated":"","final":true,"timestamp":0}}' \
  | .build/release/sherpresent-output --protocol 2 --render-png /tmp/frame.png
```

Round trip through Syphon, with the dev-only receiver in this package:

```bash
BIN=$(swift build -c release --show-bin-path)
( printf '%s\n' '{"type":"segment","segment":{"id":1,"source":"Hello","translated":"","final":true,"timestamp":0}}'; sleep 4 ) \
  | $BIN/sherpresent-output --protocol 2 --name "Test" &
sleep 1; $BIN/syphon-probe "Test" /tmp/received.png
```

`syphon-probe` prints pixel stats: `clear` / `opaque` / `partial` counts,
`notPremultiplied` (must be 0), and `captionsAtBottom` (must be `true`).

Orientation: Syphon surfaces are **bottom-up** (OpenGL convention — memory
row 0 is the bottom of the picture), which is how OBS and other receivers read
them. Our frames are top-down Metal textures, so `SyphonSink` publishes with
`flipped: true`. The probe reads bottom-up like a real receiver, so a
regression shows up as `captionsAtBottom: false`.

## Adding NDI

1. A `FrameSink` that reads `frame.surface` (BGRA, premultiplied) and sends it.
2. A `case "ndi"` in `makeSinks`.
3. On the Rust side, a sibling of `output/syphon.rs` (or a `--sink` list on
   the same helper) and an `ndi` entry next to each `syphon` config.

NDI is cross-platform, so a Windows/Linux version would more likely live in
Rust than in this macOS-only helper; the stdin protocol above is what it
would consume either way.
