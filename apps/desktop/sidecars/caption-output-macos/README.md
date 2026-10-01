# sherpresent-output

Video output helper: renders a 1920×1080 frame and publishes it to native
video sinks. Today that's **Syphon** (macOS, same machine). The `FrameSink`
protocol is the seam for NDI or others later.

`--content` picks what it draws (a `FrameContent`):
- `captions` (default): the live captions on a transparent frame.
- `notes`: the current slide's notes on an opaque frame, styled like the
  stage view, with an Ontime timer strip when the app sends a timer.
- `slideshow`: PowerPoint's slide show window, captured with
  ScreenCaptureKit (`SlideshowCapture`) and scaled into the 1920×1080 frame.
  Captured only while a show runs; between shows nothing is published, so
  receivers hold the last slide. Presenter View is never captured.

Driven by `apps/desktop/src-tauri/src/output/syphon.rs`; one process per
output.

## Build

```bash
cd apps/desktop
bun run macos:sidecar   # builds both helpers into src-tauri/binaries/
```

Command Line Tools are enough — no Xcode. Syphon is vendored as source
(`Sources/Syphon/VENDORED.md`).

## Protocol (version 4)

- **stdin**: NDJSON, EOF = graceful stop. For `captions`, exactly the
  messages the web overlay's `/api/captions/ws` socket carries: `settings`
  (`OverlaySettings`), `status`, `replay`, `segment`, `clear` (the app's
  silence timeout fired). For `notes`, the stage view's `/api/ws` messages
  (`{"type":"status"|"notes","payload":…}`) plus
  `{"type":"timer","payload":{connected,current,playback,title}|null}`.
  For `slideshow`, nothing (stdin only carries EOF).
- **stdout**: NDJSON — `ready` (with sink list), `sinks` (whenever a
  receiver connects or disconnects), `error` (`fatal` decides whether Rust
  respawns), and for `slideshow` `capture` with `state`
  `waiting|capturing|denied` whenever it changes.
- **stderr**: plain-text logs.

Frames are only rendered and published when the visible text or styling
changes; idle costs nothing. (`slideshow` publishes captured frames at up to
30 fps while the slide is changing, and none while it's still.)

## Slideshow capture

- The window match is `SlideshowCapture.isSlideshowWindow`: PowerPoint's
  bundle id and a title containing "Slide Show" but not "Presenter". On every
  change, stderr lists the PowerPoint windows the helper can see. Look there
  first if a show isn't picked up (a localized PowerPoint, for example).
- The window is shaped like its display (16:10 on a MacBook), and
  PowerPoint letterboxes the slide inside it, below the notch.
  `SlideshowCapture.slideRect` crops to that 16:9 area so the slide fills the
  frame. A 4:3 deck loses its top and bottom edges.
- **Screen Recording permission** has no Info.plist key or entitlement.
  macOS attributes it to the responsible app: SherPresent.app in a build,
  the terminal under `bun tauri dev`. Until it's granted the helper reports
  `denied` and keeps polling, so granting it takes effect without a restart.
  macOS 15+ periodically asks to re-confirm.

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
  | .build/release/sherpresent-output --protocol 4 --render-png /tmp/frame.png
```

Round trip through Syphon, with the dev-only receiver in this package:

```bash
BIN=$(swift build -c release --show-bin-path)
( printf '%s\n' '{"type":"segment","segment":{"id":1,"source":"Hello","translated":"","final":true,"timestamp":0}}'; sleep 4 ) \
  | $BIN/sherpresent-output --protocol 4 --name "Test" &
sleep 1; $BIN/syphon-probe "Test" /tmp/received.png
```

For the slideshow, start a show in PowerPoint first:

```bash
( sleep 6 ) | $BIN/sherpresent-output --protocol 4 --content slideshow --name "Test" &
sleep 3; $BIN/syphon-probe "Test" /tmp/slide.png
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
