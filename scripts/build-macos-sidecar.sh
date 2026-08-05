#!/usr/bin/env bash
# Build the Swift speech sidecar and stage it where Tauri's externalBin expects.
#
# Deliberately NOT wired into build.rs: day-to-day development happens on Linux,
# and a Swift step in the Cargo graph would break `cargo build`/`cargo test`
# there. The uname guard below is what keeps non-macOS hosts clean.
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Not macOS — skipping the Swift speech sidecar build."
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

PKG="$ROOT/apps/desktop/sidecars/speech-macos"
OUT="$ROOT/apps/desktop/src-tauri/binaries"

# Apple Silicon only: the on-device Speech and Translation models effectively
# require it, and apple.rs preflights the architecture with a clear message.
TRIPLE="aarch64-apple-darwin"

if ! command -v swift >/dev/null 2>&1; then
  echo "error: 'swift' not found. Install Xcode or the Command Line Tools (macOS 26 SDK or later)." >&2
  exit 1
fi

echo "Building sherpresent-speech (release, arm64)…"
swift build --package-path "$PKG" -c release --arch arm64

BIN_DIR="$(swift build --package-path "$PKG" -c release --arch arm64 --show-bin-path)"
BIN="$BIN_DIR/sherpresent-speech"

if [[ ! -f "$BIN" ]]; then
  echo "error: expected binary not found at $BIN" >&2
  exit 1
fi

mkdir -p "$OUT"
cp "$BIN" "$OUT/sherpresent-speech-$TRIPLE"
chmod +x "$OUT/sherpresent-speech-$TRIPLE"

echo "Sidecar staged at $OUT/sherpresent-speech-$TRIPLE"
