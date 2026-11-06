#!/usr/bin/env bash
# Build the Swift sidecars (speech recognition, caption video output) and stage
# them where Tauri's externalBin expects.
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

OUT="$ROOT/apps/desktop/src-tauri/binaries"

# Apple Silicon only: the on-device Speech and Translation models effectively
# require it, and apple.rs / output/syphon.rs preflight the architecture with a
# clear message.
TRIPLE="aarch64-apple-darwin"

if ! command -v swift >/dev/null 2>&1; then
  echo "error: 'swift' not found. Install Xcode or the Command Line Tools (macOS 26 SDK or later)." >&2
  exit 1
fi

mkdir -p "$OUT"

# build <package dir> <product>
build() {
  local pkg="$ROOT/apps/desktop/sidecars/$1" product="$2"
  echo "Building $product (release, arm64)…"
  swift build --package-path "$pkg" -c release --arch arm64 --product "$product"

  local bin_dir
  bin_dir="$(swift build --package-path "$pkg" -c release --arch arm64 --show-bin-path)"
  if [[ ! -f "$bin_dir/$product" ]]; then
    echo "error: expected binary not found at $bin_dir/$product" >&2
    exit 1
  fi

  cp "$bin_dir/$product" "$OUT/$product-$TRIPLE"
  chmod +x "$OUT/$product-$TRIPLE"
  echo "Staged $OUT/$product-$TRIPLE"
}

build speech-macos sherpresent-speech
build caption-output-macos sherpresent-output
