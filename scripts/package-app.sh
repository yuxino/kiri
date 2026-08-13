#!/usr/bin/env bash
# Packages Kiri with tauri. Bundles ffmpeg and uses the stable signing
# identity from KIRI_SIGNING_IDENTITY when provided (never silent ad-hoc).
set -euo pipefail
cd "$(dirname "$0")/.."

node scripts/ensure-ffmpeg.mjs

# Bundle the platform ffmpeg binary into the app resources.
TRIPLE="$(rustc -vV | sed -n 's/host: //p')"
FFMPEG_SRC="src-tauri/binaries/ffmpeg-$TRIPLE/ffmpeg"
if [ ! -f "$FFMPEG_SRC" ]; then
  echo "missing $FFMPEG_SRC — run: node scripts/ensure-ffmpeg.mjs" >&2
  exit 1
fi

if [ -n "${KIRI_SIGNING_IDENTITY:-}" ]; then
  export APPLE_SIGNING_IDENTITY="$KIRI_SIGNING_IDENTITY"
fi

pnpm tauri build "$@"
