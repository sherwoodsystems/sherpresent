# sherpresent-speech

On-device speech recognition + translation helper for the `apple` caption
provider. Free, offline, no API key.

**Requires macOS 26 (Tahoe) or later on Apple Silicon.** `SpeechAnalyzer` and
the non-SwiftUI `TranslationSession(installedSource:target:)` initializer both
landed in 26; `SFSpeechRecognizer` (the pre-26 option) caps a session at ~1
minute, which is useless for a talk.

## Build

```bash
cd apps/desktop
bun run macos:sidecar     # stages src-tauri/binaries/sherpresent-speech-aarch64-apple-darwin
bun run macos:dev         # sidecar + tauri dev
bun run macos:build       # sidecar + tauri build
```

The script no-ops on non-macOS hosts, so `cargo build` and `cargo test` stay
clean on Linux.

## Protocol

Driven by `apps/desktop/src-tauri/src/captions/provider/apple.rs`.

- **stdin**: raw little-endian i16 PCM, 16 kHz mono, unframed. EOF = graceful
  stop. The helper never opens the microphone; cpal capture stays in the Rust
  process so there is one device claim and one TCC prompt.
- **stdout**: newline-delimited JSON — `ready`, `assetProgress`, `partial`,
  `final`, `turnComplete`, `error`, `availability`.
- **stderr**: plain-text logs. Rust forwards these and appends the last few
  lines to any failure message.

`partial` and `final` carry the **cumulative** text of the current turn, not a
delta — `SpeechTranscriber` revises hypotheses ("hello word" → "hello world"),
so Rust maps both to `ProviderEvent::Replace`.

Test standalone, with no Tauri involved:

```bash
sox sample.wav -r 16000 -c 1 -e signed -b 16 -t raw - \
  | ./sherpresent-speech --protocol 1 --source en-US --target fr

./sherpresent-speech --probe --protocol 1 --source en-US --target fr
```

## Verified on hardware

Originally written on Linux with no macOS SDK. Since confirmed on macOS 26.6,
Apple Silicon, Swift 6.3 — builds warning-free, and synthetic speech (`say` →
`afconvert` → stdin) round-trips through transcription and en-US → fr
translation:

- All `SpeechAnalyzer`, `SpeechTranscriber`, `AssetInventory` and
  `TranslationSession` call sites compile as written.
- `bestAvailableAudioFormat` returns **16 kHz mono Int16**, not Float32, so
  `AVAudioConverter` is effectively a passthrough.
- `AssetInventory.assetInstallationRequest` returns a request even when the
  model is already installed, so `installedLocales` is checked first.
- No explicit locale `allocate` was needed for transcription to run.

Still worth watching during a long live session: turn breaks under real room
audio, and memory over 25+ minutes.

## Tuning

`--turn-max-chars` (180) and `--turn-silence-ms` (1200) control where caption
lines break, and the translation debounce is 250 ms. These are flags rather than
constants because they can only really be judged by watching a live talk.
