#!/bin/sh
# Install Agentry on macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/loopstech/Agentry/main/scripts/install.sh | sh
#
# Downloads the latest release, checks its SHA-256, copies Agentry.app to
# /Applications (or ~/Applications), links the `agentry` command, and offers
# to connect Agentry to Claude Code. Re-run it to update.
#
# Options (environment variables):
#   AGENTRY_VERSION=v0.2.0   install a specific release instead of the latest
#   AGENTRY_YES=1            don't ask; connect to Claude Code automatically
#   AGENTRY_NO_HOOKS=1       don't connect to Claude Code
#   AGENTRY_APP_DIR=…        where to put Agentry.app (default /Applications, else ~/Applications)
#   AGENTRY_BIN_DIR=…        where to link `agentry` (default /usr/local/bin, else ~/.local/bin)
set -eu

REPO="loopstech/Agentry"
APP_NAME="Agentry.app"

say() { printf '%s\n' "$*"; }
fail() { printf 'agentry install: %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || fail "Agentry is a macOS app for now."
for tool in curl shasum hdiutil ditto; do
  command -v "$tool" >/dev/null 2>&1 || fail "missing $tool"
done

if [ -n "${AGENTRY_VERSION:-}" ]; then
  base="https://github.com/$REPO/releases/download/$AGENTRY_VERSION"
else
  base="https://github.com/$REPO/releases/latest/download"
fi

tmp="$(mktemp -d)"
mount=""
cleanup() {
  if [ -n "$mount" ]; then hdiutil detach "$mount" -quiet >/dev/null 2>&1 || true; fi
  rm -rf "$tmp"
}
trap cleanup EXIT INT TERM

say "Downloading Agentry…"
curl -fsSL "$base/Agentry.dmg" -o "$tmp/Agentry.dmg" || fail "download failed ($base/Agentry.dmg)"
curl -fsSL "$base/SHA256SUMS" -o "$tmp/SHA256SUMS" || fail "couldn't download checksums"

expected="$(awk '$2 ~ /Agentry\.dmg$/ { print $1 }' "$tmp/SHA256SUMS")"
actual="$(shasum -a 256 "$tmp/Agentry.dmg" | awk '{ print $1 }')"
[ -n "$expected" ] || fail "no checksum for Agentry.dmg in SHA256SUMS"
[ "$expected" = "$actual" ] || fail "checksum mismatch: expected $expected, got $actual"
say "Checksum OK."

mount="$tmp/mnt"
mkdir -p "$mount"
hdiutil attach "$tmp/Agentry.dmg" -nobrowse -quiet -mountpoint "$mount" || fail "couldn't open the disk image"
[ -d "$mount/$APP_NAME" ] || fail "$APP_NAME not found in the disk image"

# Quit a running copy so it can be replaced.
if pgrep -x agentry-app >/dev/null 2>&1; then
  say "Quitting the running Agentry…"
  osascript -e 'tell application id "io.loopstech.agentry" to quit' >/dev/null 2>&1 || pkill -x agentry-app || true
  sleep 1
fi

if [ -n "${AGENTRY_APP_DIR:-}" ]; then dest="$AGENTRY_APP_DIR"
elif [ -w /Applications ]; then dest="/Applications"
else dest="$HOME/Applications"; fi
mkdir -p "$dest"
say "Installing to $dest/$APP_NAME…"
rm -rf "${dest:?}/$APP_NAME"
ditto "$mount/$APP_NAME" "$dest/$APP_NAME"
hdiutil detach "$mount" -quiet >/dev/null 2>&1 || true
mount=""

cli="$dest/$APP_NAME/Contents/MacOS/agentry"
[ -x "$cli" ] || fail "the agentry command is missing from the app"

# Link the command: /usr/local/bin if writable, else ~/.local/bin.
if [ -n "${AGENTRY_BIN_DIR:-}" ]; then bindir="$AGENTRY_BIN_DIR"
elif [ -w /usr/local/bin ]; then bindir="/usr/local/bin"
else bindir="$HOME/.local/bin"; fi
mkdir -p "$bindir"
link="$bindir/agentry"
if [ -e "$link" ] && [ ! -L "$link" ]; then
  say "Leaving $link alone (it isn't a link Agentry made)."
else
  ln -sf "$cli" "$link"
  say "Linked $link"
  case ":$PATH:" in
    *":$bindir:"*) ;;
    *) say "Add $bindir to your PATH to use the agentry command." ;;
  esac
fi

# Connect to Claude Code, with consent.
connect="n"
if [ -n "${AGENTRY_NO_HOOKS:-}" ]; then
  connect="n"
elif [ -n "${AGENTRY_YES:-}" ]; then
  connect="y"
elif [ -r /dev/tty ]; then
  printf 'Connect Agentry to Claude Code now? This adds hooks to ~/.claude/settings.json (backed up first). [Y/n] '
  read -r answer </dev/tty || answer=""
  case "$answer" in [nN]*) connect="n" ;; *) connect="y" ;; esac
fi
if [ "$connect" = "y" ]; then
  "$cli" hooks install
else
  say "Skipped. Connect later from Agentry's Settings, or run: agentry hooks install"
fi

open "$dest/$APP_NAME"
say "Agentry is installed and running. Look for the pill under your menu bar."
