#!/usr/bin/env bash
# Packages Kiri and installs it to /Applications (macOS).
#
# Uses the stable local signing identity by default: ad-hoc-signed builds
# get a NEW designated requirement (cdhash) on every build, so macOS TCC
# forgets the Screen Recording grant and re-prompts after every reinstall.
# A certificate-signed build keeps the same requirement (bundle id +
# certificate root) and the grant persists across reinstalls.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -z "${KIRI_SIGNING_IDENTITY:-}" ] && security find-identity -v -p codesigning 2>/dev/null | grep -q "mimi Local Development"; then
  export KIRI_SIGNING_IDENTITY="mimi Local Development"
fi

./scripts/package-app.sh --bundles app

APP="src-tauri/target/release/bundle/macos/kiri.app"
if [ ! -d "$APP" ]; then
  echo "bundle not found at $APP" >&2
  exit 1
fi

if [ -d "/Applications/Kiri.app" ]; then
  rm -rf "/Applications/Kiri.app"
fi
ditto "$APP" "/Applications/Kiri.app"
echo "installed /Applications/Kiri.app"
