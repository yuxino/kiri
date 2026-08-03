import Foundation

enum L10n {
    static func text(_ key: String, fallback: String? = nil) -> String {
        Bundle.main.localizedString(
            forKey: key,
            value: fallback ?? key,
            table: nil
        )
    }

    static func format(_ key: String, _ arguments: CVarArg...) -> String {
        String(
            format: text(key),
            locale: Locale.current,
            arguments: arguments
        )
    }
}
