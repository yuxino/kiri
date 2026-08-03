#!/bin/sh
set -eu

project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
built_app="$project_root/dist/Kiri.app"
installed_app="/Applications/Kiri.app"
staged_app="/Applications/.Kiri.installing.app"

"$project_root/scripts/package-app.sh"

if [ ! -w /Applications ]; then
    echo "Kiri needs permission to install in /Applications." >&2
    exit 1
fi

kiri_process_pattern='/[Kk]iri[^/]*\.app/Contents/MacOS/kiri$'
kiri_pids=$(pgrep -f "$kiri_process_pattern" 2>/dev/null || true)
if [ -n "$kiri_pids" ]; then
    kill -TERM $kiri_pids 2>/dev/null || true
    attempts=0
    while pgrep -f "$kiri_process_pattern" >/dev/null 2>&1 && [ "$attempts" -lt 20 ]; do
        sleep 0.1
        attempts=$((attempts + 1))
    done
    remaining_pids=$(pgrep -f "$kiri_process_pattern" 2>/dev/null || true)
    if [ -n "$remaining_pids" ]; then
        kill -KILL $remaining_pids 2>/dev/null || true
    fi
fi

rm -rf -- "$staged_app"
ditto "$built_app" "$staged_app"
codesign --verify --deep --strict "$staged_app"

if [ -e "$installed_app" ]; then
    rm -rf -- "$installed_app"
fi
mv "$staged_app" "$installed_app"
codesign --verify --deep --strict "$installed_app"

echo "$installed_app"
