#!/usr/bin/env bash
# Packages Kiri and installs it to /Applications (macOS).
set -euo pipefail
cd "$(dirname "$0")/.."

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
