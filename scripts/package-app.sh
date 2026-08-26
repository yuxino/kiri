#!/usr/bin/env bash
# Packages Kiri with Tauri. macOS builds require a stable signing identity.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
cd "$SCRIPT_DIR/.."

if [ "$(uname -s)" = "Darwin" ]; then
  APPLE_SIGNING_IDENTITY="$(
    "$SCRIPT_DIR/codesign-identity.sh" "${KIRI_SIGNING_IDENTITY:-}"
  )"
  export APPLE_SIGNING_IDENTITY
fi

pnpm tauri build "$@"
