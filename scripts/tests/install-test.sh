#!/usr/bin/env bash
# Runs scripts/install.sh against stand-ins for the macOS tools it uses
# (uname, curl, hdiutil, ditto, open, pgrep, osascript), so its logic can be
# tested on any machine. Checks: a normal install, a re-install over it, and
# that a bad checksum stops everything.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

fake="$work/bin"; release="$work/release"; mkdir -p "$fake" "$release"

# A fake release: a "dmg" (a tarball holding Agentry.app) and its checksum.
app="$work/src/Agentry.app/Contents/MacOS"
mkdir -p "$app"
printf '#!/bin/sh\necho "agentry $*" >> "%s/cli.log"\n' "$work" > "$app/agentry"
chmod +x "$app/agentry"
tar -C "$work/src" -cf "$release/Agentry.dmg" Agentry.app
(cd "$release" && sha256sum Agentry.dmg > SHA256SUMS)

cat > "$fake/uname" <<'S'
#!/bin/sh
echo Darwin
S
cat > "$fake/curl" <<S
#!/bin/sh
# curl -fsSL <url> -o <file>: serve from the fake release folder.
url=""; out=""
while [ \$# -gt 0 ]; do case "\$1" in -o) out="\$2"; shift ;; http*) url="\$1" ;; esac; shift; done
cp "$release/\${url##*/}" "\$out"
S
cat > "$fake/shasum" <<'S'
#!/bin/sh
shift 2; sha256sum "$@"
S
cat > "$fake/hdiutil" <<'S'
#!/bin/sh
# hdiutil attach <dmg> -nobrowse -quiet -mountpoint <dir> | hdiutil detach <dir>
if [ "$1" = attach ]; then
  dmg="$2"; while [ $# -gt 0 ]; do [ "$1" = -mountpoint ] && mnt="$2"; shift; done
  tar -C "$mnt" -xf "$dmg"
fi
S
cat > "$fake/ditto" <<'S'
#!/bin/sh
cp -R "$1" "$2"
S
for t in open osascript; do printf '#!/bin/sh\nexit 0\n' > "$fake/$t"; done
printf '#!/bin/sh\nexit 1\n' > "$fake/pgrep"
chmod +x "$fake"/*

run() {
  env PATH="$fake:$PATH" HOME="$work/home" AGENTRY_APP_DIR="$work/Applications" \
      AGENTRY_BIN_DIR="$work/home/.local/bin" "$@" sh "$root/scripts/install.sh"
}

echo "== install, connecting to Claude Code"
run AGENTRY_YES=1
test -x "$work/Applications/Agentry.app/Contents/MacOS/agentry"
test "$(readlink "$work/home/.local/bin/agentry")" = "$work/Applications/Agentry.app/Contents/MacOS/agentry"
grep -q "agentry hooks install" "$work/cli.log"

echo "== re-install over it, without hooks"
: > "$work/cli.log"
run AGENTRY_NO_HOOKS=1
test ! -s "$work/cli.log"

echo "== a tampered download is refused"
echo "tampered" >> "$release/Agentry.dmg"
if run AGENTRY_YES=1 2> "$work/err"; then echo "expected failure"; exit 1; fi
grep -q "checksum mismatch" "$work/err"

echo "install-test: all good"
