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

## Verify on first build

Written on a Linux machine with no macOS SDK, so **every Apple API call here is
unverified**. Compile a small spike before trusting the rest. Specifically:

- `SpeechAnalyzer.start(inputSequence:)` vs `analyzeSequence(_:)`, and
  `finalizeAndFinishThroughEndOfInput()`.
- Whether a transcriber result exposes `.isFinal` or requires comparing against
  `transcriber.volatileRange`.
- The `SpeechTranscriber.init` argument labels, and whether
  `supportedLocales` / `installedLocales` are `async`.
- `AssetInventory.assetInstallationRequest(supporting:)` returning an optional,
  whether `request.progress` is a Foundation `Progress`, and whether a locale
  must be `allocate`d (and deallocated on exit).
- The `TranslationSession(installedSource:target:)` label spelling and whether
  `prepareTranslation()` must run before the first `translate`.
- What `bestAvailableAudioFormat` actually returns — the `ready` message reports
  it precisely so this is answerable from a log paste.

## Tuning

`--turn-max-chars` (180) and `--turn-silence-ms` (1200) control where caption
lines break, and the translation debounce is 250 ms. These are flags rather than
constants because they can only really be judged by watching a live talk.
