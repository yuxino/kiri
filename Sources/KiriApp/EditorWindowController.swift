import AppKit

@MainActor
final class EditorWindowController: NSWindowController {
    typealias Completion = (CGImage, Bool, URL?) -> Void

    private let completion: Completion
    private let canvas: AnnotationCanvasView

    init(image: CGImage, completion: @escaping Completion) {
        self.completion = completion
        canvas = AnnotationCanvasView(image: image)
        let window = NSWindow(
            contentRect: CGRect(x: 0, y: 0, width: 880, height: 620),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "kiri"
        window.center()
        super.init(window: window)
        window.contentViewController = makeContentViewController()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    private func makeContentViewController() -> NSViewController {
        let controller = NSViewController()
        let root = NSView()
        root.translatesAutoresizingMaskIntoConstraints = false

        let toolbar = NSStackView(views: [
            button("Pen", action: #selector(usePen)),
            button("Rectangle", action: #selector(useRectangle)),
            button("Arrow", action: #selector(useArrow)),
            button("Undo", action: #selector(undo)),
            NSView(),
            button("Cancel", action: #selector(cancel)),
            button("Save…", action: #selector(save)),
            button("Copy", action: #selector(copyImage))
        ])
        toolbar.orientation = .horizontal
        toolbar.spacing = 8
        toolbar.edgeInsets = NSEdgeInsets(top: 8, left: 10, bottom: 8, right: 10)
        toolbar.translatesAutoresizingMaskIntoConstraints = false
        canvas.translatesAutoresizingMaskIntoConstraints = false

        root.addSubview(toolbar)
        root.addSubview(canvas)
        NSLayoutConstraint.activate([
            toolbar.topAnchor.constraint(equalTo: root.topAnchor),
            toolbar.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            toolbar.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            toolbar.heightAnchor.constraint(equalToConstant: 48),
            canvas.topAnchor.constraint(equalTo: toolbar.bottomAnchor),
            canvas.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            canvas.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            canvas.bottomAnchor.constraint(equalTo: root.bottomAnchor)
        ])
        controller.view = root
        return controller
    }

    private func button(_ title: String, action: Selector) -> NSButton {
        let button = NSButton(title: title, target: self, action: action)
        button.bezelStyle = .texturedRounded
        return button
    }

    @objc private func usePen() {
        canvas.tool = .pen
    }

    @objc private func useRectangle() {
        canvas.tool = .rectangle
    }

    @objc private func useArrow() {
        canvas.tool = .arrow
    }

    @objc private func undo() {
        canvas.undo()
    }

    @objc private func cancel() {
        close()
    }

    @objc private func copyImage() {
        guard let image = canvas.renderedImage() else { return }
        completion(image, true, nil)
        close()
    }

    @objc private func save() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.png]
        panel.nameFieldStringValue = "kiri-\(Self.timestamp()).png"
        guard panel.runModal() == .OK, let url = panel.url, let image = canvas.renderedImage() else {
            return
        }
        completion(image, false, url)
        close()
    }

    private static func timestamp() -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        return formatter.string(from: Date())
    }
}

private enum AnnotationTool {
    case pen
    case rectangle
    case arrow
}

private enum AnnotationMark {
    case pen([CGPoint])
    case rectangle(CGRect)
    case arrow(CGPoint, CGPoint)
}

private final class AnnotationCanvasView: NSView {
    let image: CGImage
    var tool: AnnotationTool = .rectangle

    private var marks: [AnnotationMark] = []
    private var draftPoints: [CGPoint] = []
    private var dragStart: CGPoint?
    private var dragCurrent: CGPoint?

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

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        NSColor(calibratedWhite: 0.08, alpha: 1).setFill()
        bounds.fill()
        let target = imageRect
        NSImage(cgImage: image, size: target.size).draw(in: target)

        for mark in marks {
            draw(mark)
        }
        if tool == .pen, draftPoints.count > 1 {
            draw(.pen(draftPoints))
        } else if let dragStart, let dragCurrent {
            switch tool {
            case .rectangle:
                draw(.rectangle(CGRect(
                    x: min(dragStart.x, dragCurrent.x),
                    y: min(dragStart.y, dragCurrent.y),
                    width: abs(dragCurrent.x - dragStart.x),
                    height: abs(dragCurrent.y - dragStart.y)
                )))
            case .arrow:
                draw(.arrow(dragStart, dragCurrent))
            case .pen:
                break
            }
        }
    }

    override func mouseDown(with event: NSEvent) {
        let point = clampedPoint(convert(event.locationInWindow, from: nil))
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
            marks.append(.rectangle(CGRect(
                x: min(start.x, end.x),
                y: min(start.y, end.y),
                width: abs(end.x - start.x),
                height: abs(end.y - start.y)
            )))
        case .arrow:
            marks.append(.arrow(start, end))
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

        for mark in marks {
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

    private func draw(_ mark: AnnotationMark) {
        let color = NSColor(calibratedRed: 0.47, green: 0.35, blue: 0.95, alpha: 1)
        color.setStroke()
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

        NSColor(calibratedRed: 0.47, green: 0.35, blue: 0.95, alpha: 1).setStroke()
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
        }
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

