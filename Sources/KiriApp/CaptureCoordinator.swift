import AppKit
import CoreGraphics
import ImageIO
import KiriCore
@preconcurrency import ScreenCaptureKit

struct CapturedDisplay {
    let image: CGImage
    let screenFrame: CGRect
    let windowRectsFrontToBack: [CGRect]
    let displayID: CGDirectDisplayID
    let backingScale: CGFloat
}

enum CaptureCoordinatorError: LocalizedError {
    case permissionRestartRequired
    case permissionSettingsRequired
    case displayUnavailable

    var errorDescription: String? {
        switch self {
        case .permissionRestartRequired:
            L10n.text("Screen Recording access was granted. Quit and reopen Kiri once to finish enabling capture.")
        case .permissionSettingsRequired:
            L10n.text("Screen Recording is off. Enable Kiri in System Settings, then quit and reopen it once.")
        case .displayUnavailable:
            L10n.text("The active display could not be captured.")
        }
    }
}

@MainActor
protocol DisplayCapturing: AnyObject {
    func captureActiveDisplay(
        excludingWindowIDs: Set<CGWindowID>
    ) async throws -> CapturedDisplay
}

@MainActor
final class CaptureCoordinator: DisplayCapturing {
    private var permissionGate = ScreenCapturePermissionGate()

    func captureActiveDisplay(
        excludingWindowIDs: Set<CGWindowID> = []
    ) async throws -> CapturedDisplay {
#if DEBUG
        let usesFixture = ProcessInfo.processInfo.environment["KIRI_CAPTURE_FIXTURE"] == "1"
            || CommandLine.arguments.contains("--capture-fixture")
        if usesFixture,
           let fixture = Self.makeFixture() {
            return fixture
        }
#endif

        switch permissionGate.check(
            preflight: CGPreflightScreenCaptureAccess,
            request: CGRequestScreenCaptureAccess
        ) {
        case .authorized:
            break
        case .restartRequired:
            throw CaptureCoordinatorError.permissionRestartRequired
        case .settingsRequired:
            throw CaptureCoordinatorError.permissionSettingsRequired
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

        let displayBounds = CGDisplayBounds(displayID)
        let currentProcessID = ProcessInfo.processInfo.processIdentifier
        let windowRects = content.windows.compactMap { window -> CGRect? in
            guard window.isOnScreen,
                  window.windowLayer == 0,
                  window.owningApplication?.processID != currentProcessID else {
                return nil
            }
            let visible = window.frame.standardized.intersection(displayBounds)
            guard !visible.isNull, visible.width >= 8, visible.height >= 8 else {
                return nil
            }
            return CGRect(
                x: visible.minX - displayBounds.minX,
                y: visible.minY - displayBounds.minY,
                width: visible.width,
                height: visible.height
            )
        }

        let filter = Self.contentFilter(
            display: display,
            content: content,
            excludingWindowIDs: excludingWindowIDs
        )
        let configuration = SCStreamConfiguration()
        // SCDisplay dimensions are measured in points, while the stream output
        // dimensions are pixels. Capture at the screen's backing resolution so
        // the full-screen selection overlay stays sharp on Retina displays.
        let backingScale = max(screen.backingScaleFactor, 1)
        configuration.width = max(
            1,
            Int((CGFloat(display.width) * backingScale).rounded())
        )
        configuration.height = max(
            1,
            Int((CGFloat(display.height) * backingScale).rounded())
        )
        configuration.showsCursor = false

        let image = try await SCScreenshotManager.captureImage(
            contentFilter: filter,
            configuration: configuration
        )
        return CapturedDisplay(
            image: image,
            screenFrame: screen.frame,
            windowRectsFrontToBack: windowRects,
            displayID: displayID,
            backingScale: backingScale
        )
    }


    private static func contentFilter(
        display: SCDisplay,
        content: SCShareableContent,
        excludingWindowIDs: Set<CGWindowID>
    ) -> SCContentFilter {
        guard !excludingWindowIDs.isEmpty else {
            return SCContentFilter(display: display, excludingWindows: [])
        }
        let currentProcessID = ProcessInfo.processInfo.processIdentifier
        let toExclude = content.windows.filter {
            excludingWindowIDs.contains($0.windowID)
        }
        if let application = content.applications.first(where: {
            $0.processID == currentProcessID
        }) {
            // Mirror the recording backend: exclude the Kiri application but
            // re-include any Kiri window that must stay visible.
            let kiriWindows = content.windows.filter {
                $0.owningApplication?.processID == currentProcessID
            }
            let exceptedWindows = kiriWindows.filter {
                !excludingWindowIDs.contains($0.windowID)
            }
            return SCContentFilter(
                display: display,
                excludingApplications: [application],
                exceptingWindows: exceptedWindows
            )
        }
        return SCContentFilter(display: display, excludingWindows: toExclude)
    }

#if DEBUG
    private static func makeFixture() -> CapturedDisplay? {
        guard let screen = NSScreen.main else { return nil }
        let size = screen.frame.size
        let windows = [
            CGRect(x: 90, y: 75, width: min(620, size.width - 180), height: min(420, size.height - 180)),
            CGRect(x: max(240, size.width * 0.42), y: 155, width: min(520, size.width * 0.48), height: min(360, size.height - 240))
        ]
        let fixture = NSImage(size: size)
        fixture.lockFocus()

        NSColor(calibratedRed: 0.12, green: 0.14, blue: 0.19, alpha: 1).setFill()
        CGRect(origin: .zero, size: size).fill()
        drawFixtureWindow(
            topLeftRect: windows[1],
            in: size,
            color: NSColor(calibratedRed: 0.17, green: 0.22, blue: 0.31, alpha: 1),
            title: "Reference"
        )
        drawFixtureWindow(
            topLeftRect: windows[0],
            in: size,
            color: NSColor(calibratedRed: 0.95, green: 0.95, blue: 0.97, alpha: 1),
            title: "Kiri interaction fixture"
        )

        fixture.unlockFocus()
        guard let data = fixture.tiffRepresentation,
              let bitmap = NSBitmapImageRep(data: data),
              let pngData = bitmap.representation(using: .png, properties: [:]),
              let source = CGImageSourceCreateWithData(pngData as CFData, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
            return nil
        }
        return CapturedDisplay(
            image: image,
            screenFrame: screen.frame,
            windowRectsFrontToBack: windows,
            displayID: 0,
            backingScale: max(screen.backingScaleFactor, 1)
        )
    }

    private static func drawFixtureWindow(
        topLeftRect: CGRect,
        in canvasSize: CGSize,
        color: NSColor,
        title: String
    ) {
        let rect = CGRect(
            x: topLeftRect.minX,
            y: canvasSize.height - topLeftRect.maxY,
            width: topLeftRect.width,
            height: topLeftRect.height
        )
        let path = NSBezierPath(roundedRect: rect, xRadius: 12, yRadius: 12)
        color.setFill()
        path.fill()

        let titleBar = CGRect(x: rect.minX, y: rect.maxY - 42, width: rect.width, height: 42)
        NSColor.black.withAlphaComponent(0.08).setFill()
        titleBar.fill()
        (title as NSString).draw(
            at: CGPoint(x: rect.minX + 18, y: rect.maxY - 29),
            withAttributes: [
                .font: NSFont.systemFont(ofSize: 14, weight: .semibold),
                .foregroundColor: color.brightnessComponent > 0.6
                    ? NSColor.labelColor
                    : NSColor.white
            ]
        )
    }
#endif
}
