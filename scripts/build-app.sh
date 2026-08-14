#!/usr/bin/env bash
# Builds Kiri correctly. ALWAYS use this (or `pnpm tauri build`): a plain
# `cargo build` does NOT embed the frontend assets and produces a white window.
set -euo pipefail
cd "$(dirname "$0")/.."
pnpm tauri build --no-bundle "$@"
