#!/usr/bin/env bash
# Builds and verifies the single Universal macOS release DMG.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd -P)"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "package-macos-release: macOS is required" >&2
  exit 1
fi
if [[ "$#" -ne 0 ]]; then
  echo "usage: ./scripts/package-macos-release.sh" >&2
  exit 2
fi

installed_targets="$(rustup target list --installed)"
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  if ! grep -qx "$target" <<<"$installed_targets"; then
    echo "package-macos-release: missing Rust target $target" >&2
    exit 1
  fi
done

cd "$PROJECT_DIR"
"$SCRIPT_DIR/package-app.sh" --target universal-apple-darwin --bundles dmg

config_value() {
  node -e '
    const fs = require("node:fs");
    const config = JSON.parse(fs.readFileSync("src-tauri/tauri.conf.json", "utf8"));
    const value = process.argv[1].split(".").reduce((current, key) => current[key], config);
    process.stdout.write(String(value));
  ' "$1"
}

version="$(config_value version)"
expected_identifier="$(config_value identifier)"
expected_macos="$(config_value bundle.macOS.minimumSystemVersion)"
dmg="$PROJECT_DIR/src-tauri/target/universal-apple-darwin/release/bundle/dmg/kiri_${version}_universal.dmg"

[[ -f "$dmg" ]] || {
  echo "package-macos-release: expected DMG not found: $dmg" >&2
  exit 1
}

hdiutil verify "$dmg" >/dev/null
codesign --verify --verbose=2 "$dmg"

mount_dir="$(mktemp -d "${TMPDIR:-/tmp}/kiri-release-verify.XXXXXX")"
mounted_device=""
cleanup() {
  if [[ -n "$mounted_device" ]]; then
    hdiutil detach "$mounted_device" >/dev/null || true
  fi
  rmdir "$mount_dir" 2>/dev/null || true
}
trap cleanup EXIT

attach_output="$(hdiutil attach -readonly -nobrowse -mountpoint "$mount_dir" "$dmg")"
mounted_device="$(
  awk '$1 ~ /^\/dev\/disk[0-9]+s[0-9]+$/ { print $1; exit }' <<<"$attach_output"
)"
[[ -n "$mounted_device" ]] || {
  echo "package-macos-release: could not identify the mounted DMG device" >&2
  exit 1
}

app="$mount_dir/kiri.app"
executable="$app/Contents/MacOS/kiri"
[[ -d "$app" && -x "$executable" ]] || {
  echo "package-macos-release: DMG does not contain kiri.app" >&2
  exit 1
}
[[ -L "$mount_dir/Applications" && "$(readlink "$mount_dir/Applications")" == "/Applications" ]] || {
  echo "package-macos-release: DMG is missing its Applications link" >&2
  exit 1
}

architectures="$(lipo -archs "$executable")"
[[ "$(wc -w <<<"$architectures" | tr -d ' ')" == "2" ]] || {
  echo "package-macos-release: expected exactly two architectures, found: $architectures" >&2
  exit 1
}
for architecture in arm64 x86_64; do
  [[ " $architectures " == *" $architecture "* ]] || {
    echo "package-macos-release: missing $architecture slice" >&2
    exit 1
  }
  minimum="$(
    otool -arch "$architecture" -l "$executable" |
      awk '$1 == "cmd" && $2 == "LC_BUILD_VERSION" { build = 1; next }
           build && $1 == "minos" { print $2; exit }'
  )"
  [[ "$minimum" == "$expected_macos" ]] || {
    echo "package-macos-release: $architecture requires macOS $minimum, expected $expected_macos" >&2
    exit 1
  }
done

[[ "$(plutil -extract CFBundleIdentifier raw "$app/Contents/Info.plist")" == "$expected_identifier" ]]
[[ "$(plutil -extract CFBundleShortVersionString raw "$app/Contents/Info.plist")" == "$version" ]]
[[ "$(plutil -extract LSMinimumSystemVersion raw "$app/Contents/Info.plist")" == "$expected_macos" ]]
codesign --verify --deep --strict --verbose=2 "$app"

requirement="$(
  codesign --display --requirements - "$app" 2>&1 |
    sed -n 's/^#*[[:space:]]*designated => //p'
)"
[[ -n "$requirement" && "$requirement" != cdhash\ * ]] || {
  echo "package-macos-release: app has an ad-hoc or unusable designated requirement" >&2
  exit 1
}

echo "verified Universal macOS release: $dmg"
echo "architectures: $architectures; minimum macOS: $expected_macos"
