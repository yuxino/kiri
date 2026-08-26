#!/usr/bin/env bash
set -euo pipefail

# Build and update one canonical Kiri installation without silently changing
# the macOS code identity that owns its privacy grants.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd -P)"
BUILD_APP="$PROJECT_DIR/src-tauri/target/release/bundle/macos/kiri.app"
INSTALL_APP="${KIRI_INSTALL_PATH:-/Applications/Kiri.app}"
EXPECTED_IDENTIFIER="io.yuxino.kiri"

case "$INSTALL_APP" in
  /*/*.app) ;;
  *) echo "install-app: KIRI_INSTALL_PATH must be an absolute .app path" >&2; exit 2 ;;
esac
[[ ! -L "$INSTALL_APP" ]] || {
  echo "install-app: the canonical app path cannot be a symbolic link" >&2
  exit 1
}

requested_parent="$(dirname "$INSTALL_APP")"
install_name="$(basename "$INSTALL_APP")"
[[ -d "$requested_parent" && -w "$requested_parent" ]] || {
  echo "install-app: install directory is not writable: $requested_parent" >&2
  exit 1
}
install_parent="$(cd "$requested_parent" && pwd -P)"
INSTALL_APP="$install_parent/$install_name"

bundle_identifier() {
  /usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' \
    "$1/Contents/Info.plist" 2>/dev/null
}

designated_requirement() {
  /usr/bin/codesign --display --requirements - "$1" 2>&1 \
    | /usr/bin/sed -n 's/^#*[[:space:]]*designated => //p'
}

verify_kiri_app() {
  local app="$1"
  local requirement
  [[ -d "$app" && ! -L "$app" ]] || return 1
  /usr/bin/codesign --verify --deep --strict "$app" || return 1
  [[ "$(bundle_identifier "$app")" == "$EXPECTED_IDENTIFIER" ]] || return 1
  requirement="$(designated_requirement "$app")"
  [[ -n "$requirement" && "$requirement" != cdhash\ * ]]
}

cd "$PROJECT_DIR"
"$SCRIPT_DIR/package-app.sh" --bundles app
verify_kiri_app "$BUILD_APP" || {
  echo "install-app: packaged Kiri is unsigned, ad-hoc, or otherwise invalid" >&2
  exit 1
}
new_requirement="$(designated_requirement "$BUILD_APP")"

if [[ -e "$INSTALL_APP" && ! -d "$INSTALL_APP" ]]; then
  echo "install-app: install path exists but is not an app bundle: $INSTALL_APP" >&2
  exit 1
fi
if [[ -d "$INSTALL_APP" ]]; then
  existing_requirement="$(designated_requirement "$INSTALL_APP" || true)"
  if ! verify_kiri_app "$INSTALL_APP" || [[ "$existing_requirement" != "$new_requirement" ]]; then
    if [[ "${KIRI_ALLOW_IDENTITY_CHANGE:-0}" != "1" ]]; then
      cat >&2 <<EOF
install-app: refusing to replace Kiri with a different macOS code identity.

Installed: ${existing_requirement:-<invalid or unsigned>}
New:       $new_requirement

A deliberate certificate migration requires one run with
KIRI_ALLOW_IDENTITY_CHANGE=1 and may require one final macOS authorization.
EOF
      exit 1
    fi
    echo "install-app: warning: performing an explicit one-time identity migration" >&2
  fi
fi

running_pids="$(/usr/bin/pgrep -U "$(/usr/bin/id -u)" -x kiri || true)"
if [[ -n "$running_pids" ]]; then
  echo "install-app: quit every running Kiri copy before installing (PID ${running_pids//$'\n'/, })" >&2
  exit 1
fi

staging_dir="$(/usr/bin/mktemp -d "$install_parent/.kiri-install.XXXXXX")"
case "$staging_dir" in
  "$install_parent"/.kiri-install.*) ;;
  *) echo "install-app: unexpected staging path: $staging_dir" >&2; exit 1 ;;
esac
staged_app="$staging_dir/new.app"
previous_app="$staging_dir/previous.app"
install_committed=0

cleanup() {
  if [[ -d "${previous_app:-}" && ! -e "$INSTALL_APP" ]]; then
    /bin/mv "$previous_app" "$INSTALL_APP" || true
  fi
  if [[ "${install_committed:-0}" == "1" ]]; then
    /bin/rm -rf "$staging_dir"
  elif [[ -d "${staging_dir:-}" ]]; then
    echo "install-app: interrupted install preserved at $staging_dir" >&2
  fi
}
trap cleanup EXIT

/usr/bin/ditto "$BUILD_APP" "$staged_app"
verify_kiri_app "$staged_app" || {
  echo "install-app: staged app failed signature verification" >&2
  exit 1
}
[[ "$(designated_requirement "$staged_app")" == "$new_requirement" ]] || {
  echo "install-app: staged app identity changed while copying" >&2
  exit 1
}

if [[ -d "$INSTALL_APP" ]]; then
  /bin/mv "$INSTALL_APP" "$previous_app"
fi
if ! /bin/mv "$staged_app" "$INSTALL_APP"; then
  [[ ! -d "$previous_app" ]] || /bin/mv "$previous_app" "$INSTALL_APP"
  echo "install-app: installation failed" >&2
  exit 1
fi

if ! verify_kiri_app "$INSTALL_APP" \
  || [[ "$(designated_requirement "$INSTALL_APP")" != "$new_requirement" ]]; then
  /bin/rm -rf "$INSTALL_APP"
  [[ ! -d "$previous_app" ]] || /bin/mv "$previous_app" "$INSTALL_APP"
  echo "install-app: final identity verification failed" >&2
  exit 1
fi

install_committed=1
/bin/rm -rf "$previous_app"
echo "installed stable Kiri app: $INSTALL_APP"
echo "designated requirement: $new_requirement"
