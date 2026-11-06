#!/usr/bin/env bash
# Run `swift test` for the macOS helper packages.
#
# With only the Command Line Tools installed (no Xcode), SwiftPM finds neither
# Swift Testing nor its interop library on its own; point it at them. With
# Xcode selected this adds nothing.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
flags=()
dev="$(xcode-select -p)/Library/Developer"
if [[ -d "$dev/Frameworks/Testing.framework" ]]; then
  flags=(
    -Xswiftc -F -Xswiftc "$dev/Frameworks"
    -Xlinker -F -Xlinker "$dev/Frameworks"
    -Xlinker -rpath -Xlinker "$dev/Frameworks"
    -Xlinker -rpath -Xlinker "$dev/usr/lib"
  )
fi

for pkg in caption-output-macos speech-macos; do
  echo "== $pkg"
  swift test --package-path "$root/apps/desktop/sidecars/$pkg" "${flags[@]}"
done
