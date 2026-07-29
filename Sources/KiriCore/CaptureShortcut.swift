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
}

public enum CaptureShortcutPreset: String, Codable, CaseIterable, Identifiable, Sendable {
    case shiftCommandA
    case optionCommand2
    case controlShift2

    public var id: String { rawValue }

    public init?(storedIdentifier: String) {
        if storedIdentifier == "shiftCommand2" {
            self = .shiftCommandA
        } else {
            self.init(rawValue: storedIdentifier)
        }
    }

    public var shortcut: CaptureShortcut {
        switch self {
        case .shiftCommandA:
            CaptureShortcut(key: "A", modifiers: [.shift, .command])
        case .optionCommand2:
            CaptureShortcut(key: "2", modifiers: [.option, .command])
        case .controlShift2:
            CaptureShortcut(key: "2", modifiers: [.control, .shift])
        }
    }
}
