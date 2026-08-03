import AppKit
import KiriCore

enum AnnotationTool: CaseIterable {
    case pen
    case rectangle
    case line
    case arrow
    case text
    case mosaic
}

enum AnnotationColorPreset: CaseIterable {
    case violet
    case cherry
    case orange
    case yellow
    case mint
    case blue
    case white
    case black

    var name: String {
        switch self {
        case .violet: "Violet"
        case .cherry: "Cherry"
        case .orange: "Orange"
        case .yellow: "Yellow"
        case .mint: "Mint"
        case .blue: "Blue"
        case .white: "White"
        case .black: "Black"
        }
    }

    var color: NSColor {
        switch self {
        case .violet:
            CaptureUIColors.accent
        case .cherry:
            NSColor(calibratedRed: 0.98, green: 0.28, blue: 0.43, alpha: 1)
        case .orange:
            NSColor(calibratedRed: 1.00, green: 0.49, blue: 0.18, alpha: 1)
        case .yellow:
            NSColor(calibratedRed: 1.00, green: 0.82, blue: 0.16, alpha: 1)
        case .mint:
            NSColor(calibratedRed: 0.16, green: 0.78, blue: 0.56, alpha: 1)
        case .blue:
            NSColor(calibratedRed: 0.16, green: 0.58, blue: 1.00, alpha: 1)
        case .white:
            .white
        case .black:
            NSColor(calibratedWhite: 0.08, alpha: 1)
        }
    }
}

private enum AnnotationMark {
    case pen([CGPoint], AnnotationColorPreset)
    case rectangle(CGRect, AnnotationColorPreset)
    case line(CGPoint, CGPoint, AnnotationColorPreset)
    case arrow(CGPoint, CGPoint, AnnotationColorPreset)
    case text(String, CGPoint, AnnotationColorPreset)
    case mosaic(CGRect)
}

final class AnnotationCanvasView: NSView, NSTextFieldDelegate {
    let image: CGImage
    var onToolChange: ((AnnotationTool) -> Void)?
    var onConfirmRequested: (() -> Void)?
    var onCancelRequested: (() -> Void)?
    var onHistoryChange: ((_ canUndo: Bool, _ canRedo: Bool) -> Void)? {
        didSet {
            publishHistoryState()
        }
    }
    var tool: AnnotationTool = .rectangle {
        didSet {
            if oldValue == .text, tool != .text {
                commitTextEditing()
            }
            onToolChange?(tool)
            needsDisplay = true
        }
    }
    var colorPreset: AnnotationColorPreset = .violet {
        didSet {
            needsDisplay = true
        }
    }

    private var history = AnnotationHistory<AnnotationMark>()
    private var draftPoints: [CGPoint] = []
    private var dragStart: CGPoint?
    private var dragCurrent: CGPoint?
    private weak var textEditor: NSTextField?
    private var textOrigin: CGPoint?
    private var textColorPreset: AnnotationColorPreset?

    init(image: CGImage) {
        self.image = image
        super.init(frame: .zero)
        wantsLayer = true
        layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        NSColor(calibratedWhite: 0.08, alpha: 1).setFill()
        bounds.fill()
        let target = imageRect
        NSImage(cgImage: image, size: target.size).draw(in: target)

        for mark in history.elements where mark.isMosaic {
            draw(mark)
        }
        for mark in history.elements where !mark.isMosaic {
            draw(mark)
        }
        if tool == .pen, draftPoints.count > 1 {
            draw(.pen(draftPoints, colorPreset))
        } else if let dragStart, let dragCurrent {
            switch tool {
            case .rectangle:
                draw(.rectangle(Self.rect(from: dragStart, to: dragCurrent), colorPreset))
            case .line:
                draw(.line(dragStart, dragCurrent, colorPreset))
            case .arrow:
                draw(.arrow(dragStart, dragCurrent, colorPreset))
            case .mosaic:
                drawMosaicPreview(in: Self.rect(from: dragStart, to: dragCurrent))
            case .pen, .text:
                break
            }
        }
    }

    override func mouseDown(with event: NSEvent) {
        let point = clampedPoint(convert(event.locationInWindow, from: nil))
        if tool == .text {
            beginTextEditing(at: point)
            return
        }
        dragStart = point
        dragCurrent = point
        draftPoints = tool == .pen ? [point] : []
    }

    override func mouseDragged(with event: NSEvent) {
        let point = clampedPoint(convert(event.locationInWindow, from: nil))
        dragCurrent = point
        if tool == .pen {
            draftPoints.append(point)
        }
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        mouseDragged(with: event)
        guard let start = dragStart, let end = dragCurrent else { return }
        switch tool {
        case .pen where draftPoints.count > 1:
            append(.pen(draftPoints, colorPreset))
        case .rectangle:
            append(.rectangle(Self.rect(from: start, to: end), colorPreset))
        case .line where Self.hasVisibleLength(from: start, to: end):
            append(.line(start, end, colorPreset))
        case .arrow:
            if Self.hasVisibleLength(from: start, to: end) {
                append(.arrow(start, end, colorPreset))
            }
        case .mosaic:
            let rect = Self.rect(from: start, to: end)
            if rect.width >= 4, rect.height >= 4 {
                append(.mosaic(rect))
            }
        case .text:
            break
        default:
            break
        }
        dragStart = nil
        dragCurrent = nil
        draftPoints = []
        needsDisplay = true
    }

    func undo() {
        guard history.undo() != nil else { return }
        publishHistoryState()
        needsDisplay = true
    }

    func redo() {
        guard history.redo() != nil else { return }
        publishHistoryState()
        needsDisplay = true
    }

    func clearAnnotations() {
        guard history.canUndo || history.canRedo else { return }
        history.clear()
        publishHistoryState()
        needsDisplay = true
    }

    func beginTextPlacement() {
        tool = .text
    }

    func renderedImage() -> CGImage? {
        commitTextEditing()
        let outputSize = NSSize(width: image.width, height: image.height)
        guard let bitmap = NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: image.width,
            pixelsHigh: image.height,
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: .deviceRGB,
            bytesPerRow: 0,
            bitsPerPixel: 0
        ), let context = NSGraphicsContext(bitmapImageRep: bitmap) else {
            return nil
        }

        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = context
        context.imageInterpolation = .high
        NSImage(cgImage: image, size: outputSize).draw(
            in: NSRect(origin: .zero, size: outputSize),
            from: .zero,
            operation: .copy,
            fraction: 1
        )

        for mark in history.elements where mark.isMosaic {
            drawForExport(mark, outputHeight: outputSize.height)
        }
        for mark in history.elements where !mark.isMosaic {
            drawForExport(mark, outputHeight: outputSize.height)
        }
        context.flushGraphics()
        NSGraphicsContext.restoreGraphicsState()
        return bitmap.cgImage
    }

    private var imageRect: CGRect {
        guard image.width > 0, image.height > 0 else { return .zero }
        let imageAspect = CGFloat(image.width) / CGFloat(image.height)
        let viewAspect = bounds.width / max(bounds.height, 1)
        if imageAspect > viewAspect {
            let height = bounds.width / imageAspect
            return CGRect(x: 0, y: (bounds.height - height) / 2, width: bounds.width, height: height)
        }
        let width = bounds.height * imageAspect
        return CGRect(x: (bounds.width - width) / 2, y: 0, width: width, height: bounds.height)
    }

    private func append(_ mark: AnnotationMark) {
        history.append(mark)
        publishHistoryState()
        needsDisplay = true
    }

    private func publishHistoryState() {
        onHistoryChange?(history.canUndo, history.canRedo)
    }

    private func clampedPoint(_ point: CGPoint) -> CGPoint {
        CGPoint(
            x: min(max(point.x, imageRect.minX), imageRect.maxX),
            y: min(max(point.y, imageRect.minY), imageRect.maxY)
        )
    }

    private static func rect(from start: CGPoint, to end: CGPoint) -> CGRect {
        CGRect(
            x: min(start.x, end.x),
            y: min(start.y, end.y),
            width: abs(end.x - start.x),
            height: abs(end.y - start.y)
        )
    }

    private static func hasVisibleLength(from start: CGPoint, to end: CGPoint) -> Bool {
        hypot(end.x - start.x, end.y - start.y) >= 3
    }

    private func draw(_ mark: AnnotationMark) {
        switch mark {
        case let .pen(points, colorPreset):
            colorPreset.color.setStroke()
            guard let first = points.first else { return }
            let path = NSBezierPath()
            path.move(to: first)
            points.dropFirst().forEach { path.line(to: $0) }
            path.lineWidth = 3
            path.lineCapStyle = .round
            path.lineJoinStyle = .round
            path.stroke()
        case let .rectangle(rect, colorPreset):
            colorPreset.color.setStroke()
            let path = NSBezierPath(roundedRect: rect, xRadius: 2, yRadius: 2)
            path.lineWidth = 3
            path.stroke()
        case let .line(start, end, colorPreset):
            colorPreset.color.setStroke()
            drawLine(from: start, to: end, width: 3)
        case let .arrow(start, end, colorPreset):
            colorPreset.color.setStroke()
            drawArrow(from: start, to: end, width: 3)
        case let .text(text, point, colorPreset):
            text.draw(
                at: point,
                withAttributes: textAttributes(fontSize: 18, colorPreset: colorPreset)
            )
        case let .mosaic(rect):
            drawMosaicPreview(in: rect)
        }
    }

    private func drawForExport(_ mark: AnnotationMark, outputHeight: CGFloat) {
        let scaleX = CGFloat(image.width) / imageRect.width
        let scaleY = CGFloat(image.height) / imageRect.height
        func convert(_ point: CGPoint) -> CGPoint {
            let x = (point.x - imageRect.minX) * scaleX
            let topY = (point.y - imageRect.minY) * scaleY
            return CGPoint(x: x, y: outputHeight - topY)
        }

        switch mark {
        case let .pen(points, colorPreset):
            colorPreset.color.setStroke()
            guard let first = points.first else { return }
            let path = NSBezierPath()
            path.move(to: convert(first))
            points.dropFirst().forEach { path.line(to: convert($0)) }
            path.lineWidth = max(4, 3 * scaleX)
            path.lineCapStyle = .round
            path.lineJoinStyle = .round
            path.stroke()
        case let .rectangle(rect, colorPreset):
            colorPreset.color.setStroke()
            let first = convert(CGPoint(x: rect.minX, y: rect.maxY))
            let second = convert(CGPoint(x: rect.maxX, y: rect.minY))
            let converted = CGRect(
                x: min(first.x, second.x),
                y: min(first.y, second.y),
                width: abs(second.x - first.x),
                height: abs(second.y - first.y)
            )
            let path = NSBezierPath(roundedRect: converted, xRadius: 3, yRadius: 3)
            path.lineWidth = max(4, 3 * scaleX)
            path.stroke()
        case let .line(start, end, colorPreset):
            colorPreset.color.setStroke()
            drawLine(
                from: convert(start),
                to: convert(end),
                width: max(4, 3 * scaleX)
            )
        case let .arrow(start, end, colorPreset):
            colorPreset.color.setStroke()
            drawArrow(from: convert(start), to: convert(end), width: max(4, 3 * scaleX))
        case let .text(text, point, colorPreset):
            let fontSize = max(18, 18 * min(scaleX, scaleY))
            let converted = convert(point)
            text.draw(
                at: CGPoint(x: converted.x, y: converted.y - fontSize),
                withAttributes: textAttributes(fontSize: fontSize, colorPreset: colorPreset)
            )
        case let .mosaic(rect):
            let topLeft = convert(CGPoint(x: rect.minX, y: rect.maxY))
            let bottomRight = convert(CGPoint(x: rect.maxX, y: rect.minY))
            let outputRect = CGRect(
                x: min(topLeft.x, bottomRight.x),
                y: min(topLeft.y, bottomRight.y),
                width: abs(bottomRight.x - topLeft.x),
                height: abs(bottomRight.y - topLeft.y)
            )
            guard let mosaic = pixelatedCrop(for: rect) else { return }
            NSGraphicsContext.current?.imageInterpolation = .none
            mosaic.draw(
                in: outputRect,
                from: NSRect(origin: .zero, size: mosaic.size),
                operation: .copy,
                fraction: 1
            )
        }
    }

    private func textAttributes(
        fontSize: CGFloat,
        colorPreset: AnnotationColorPreset
    ) -> [NSAttributedString.Key: Any] {
        let color = colorPreset.color
        let background = color.brightnessComponent < 0.36
            ? NSColor.white.withAlphaComponent(0.84)
            : NSColor.black.withAlphaComponent(0.68)
        return [
            NSAttributedString.Key.font: NSFont.systemFont(
                ofSize: fontSize,
                weight: .semibold
            ),
            NSAttributedString.Key.foregroundColor: color,
            NSAttributedString.Key.backgroundColor: background
        ]
    }

    private func beginTextEditing(at point: CGPoint) {
        commitTextEditing()

        let editor = NSTextField(string: "")
        editor.placeholderString = "Type something…"
        editor.font = .systemFont(ofSize: 18, weight: .semibold)
        editor.textColor = colorPreset.color
        editor.backgroundColor = colorPreset.color.brightnessComponent < 0.36
            ? NSColor.white.withAlphaComponent(0.9)
            : NSColor.black.withAlphaComponent(0.74)
        editor.isBordered = false
        editor.isBezeled = false
        editor.drawsBackground = true
        editor.focusRingType = .exterior
        editor.delegate = self
        editor.wantsLayer = true
        editor.layer?.cornerRadius = 7
        editor.layer?.cornerCurve = .continuous
        editor.setAccessibilityLabel("Annotation text")

        let width = min(260, max(120, imageRect.maxX - point.x))
        let origin = CGPoint(
            x: min(point.x, imageRect.maxX - width),
            y: min(point.y, imageRect.maxY - 30)
        )
        editor.frame = CGRect(origin: origin, size: CGSize(width: width, height: 30))
        addSubview(editor)
        textEditor = editor
        textOrigin = origin
        textColorPreset = colorPreset
        window?.makeFirstResponder(editor)
    }

    private func commitTextEditing() {
        guard let editor = textEditor else { return }
        let text = editor.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        let origin = textOrigin
        let colorPreset = textColorPreset
        discardTextEditing()
        if !text.isEmpty, let origin, let colorPreset {
            append(.text(text, origin, colorPreset))
        }
    }

    private func discardTextEditing() {
        guard let editor = textEditor else { return }
        textEditor = nil
        textOrigin = nil
        textColorPreset = nil
        editor.delegate = nil
        editor.removeFromSuperview()
        window?.makeFirstResponder(superview ?? self)
    }

    func controlTextDidEndEditing(_ notification: Notification) {
        commitTextEditing()
    }

    func control(
        _ control: NSControl,
        textView: NSTextView,
        doCommandBy commandSelector: Selector
    ) -> Bool {
        if commandSelector == #selector(NSResponder.insertNewline(_:)) {
            commitTextEditing()
            onConfirmRequested?()
            return true
        }
        if commandSelector == #selector(NSResponder.cancelOperation(_:)) {
            discardTextEditing()
            onCancelRequested?()
            return true
        }
        return false
    }

    private func drawMosaicPreview(in rect: CGRect) {
        guard let mosaic = pixelatedCrop(for: rect) else { return }
        NSGraphicsContext.current?.imageInterpolation = .none
        mosaic.draw(
            in: rect,
            from: NSRect(origin: .zero, size: mosaic.size),
            operation: .copy,
            fraction: 1
        )
    }

    private func pixelatedCrop(for viewRect: CGRect) -> NSImage? {
        let clipped = viewRect.standardized.intersection(imageRect)
        guard clipped.width >= 1, clipped.height >= 1 else { return nil }
        let scaleX = CGFloat(image.width) / imageRect.width
        let scaleY = CGFloat(image.height) / imageRect.height
        let cropRect = CGRect(
            x: (clipped.minX - imageRect.minX) * scaleX,
            y: (clipped.minY - imageRect.minY) * scaleY,
            width: clipped.width * scaleX,
            height: clipped.height * scaleY
        ).integral.intersection(
            CGRect(x: 0, y: 0, width: image.width, height: image.height)
        )
        guard let cropped = image.cropping(to: cropRect) else { return nil }

        let blockSize: CGFloat = 12
        let smallSize = NSSize(
            width: max(1, ceil(cropRect.width / blockSize)),
            height: max(1, ceil(cropRect.height / blockSize))
        )
        let small = NSImage(size: smallSize)
        small.lockFocus()
        NSGraphicsContext.current?.imageInterpolation = .none
        NSImage(cgImage: cropped, size: smallSize).draw(
            in: NSRect(origin: .zero, size: smallSize),
            from: .zero,
            operation: .copy,
            fraction: 1
        )
        small.unlockFocus()
        return small
    }

    private func drawArrow(from start: CGPoint, to end: CGPoint, width: CGFloat) {
        drawLine(from: start, to: end, width: width)

        let angle = atan2(end.y - start.y, end.x - start.x)
        let headLength = max(12, width * 4)
        let left = CGPoint(
            x: end.x - headLength * cos(angle - .pi / 6),
            y: end.y - headLength * sin(angle - .pi / 6)
        )
        let right = CGPoint(
            x: end.x - headLength * cos(angle + .pi / 6),
            y: end.y - headLength * sin(angle + .pi / 6)
        )
        let head = NSBezierPath()
        head.move(to: left)
        head.line(to: end)
        head.line(to: right)
        head.lineWidth = width
        head.lineCapStyle = .round
        head.lineJoinStyle = .round
        head.stroke()
    }

    private func drawLine(from start: CGPoint, to end: CGPoint, width: CGFloat) {
        let path = NSBezierPath()
        path.move(to: start)
        path.line(to: end)
        path.lineWidth = width
        path.lineCapStyle = .round
        path.stroke()
    }
}

enum CaptureFilename {
    static func timestamp(date: Date = Date()) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        return formatter.string(from: date)
    }
}

private extension AnnotationMark {
    var isMosaic: Bool {
        if case .mosaic = self {
            true
        } else {
            false
        }
    }
}
