@testable import KiriCore

func screenCapturePermissionSkipsRequestWhenAuthorized() throws {
    var gate = ScreenCapturePermissionGate()
    var requestCount = 0

    let outcome = gate.check(
        preflight: { true },
        request: {
            requestCount += 1
            return false
        }
    )

    try expect(outcome == .authorized, "Authorized access should continue")
    try expect(requestCount == 0, "Authorized access should not request again")
}

func screenCapturePermissionCachesGrantedRequest() throws {
    var gate = ScreenCapturePermissionGate()
    var requestCount = 0

    let first = gate.check(
        preflight: { false },
        request: {
            requestCount += 1
            return true
        }
    )
    let second = gate.check(
        preflight: { false },
        request: {
            requestCount += 1
            return true
        }
    )

    try expect(first == .restartRequired, "A granted request should require restart")
    try expect(second == .restartRequired, "The granted outcome should be cached")
    try expect(requestCount == 1, "The system request should run once per launch")
}

func screenCapturePermissionCachesDeclinedRequest() throws {
    var gate = ScreenCapturePermissionGate()
    var requestCount = 0

    let first = gate.check(
        preflight: { false },
        request: {
            requestCount += 1
            return false
        }
    )
    let second = gate.check(
        preflight: { false },
        request: {
            requestCount += 1
            return false
        }
    )

    try expect(first == .settingsRequired, "A declined request should point to settings")
    try expect(second == .settingsRequired, "The declined outcome should be cached")
    try expect(requestCount == 1, "A declined request should not be repeated")
}

func screenCapturePermissionPreflightOverridesCache() throws {
    var gate = ScreenCapturePermissionGate()
    var requestCount = 0

    _ = gate.check(
        preflight: { false },
        request: {
            requestCount += 1
            return false
        }
    )
    let outcome = gate.check(preflight: { true }, request: { false })
    let revokedOutcome = gate.check(
        preflight: { false },
        request: {
            requestCount += 1
            return true
        }
    )

    try expect(outcome == .authorized, "A later successful preflight should win")
    try expect(
        revokedOutcome == .restartRequired,
        "A successful preflight should clear the old missing-permission outcome"
    )
    try expect(requestCount == 2, "A later revocation should allow one fresh request")
}
