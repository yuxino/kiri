import Foundation
@testable import KiriCore

func captureShortcutPresetsHaveStableLabels() throws {
    try expect(
        CaptureShortcutPreset.shiftCommand2.shortcut.displayLabel == "⇧⌘2",
        "Shift-command preset should use the expected macOS label"
    )
    try expect(
        CaptureShortcutPreset.optionCommand2.shortcut.displayLabel == "⌥⌘2",
        "Option-command preset should use the expected macOS label"
    )
    try expect(
        CaptureShortcutPreset.controlShift2.shortcut.displayLabel == "⌃⇧2",
        "Control-shift preset should use the expected macOS label"
    )
}

func captureShortcutRoundTrips() throws {
    for preset in CaptureShortcutPreset.allCases {
        let data = try JSONEncoder().encode(preset)
        let decoded = try JSONDecoder().decode(CaptureShortcutPreset.self, from: data)
        try expect(decoded == preset, "\(preset.rawValue) should survive JSON round-trip")
    }
}

func captureShortcutExposesNormalizedModifiers() throws {
    let shortcut = CaptureShortcutPreset.shiftCommand2.shortcut
    try expect(shortcut.key == "2", "Capture shortcut should expose its key")
    try expect(
        shortcut.modifiers == [.shift, .command],
        "Capture shortcut should expose a normalized modifier set"
    )
}

