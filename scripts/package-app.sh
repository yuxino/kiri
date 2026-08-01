#!/bin/sh
set -eu

project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
app_dir=${KIRI_APP_OUTPUT:-"$project_root/dist/kiri.app"}
contents_dir="$app_dir/Contents"

available_identities=$(security find-identity -v -p codesigning 2>/dev/null || true)

first_identity_with_prefix() {
    printf '%s\n' "$available_identities" | awk -F'"' -v prefix="$1" '
        index($2, prefix) == 1 { print $2; exit }
    '
}

signing_identity=${KIRI_CODESIGN_IDENTITY:-}
if [ -z "$signing_identity" ]; then
    signing_identity=$(first_identity_with_prefix "Apple Development:")
fi
if [ -z "$signing_identity" ]; then
    signing_identity=$(first_identity_with_prefix "Developer ID Application:")
fi
if [ -z "$signing_identity" ]; then
    signing_identity=$(first_identity_with_prefix "mimi Local Development")
fi

if [ -z "$signing_identity" ]; then
    if [ "${KIRI_ALLOW_ADHOC_SIGNING:-0}" = "1" ]; then
        signing_identity=-
    else
        echo "No stable code-signing identity is available." >&2
        echo "Install an Apple Development certificate, set KIRI_CODESIGN_IDENTITY, or explicitly opt into permission-breaking ad-hoc signing with KIRI_ALLOW_ADHOC_SIGNING=1." >&2
        exit 1
    fi
fi

if [ "$signing_identity" = "-" ] && [ "${KIRI_ALLOW_ADHOC_SIGNING:-0}" != "1" ]; then
    echo "Ad-hoc signing changes kiri's privacy identity between builds." >&2
    echo "Set KIRI_ALLOW_ADHOC_SIGNING=1 only when persistent Screen Recording permission is not required." >&2
    exit 1
fi

cd "$project_root"
swift build -c release --product kiri -Xswiftc -warnings-as-errors

mkdir -p "$contents_dir/MacOS" "$contents_dir/Resources"
cp "$project_root/.build/release/kiri" "$contents_dir/MacOS/kiri"
cp "$project_root/Sources/KiriApp/Info.plist" "$contents_dir/Info.plist"
cp "$project_root/Resources/kiri.icns" "$contents_dir/Resources/kiri.icns"

echo "Signing kiri with: $signing_identity"
codesign --force --deep --options runtime --sign "$signing_identity" "$app_dir"
echo "$app_dir"
