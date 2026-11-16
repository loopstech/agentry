#!/usr/bin/env bash
# Build the programs bundled inside Agentry.app (`agentry-hook` and the
# `agentry` CLI) and put them where Tauri expects sidecars:
#   app/src-tauri/binaries/<name>-<target triple>
#
#   scripts/build-sidecars.sh              # this machine's target
#   scripts/build-sidecars.sh universal    # Apple Silicon + Intel (releases)
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/app/src-tauri/binaries"
mkdir -p "$out"
cd "$root"

build() { cargo build --release --locked -p agentry-hook -p agentry-cli --target "$1"; }

if [ "${1:-}" = "universal" ]; then
  for t in aarch64-apple-darwin x86_64-apple-darwin; do build "$t"; done
  for bin in agentry-hook agentry; do
    lipo -create \
      "target/aarch64-apple-darwin/release/$bin" \
      "target/x86_64-apple-darwin/release/$bin" \
      -output "$out/$bin-universal-apple-darwin"
  done
  echo "Built universal sidecars in $out"
else
  triple="$(rustc -vV | sed -n 's/^host: //p')"
  build "$triple"
  for bin in agentry-hook agentry; do
    cp "target/$triple/release/$bin" "$out/$bin-$triple"
  done
  echo "Built $triple sidecars in $out"
fi
