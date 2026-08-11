#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: $0 OUTPUT_DIRECTORY" >&2
    exit 2
fi

project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
output_dir=$1
snapshot_temp_dir=$(mktemp -d)
trap 'rm -rf "$snapshot_temp_dir"' EXIT

cd "$project_root"
swift build --product kiri -Xswiftc -warnings-as-errors >/dev/null
swift_bin_path=$(swift build --show-bin-path)

swiftc \
    -parse-as-library \
    -D DEBUG \
    -warnings-as-errors \
    -target "$(uname -m)-apple-macosx14.0" \
    -I "$swift_bin_path/Modules" \
    Sources/KiriApp/AnnotationCanvasView.swift \
    Sources/KiriApp/AppModel.swift \
    Sources/KiriApp/CaptureCoordinator.swift \
    Sources/KiriApp/CaptureUIStyle.swift \
    Sources/KiriApp/EditorWindowController.swift \
    Sources/KiriApp/GIFExporter.swift \
    Sources/KiriApp/KiriDesignSystem.swift \
    Sources/KiriApp/L10n.swift \
    Sources/KiriApp/LibraryView.swift \
    Sources/KiriApp/OCRResultPanel.swift \
    Sources/KiriApp/PinnedImageController.swift \
    Sources/KiriApp/RecordingClickHighlighterController.swift \
    Sources/KiriApp/RecordingControlPanelController.swift \
    Sources/KiriApp/RecordingCountdownController.swift \
    Sources/KiriApp/RecordingOptionsPopoverController.swift \
    Sources/KiriApp/RecordingPreferences.swift \
    Sources/KiriApp/RecordingSegmentMerger.swift \
    Sources/KiriApp/RegionRecorder.swift \
    Sources/KiriApp/SelectionOverlayController.swift \
    Sources/KiriApp/TextRecognizer.swift \
    scripts/qa/LibrarySnapshotMain.swift \
    "$swift_bin_path"/KiriCore.build/*.o \
    -framework AppKit \
    -framework AVFoundation \
    -framework Carbon \
    -framework CoreMedia \
    -framework CoreVideo \
    -framework ImageIO \
    -framework ScreenCaptureKit \
    -framework SwiftUI \
    -framework Vision \
    -o "$snapshot_temp_dir/kiri-library-snapshot"

mkdir -p "$output_dir"

render_snapshot() {
    mode=$1
    width=$2
    height=$3
    fixture_root="$snapshot_temp_dir/library-$mode"
    output_path="$output_dir/library-$mode.png"
    KIRI_LIBRARY_ROOT="$fixture_root" \
        KIRI_BRAND_ICON_PATH="$project_root/Resources/Assets/kiri-icon.png" \
        "$snapshot_temp_dir/kiri-library-snapshot" \
        "$fixture_root" \
        "$output_path" \
        "$mode" \
        "$width" \
        "$height"
}

render_snapshot populated 1000 680
render_snapshot compact 760 620
render_snapshot dark 880 600
render_snapshot empty 880 600
render_snapshot loading 880 600
render_snapshot search 880 600
render_snapshot trash 880 600
render_snapshot error 880 600
render_snapshot editor 960 640

echo "$output_dir"
