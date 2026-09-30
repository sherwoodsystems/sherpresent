#!/usr/bin/env bash
# Every test, lint and format check in the repo. Run before committing.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"

echo "== Rust"
(cd "$root" && cargo fmt --all --check \
  && cargo clippy --workspace --all-targets -- -D warnings \
  && cargo test --workspace)

for app in desktop bridge; do
  echo "== $app frontend"
  (cd "$root/apps/$app" && bun run lint && bun run check && bun run test)
done

if [[ "$(uname)" == "Darwin" ]]; then
  echo "== Swift helpers"
  (cd "$root/apps/desktop" && swift format lint --strict -r \
    sidecars/caption-output-macos/Sources/sherpresent-output sidecars/caption-output-macos/Sources/syphon-probe \
    sidecars/caption-output-macos/Tests sidecars/speech-macos/Sources sidecars/speech-macos/Tests)
  "$root/scripts/swift-test.sh"
fi

echo "All checks passed."
