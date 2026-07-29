import AppKit

enum AnnotationTool: CaseIterable {
    case pen
    case rectangle
    case arrow
    case text
    case mosaic
}

private enum AnnotationMark {
    case pen([CGPoint])
    case rectangle(CGRect)
    case arrow(CGPoint, CGPoint)
    case text(String, CGPoint)
    case mosaic(CGRect)
}

final class AnnotationCanvasView: NSView {
    let image: CGImage
    var onToolChange: ((AnnotationTool) -> Void)?
    var tool: AnnotationTool = .rectangle {
        didSet {
            onToolChange?(tool)
            needsDisplay = true
        }
    }

    private var marks: [AnnotationMark] = []
    private var draftPoints: [CGPoint] = []
    private var dragStart: CGPoint?
    private var dragCurrent: CGPoint?
    private var pendingText: String?

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

        for mark in marks where mark.isMosaic {
            draw(mark)
        }
        for mark in marks where !mark.isMosaic {
            draw(mark)
        }
        if tool == .pen, draftPoints.count > 1 {
            draw(.pen(draftPoints))
        } else if let dragStart, let dragCurrent {
            switch tool {
            case .rectangle:
                draw(.rectangle(Self.rect(from: dragStart, to: dragCurrent)))
            case .arrow:
                draw(.arrow(dragStart, dragCurrent))
            case .mosaic:
                drawMosaicPreview(in: Self.rect(from: dragStart, to: dragCurrent))
            case .pen, .text:
                break
            }
        }
    }

    override func mouseDown(with event: NSEvent) {
        let point = clampedPoint(convert(event.locationInWindow, from: nil))
        if tool == .text, let pendingText {
            marks.append(.text(pendingText, point))
            self.pendingText = nil
            tool = .rectangle
            needsDisplay = true
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
            marks.append(.pen(draftPoints))
        case .rectangle:
            marks.append(.rectangle(Self.rect(from: start, to: end)))
        case .arrow:
            marks.append(.arrow(start, end))
        case .mosaic:
            let rect = Self.rect(from: start, to: end)
            if rect.width >= 4, rect.height >= 4 {
                marks.append(.mosaic(rect))
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
        _ = marks.popLast()
        needsDisplay = true
    }

    func beginTextPlacement(_ text: String) {
        pendingText = text
        tool = .text
    }

    func renderedImage() -> CGImage? {
        let outputSize = NSSize(width: image.width, height: image.height)
        let output = NSImage(size: outputSize)
        output.lockFocus()
        NSGraphicsContext.current?.imageInterpolation = .high
        NSImage(cgImage: image, size: outputSize).draw(
            in: NSRect(origin: .zero, size: outputSize),
            from: .zero,
            operation: .copy,
            fraction: 1
        )

        for mark in marks where mark.isMosaic {
            drawForExport(mark, outputHeight: outputSize.height)
        }
        for mark in marks where !mark.isMosaic {
            drawForExport(mark, outputHeight: outputSize.height)
        }
        output.unlockFocus()

        guard let data = output.tiffRepresentation,
              let bitmap = NSBitmapImageRep(data: data) else {
            return nil
        }
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

    private func draw(_ mark: AnnotationMark) {
        annotationColor.setStroke()
        switch mark {
        case let .pen(points):
            guard let first = points.first else { return }
            let path = NSBezierPath()
            path.move(to: first)
            points.dropFirst().forEach { path.line(to: $0) }
            path.lineWidth = 3
            path.lineCapStyle = .round
            path.lineJoinStyle = .round
            path.stroke()
        case let .rectangle(rect):
            let path = NSBezierPath(roundedRect: rect, xRadius: 2, yRadius: 2)
            path.lineWidth = 3
            path.stroke()
        case let .arrow(start, end):
            drawArrow(from: start, to: end, width: 3)
        case let .text(text, point):
            text.draw(at: point, withAttributes: textAttributes(fontSize: 18))
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

        annotationColor.setStroke()
        switch mark {
        case let .pen(points):
            guard let first = points.first else { return }
            let path = NSBezierPath()
            path.move(to: convert(first))
            points.dropFirst().forEach { path.line(to: convert($0)) }
            path.lineWidth = max(4, 3 * scaleX)
            path.lineCapStyle = .round
            path.lineJoinStyle = .round
            path.stroke()
        case let .rectangle(rect):
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
        case let .arrow(start, end):
            drawArrow(from: convert(start), to: convert(end), width: max(4, 3 * scaleX))
        case let .text(text, point):
            let fontSize = max(18, 18 * min(scaleX, scaleY))
            let converted = convert(point)
            text.draw(
                at: CGPoint(x: converted.x, y: converted.y - fontSize),
                withAttributes: textAttributes(fontSize: fontSize)
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

    private var annotationColor: NSColor {
        NSColor(calibratedRed: 0.47, green: 0.35, blue: 0.95, alpha: 1)
    }

    private func textAttributes(fontSize: CGFloat) -> [NSAttributedString.Key: Any] {
        [
            .font: NSFont.systemFont(ofSize: fontSize, weight: .semibold),
            .foregroundColor: NSColor.white,
            .backgroundColor: NSColor.black.withAlphaComponent(0.68)
        ]
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
        let path = NSBezierPath()
        path.move(to: start)
        path.line(to: end)
        path.lineWidth = width
        path.lineCapStyle = .round
        path.stroke()

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
}

enum AnnotationTextPrompt {
    @MainActor
    static func requestText() -> String? {
        let alert = NSAlert()
        alert.messageText = "Add text"
        alert.informativeText = "Enter the text, then click the image to place it."
        alert.addButton(withTitle: "Place")
        alert.addButton(withTitle: "Cancel")
        let field = NSTextField(string: "")
        field.placeholderString = "Text"
        field.frame = CGRect(x: 0, y: 0, width: 280, height: 24)
        alert.accessoryView = field
        alert.window.initialFirstResponder = field

        guard alert.runModal() == .alertFirstButtonReturn else { return nil }
        let text = field.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        return text.isEmpty ? nil : text
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
