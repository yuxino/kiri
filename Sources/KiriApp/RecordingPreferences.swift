import Foundation
import KiriCore

enum RecordingPreferences {
    private static let optionsKey = "recording.options.v1"

    static func load() -> RecordingOptions {
        guard let data = UserDefaults.standard.data(forKey: optionsKey),
              let options = try? JSONDecoder().decode(RecordingOptions.self, from: data) else {
            return RecordingOptions()
        }
        return options.normalized
    }

    static func save(_ options: RecordingOptions) {
        guard let data = try? JSONEncoder().encode(options.normalized) else { return }
        UserDefaults.standard.set(data, forKey: optionsKey)
    }
}
