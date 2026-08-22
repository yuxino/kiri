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

available_identity_names() {
  /usr/bin/security find-identity -v -p codesigning 2>/dev/null \
    | /usr/bin/sed -n 's/^[^"]*"\([^"]*\)".*$/\1/p'
}

automatic_identity() {
  local prefix candidate
  for prefix in "Apple Development:" "Developer ID Application:"; do
    while IFS= read -r candidate; do
      case "$candidate" in
        "$prefix"*) printf '%s\n' "$candidate"; return 0 ;;
      esac
    done < <(available_identity_names)
  done
  while IFS= read -r candidate; do
    case "$candidate" in
      *" Local Development") printf '%s\n' "$candidate"; return 0 ;;
    esac
  done < <(available_identity_names)
  return 1
}

if [ -z "${KIRI_SIGNING_IDENTITY:-}" ]; then
  KIRI_SIGNING_IDENTITY="$(automatic_identity || true)"
fi
if [ -z "$KIRI_SIGNING_IDENTITY" ]; then
  echo "install-app: no stable code-signing identity found; set KIRI_SIGNING_IDENTITY" >&2
  exit 1
fi
export KIRI_SIGNING_IDENTITY

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
