#!/bin/sh
set -eu

project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
app_dir="$project_root/dist/kiri.app"
contents_dir="$app_dir/Contents"

cd "$project_root"
swift build -c release --product kiri -Xswiftc -warnings-as-errors

mkdir -p "$contents_dir/MacOS" "$contents_dir/Resources"
cp "$project_root/.build/release/kiri" "$contents_dir/MacOS/kiri"
cp "$project_root/Sources/KiriApp/Info.plist" "$contents_dir/Info.plist"
cp "$project_root/Resources/kiri.icns" "$contents_dir/Resources/kiri.icns"

codesign --force --deep --sign - "$app_dir"
echo "$app_dir"

