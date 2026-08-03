import Foundation

public enum CaptureShortcutModifier: String, Codable, CaseIterable, Hashable, Sendable {
    case control
    case option
    case shift
    case command

    public var glyph: String {
        switch self {
        case .control: "⌃"
        case .option: "⌥"
        case .shift: "⇧"
        case .command: "⌘"
        }
    }
}

public struct CaptureShortcut: Codable, Equatable, Sendable {
    public let key: String
    public let modifiers: Set<CaptureShortcutModifier>

    public init(key: String, modifiers: Set<CaptureShortcutModifier>) {
        self.key = key
        self.modifiers = modifiers
    }

    public var displayLabel: String {
        let prefix = CaptureShortcutModifier.allCases
            .filter(modifiers.contains)
            .map(\.glyph)
            .joined()
        return prefix + key.uppercased()
    }

    public static let kiriCapture = CaptureShortcut(
        key: "a",
        modifiers: [.shift, .command]
    )
}
