import Foundation
@testable import KiriCore

func captureShortcutHasStableLabel() throws {
    try expect(
        CaptureShortcut.kiriCapture.displayLabel == "⇧⌘A",
        "Kiri should expose the fixed shift-command-A label"
    )
}

func captureShortcutRoundTrips() throws {
    let data = try JSONEncoder().encode(CaptureShortcut.kiriCapture)
    let decoded = try JSONDecoder().decode(CaptureShortcut.self, from: data)
    try expect(decoded == .kiriCapture, "The fixed capture shortcut should round-trip")
}

func captureShortcutExposesNormalizedModifiers() throws {
    let shortcut = CaptureShortcut.kiriCapture
    try expect(shortcut.key == "a", "Capture shortcut should expose the A key")
    try expect(
        shortcut.modifiers == [.shift, .command],
        "Capture shortcut should expose a normalized modifier set"
    )
}
