#!/usr/bin/env bash
# Packages Kiri with Tauri. macOS builds require a stable signing identity;
# ad-hoc signing is available only when explicitly requested for disposable QA.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ "$(uname -s)" = "Darwin" ]; then
  if [ -n "${KIRI_SIGNING_IDENTITY:-}" ]; then
    export APPLE_SIGNING_IDENTITY="$KIRI_SIGNING_IDENTITY"
  elif [ "${KIRI_ALLOW_ADHOC_SIGNING:-0}" = "1" ]; then
    export APPLE_SIGNING_IDENTITY="-"
    echo "package-app: using explicitly requested ad-hoc signing" >&2
  else
    echo "package-app: no stable signing identity; set KIRI_SIGNING_IDENTITY" >&2
    echo "package-app: disposable QA may explicitly set KIRI_ALLOW_ADHOC_SIGNING=1" >&2
    exit 1
  fi
fi

pnpm tauri build "$@"
