import AppKit
import KiriCore

@MainActor
final class SelectionOverlayController {
    private let capture: CapturedDisplay
    private var window: NSWindow?

    init(capture: CapturedDisplay) {
        self.capture = capture
    }

    func present(onSelect: @escaping (CGImage) -> Void, onCancel: @escaping () -> Void) {
        let window = NSWindow(
            contentRect: capture.screenFrame,
            styleMask: .borderless,
            backing: .buffered,
            defer: false
        )
        window.level = .screenSaver
        window.backgroundColor = .clear
        window.isOpaque = false
        window.hasShadow = false
        window.acceptsMouseMovedEvents = true
        window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]

        let overlay = SelectionOverlayView(image: capture.image)
        overlay.onCancel = { [weak self] in
            self?.close()
            onCancel()
        }
        overlay.onSelection = { [weak self] rect in
            guard let self else { return }
            let pixelRect = SelectionGeometry.pixelRect(
                forTopLeftRect: rect,
                canvasSize: overlay.bounds.size,
                imageSize: CGSize(width: capture.image.width, height: capture.image.height)
            ).intersection(
                CGRect(
                    x: 0,
                    y: 0,
                    width: capture.image.width,
                    height: capture.image.height
                )
            )
            guard let cropped = capture.image.cropping(to: pixelRect) else { return }
            close()
            onSelect(cropped)
        }

        window.contentView = overlay
        self.window = window
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(overlay)
        NSCursor.crosshair.push()
    }

    private func close() {
        NSCursor.pop()
        window?.orderOut(nil)
        window?.close()
        window = nil
    }
}

private final class SelectionOverlayView: NSView {
    let image: CGImage
    var onSelection: ((CGRect) -> Void)?
    var onCancel: (() -> Void)?

    private var dragStart: CGPoint?
    private var interaction: SelectionInteraction?
    private var selection: CGRect = .null
    private var hoverPoint: CGPoint?

    init(image: CGImage) {
        self.image = image
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        NSImage(cgImage: image, size: bounds.size).draw(in: bounds)

        NSColor.black.withAlphaComponent(0.46).setFill()
        if SelectionGeometry.isValid(selection) {
            NSRect(x: 0, y: 0, width: bounds.width, height: selection.minY).fill()
            NSRect(x: 0, y: selection.maxY, width: bounds.width, height: bounds.height - selection.maxY).fill()
            NSRect(x: 0, y: selection.minY, width: selection.minX, height: selection.height).fill()
            NSRect(
                x: selection.maxX,
                y: selection.minY,
                width: bounds.width - selection.maxX,
                height: selection.height
            ).fill()

            let border = NSBezierPath(rect: selection)
            border.lineWidth = 1.5
            NSColor(calibratedRed: 0.62, green: 0.62, blue: 0.91, alpha: 1).setStroke()
            border.stroke()
            drawHandles()
            drawDimensions()
            drawHint()
        } else {
            bounds.fill()
        }
        drawLoupe()
    }

    override func mouseDown(with event: NSEvent) {
        let point = clampedPoint(convert(event.locationInWindow, from: nil))
        if event.clickCount >= 2, SelectionGeometry.isValid(selection), selection.contains(point) {
            onSelection?(selection)
            return
        }

        dragStart = point
        if let handle = SelectionGeometry.hitTest(
            point,
            selection: selection,
            radius: 10
        ) {
            interaction = .resizing(handle: handle, original: selection)
        } else if SelectionGeometry.isValid(selection), selection.contains(point) {
            interaction = .moving(original: selection)
            NSCursor.closedHand.set()
        } else {
            interaction = .creating
            selection = .null
        }
        hoverPoint = point
        needsDisplay = true
    }

    override func mouseMoved(with event: NSEvent) {
        let point = clampedPoint(convert(event.locationInWindow, from: nil))
        hoverPoint = point
        updateCursor(at: point)
        needsDisplay = true
    }

    override func mouseExited(with event: NSEvent) {
        hoverPoint = nil
        needsDisplay = true
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(
            NSTrackingArea(
                rect: bounds,
                options: [
                    .activeAlways,
                    .mouseMoved,
                    .mouseEnteredAndExited,
                    .cursorUpdate,
                    .inVisibleRect
                ],
                owner: self
            )
        )
    }

    override func cursorUpdate(with event: NSEvent) {
        updateCursor(at: clampedPoint(convert(event.locationInWindow, from: nil)))
    }

    override func mouseEntered(with event: NSEvent) {
        mouseMoved(with: event)
    }

    override func mouseDragged(with event: NSEvent) {
        guard let dragStart, let interaction else { return }
        let current = clampedPoint(convert(event.locationInWindow, from: nil))
        switch interaction {
        case .creating:
            selection = SelectionGeometry.clamped(
                SelectionGeometry.normalized(from: dragStart, to: current),
                to: bounds
            )
        case let .moving(original):
            selection = SelectionGeometry.moved(
                original,
                by: CGSize(
                    width: current.x - dragStart.x,
                    height: current.y - dragStart.y
                ),
                within: bounds
            )
        case let .resizing(handle, original):
            selection = SelectionGeometry.resized(
                original,
                using: handle,
                to: current,
                within: bounds,
                minimumSide: 8
            )
        }
        hoverPoint = current
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        mouseDragged(with: event)
        if !SelectionGeometry.isValid(selection) {
            selection = .null
        }
        dragStart = nil
        interaction = nil
        if let hoverPoint {
            updateCursor(at: hoverPoint)
        }
        needsDisplay = true
    }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53 {
            onCancel?()
        } else if event.keyCode == 36 || event.keyCode == 76 {
            if SelectionGeometry.isValid(selection) {
                onSelection?(selection)
            }
        } else {
            super.keyDown(with: event)
        }
    }

    private func clampedPoint(_ point: CGPoint) -> CGPoint {
        CGPoint(
            x: min(max(point.x, bounds.minX), bounds.maxX),
            y: min(max(point.y, bounds.minY), bounds.maxY)
        )
    }

    private func updateCursor(at point: CGPoint) {
        guard interaction == nil else { return }
        if let handle = SelectionGeometry.hitTest(
            point,
            selection: selection,
            radius: 10
        ) {
            switch handle {
            case .top, .bottom:
                NSCursor.resizeUpDown.set()
            case .left, .right:
                NSCursor.resizeLeftRight.set()
            case .topLeft, .topRight, .bottomRight, .bottomLeft:
                NSCursor.crosshair.set()
            }
        } else if SelectionGeometry.isValid(selection), selection.contains(point) {
            NSCursor.openHand.set()
        } else {
            NSCursor.crosshair.set()
        }
    }

    private func drawHandles() {
        for handle in SelectionHandle.allCases {
            let point = SelectionGeometry.handlePoint(for: handle, in: selection)
            let rect = CGRect(x: point.x - 4, y: point.y - 4, width: 8, height: 8)
            let path = NSBezierPath(ovalIn: rect)
            NSColor.white.setFill()
            NSColor(calibratedRed: 0.47, green: 0.41, blue: 0.86, alpha: 1).setStroke()
            path.lineWidth = 1.5
            path.fill()
            path.stroke()
        }
    }

    private func drawDimensions() {
        let text = "\(Int(selection.width)) × \(Int(selection.height))" as NSString
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.monospacedSystemFont(ofSize: 11, weight: .medium),
            .foregroundColor: NSColor.white,
            .backgroundColor: NSColor.black.withAlphaComponent(0.72)
        ]
        let size = text.size(withAttributes: attributes)
        var origin = CGPoint(x: selection.minX, y: selection.minY - size.height - 5)
        if origin.y < 4 {
            origin.y = selection.minY + 5
        }
        text.draw(at: origin, withAttributes: attributes)
    }

    private func drawHint() {
        let text = "Drag to move or resize · Double-click / Return to capture · Esc to cancel" as NSString
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 11, weight: .medium),
            .foregroundColor: NSColor.white,
            .backgroundColor: NSColor.black.withAlphaComponent(0.7)
        ]
        let size = text.size(withAttributes: attributes)
        var origin = CGPoint(
            x: selection.maxX - size.width,
            y: selection.maxY + 6
        )
        origin.x = min(max(origin.x, 6), bounds.maxX - size.width - 6)
        if origin.y + size.height > bounds.maxY - 6 {
            origin.y = selection.maxY - size.height - 6
        }
        text.draw(at: origin, withAttributes: attributes)
    }

    private func drawLoupe() {
        guard let hoverPoint else { return }
        let scaleX = CGFloat(image.width) / bounds.width
        let scaleY = CGFloat(image.height) / bounds.height
        let center = CGPoint(x: hoverPoint.x * scaleX, y: hoverPoint.y * scaleY)
        let sampleSide: CGFloat = 11
        let sourceRect = CGRect(
            x: center.x - sampleSide / 2,
            y: center.y - sampleSide / 2,
            width: sampleSide,
            height: sampleSide
        ).integral.intersection(
            CGRect(x: 0, y: 0, width: image.width, height: image.height)
        )
        guard let sample = image.cropping(to: sourceRect) else { return }

        let side: CGFloat = 88
        var origin = CGPoint(x: hoverPoint.x + 18, y: hoverPoint.y + 18)
        if origin.x + side > bounds.maxX - 8 {
            origin.x = hoverPoint.x - side - 18
        }
        if origin.y + side > bounds.maxY - 8 {
            origin.y = hoverPoint.y - side - 18
        }
        origin.x = min(max(origin.x, 8), bounds.maxX - side - 8)
        origin.y = min(max(origin.y, 8), bounds.maxY - side - 8)
        let loupeRect = CGRect(origin: origin, size: CGSize(width: side, height: side))

        NSGraphicsContext.current?.imageInterpolation = .none
        NSImage(cgImage: sample, size: loupeRect.size).draw(in: loupeRect)

        let border = NSBezierPath(roundedRect: loupeRect, xRadius: 6, yRadius: 6)
        border.lineWidth = 2
        NSColor.white.setStroke()
        border.stroke()

        let crosshair = NSBezierPath()
        crosshair.move(to: CGPoint(x: loupeRect.midX, y: loupeRect.minY))
        crosshair.line(to: CGPoint(x: loupeRect.midX, y: loupeRect.maxY))
        crosshair.move(to: CGPoint(x: loupeRect.minX, y: loupeRect.midY))
        crosshair.line(to: CGPoint(x: loupeRect.maxX, y: loupeRect.midY))
        crosshair.lineWidth = 1
        NSColor.white.withAlphaComponent(0.8).setStroke()
        crosshair.stroke()
    }
}

private enum SelectionInteraction: Equatable {
    case creating
    case moving(original: CGRect)
    case resizing(handle: SelectionHandle, original: CGRect)
}
