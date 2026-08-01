public enum ScreenCapturePermissionOutcome: Equatable, Sendable {
    case authorized
    case restartRequired
    case settingsRequired
}

public struct ScreenCapturePermissionGate: Sendable {
    private var cachedMissingPermissionOutcome: ScreenCapturePermissionOutcome?

    public init() {}

    public mutating func check(
        preflight: () -> Bool,
        request: () -> Bool
    ) -> ScreenCapturePermissionOutcome {
        if preflight() {
            cachedMissingPermissionOutcome = nil
            return .authorized
        }

        if let cachedMissingPermissionOutcome {
            return cachedMissingPermissionOutcome
        }

        let outcome: ScreenCapturePermissionOutcome = request()
            ? .restartRequired
            : .settingsRequired
        cachedMissingPermissionOutcome = outcome
        return outcome
    }
}
