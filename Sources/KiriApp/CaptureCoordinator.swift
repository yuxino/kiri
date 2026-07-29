import AppKit
import CoreGraphics
@preconcurrency import ScreenCaptureKit

struct CapturedDisplay {
    let image: CGImage
    let screenFrame: CGRect
}

enum CaptureCoordinatorError: LocalizedError {
    case permissionRequired
    case displayUnavailable

    var errorDescription: String? {
        switch self {
        case .permissionRequired:
            "kiri needs Screen Recording permission. Enable it in System Settings → Privacy & Security → Screen & System Audio Recording, then try again."
        case .displayUnavailable:
            "The active display could not be captured."
        }
    }
}

@MainActor
final class CaptureCoordinator {
    func captureActiveDisplay() async throws -> CapturedDisplay {
        if !CGPreflightScreenCaptureAccess(), !CGRequestScreenCaptureAccess() {
            throw CaptureCoordinatorError.permissionRequired
        }

        let mouseLocation = NSEvent.mouseLocation
        guard let screen = NSScreen.screens.first(where: { NSMouseInRect(mouseLocation, $0.frame, false) })
            ?? NSScreen.main,
            let displayNumber = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber
        else {
            throw CaptureCoordinatorError.displayUnavailable
        }

        let displayID = CGDirectDisplayID(displayNumber.uint32Value)
        let content = try await SCShareableContent.excludingDesktopWindows(
            false,
            onScreenWindowsOnly: true
        )
        guard let display = content.displays.first(where: { $0.displayID == displayID }) else {
            throw CaptureCoordinatorError.displayUnavailable
        }

        let filter = SCContentFilter(display: display, excludingWindows: [])
        let configuration = SCStreamConfiguration()
        configuration.width = display.width
        configuration.height = display.height
        configuration.showsCursor = false
        configuration.captureResolution = .best

        let image = try await SCScreenshotManager.captureImage(
            contentFilter: filter,
            configuration: configuration
        )
        return CapturedDisplay(image: image, screenFrame: screen.frame)
    }
}
