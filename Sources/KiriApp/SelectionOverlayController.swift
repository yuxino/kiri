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
    private var selection: CGRect = .null

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
            drawDimensions()
        } else {
            bounds.fill()
        }
    }

    override func mouseDown(with event: NSEvent) {
        dragStart = convert(event.locationInWindow, from: nil)
        selection = .null
        needsDisplay = true
    }

    override func mouseDragged(with event: NSEvent) {
        guard let dragStart else { return }
        let current = convert(event.locationInWindow, from: nil)
        selection = SelectionGeometry.clamped(
            SelectionGeometry.normalized(from: dragStart, to: current),
            to: bounds
        )
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        mouseDragged(with: event)
        guard SelectionGeometry.isValid(selection) else {
            selection = .null
            needsDisplay = true
            return
        }
        onSelection?(selection)
    }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53 {
            onCancel?()
        } else {
            super.keyDown(with: event)
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
}

