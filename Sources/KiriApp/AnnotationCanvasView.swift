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

enum AnnotationTextBackgroundStyle: CaseIterable {
    case transparent
    case dark
    case light

    var name: String {
        switch self {
        case .transparent: "Transparent"
        case .dark: "Dark"
        case .light: "Light"
        }
    }

    var color: NSColor? {
        switch self {
        case .transparent: nil
        case .dark: NSColor.black.withAlphaComponent(0.72)
        case .light: NSColor.white.withAlphaComponent(0.9)
        }
    }
}

private enum AnnotationMark {
    case pen([CGPoint], AnnotationColorPreset)
    case rectangle(CGRect, AnnotationColorPreset)
    case line(CGPoint, CGPoint, AnnotationColorPreset)
    case arrow(CGPoint, CGPoint, AnnotationColorPreset)
    case text(String, CGRect, AnnotationColorPreset, AnnotationTextBackgroundStyle)
    case mosaic(CGRect)
}

private final class InlineAnnotationTextView: NSTextView {
    var onCommit: (() -> Void)?
    var onCancel: (() -> Void)?
    var placeholder = "Type something…"

    override func doCommand(by selector: Selector) {
        if selector == #selector(NSResponder.insertNewline(_:)) {
            onCommit?()
            return
        }
        if selector == #selector(NSResponder.cancelOperation(_:)) {
            onCancel?()
            return
        }
        super.doCommand(by: selector)
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        guard string.isEmpty, markedRange().location == NSNotFound else { return }
        (placeholder as NSString).draw(
            at: CGPoint(x: textContainerInset.width, y: textContainerInset.height),
            withAttributes: [
                .font: font ?? NSFont.systemFont(ofSize: 18, weight: .semibold),
                .foregroundColor: NSColor.placeholderTextColor
            ]
        )
    }
}

final class AnnotationCanvasView: NSView, NSTextViewDelegate {
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
            updateTextEditorStyle()
            needsDisplay = true
        }
    }
    var textBackgroundStyle: AnnotationTextBackgroundStyle = .transparent {
        didSet {
            updateTextEditorStyle()
            needsDisplay = true
        }
    }

    private var history = AnnotationHistory<AnnotationMark>()
    private var draftPoints: [CGPoint] = []
    private var dragStart: CGPoint?
    private var dragCurrent: CGPoint?
    private var textEditor: InlineAnnotationTextView?
    private var textColorPreset: AnnotationColorPreset?
    private var editingTextBackgroundStyle: AnnotationTextBackgroundStyle?

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
        commitTextEditing()
        guard history.undo() != nil else { return }
        publishHistoryState()
        needsDisplay = true
    }

    func redo() {
        commitTextEditing()
        guard history.redo() != nil else { return }
        publishHistoryState()
        needsDisplay = true
    }

    func clearAnnotations() {
        discardTextEditing()
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
        case let .text(text, rect, colorPreset, backgroundStyle):
            drawText(
                text,
                in: rect,
                fontSize: 18,
                colorPreset: colorPreset,
                backgroundStyle: backgroundStyle
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
        case let .text(text, rect, colorPreset, backgroundStyle):
            let fontSize = max(18, 18 * min(scaleX, scaleY))
            let first = convert(CGPoint(x: rect.minX, y: rect.maxY))
            let second = convert(CGPoint(x: rect.maxX, y: rect.minY))
            let convertedRect = CGRect(
                x: min(first.x, second.x),
                y: min(first.y, second.y),
                width: abs(second.x - first.x),
                height: abs(second.y - first.y)
            )
            drawText(
                text,
                in: convertedRect,
                fontSize: fontSize,
                colorPreset: colorPreset,
                backgroundStyle: backgroundStyle,
                paddingScale: min(scaleX, scaleY)
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
        [
            NSAttributedString.Key.font: NSFont.systemFont(
                ofSize: fontSize,
                weight: .semibold
            ),
            NSAttributedString.Key.foregroundColor: colorPreset.color
        ]
    }

    private func drawText(
        _ text: String,
        in rect: CGRect,
        fontSize: CGFloat,
        colorPreset: AnnotationColorPreset,
        backgroundStyle: AnnotationTextBackgroundStyle,
        paddingScale: CGFloat = 1
    ) {
        if let backgroundColor = backgroundStyle.color {
            backgroundColor.setFill()
            let horizontalPadding = 5 * paddingScale
            let verticalPadding = 3 * paddingScale
            let backgroundRect = rect.insetBy(
                dx: -horizontalPadding,
                dy: -verticalPadding
            )
            NSBezierPath(
                roundedRect: backgroundRect,
                xRadius: 5 * paddingScale,
                yRadius: 5 * paddingScale
            ).fill()
        }
        (text as NSString).draw(
            with: rect,
            options: [.usesLineFragmentOrigin, .usesFontLeading],
            attributes: textAttributes(fontSize: fontSize, colorPreset: colorPreset)
        )
    }

    private func beginTextEditing(at point: CGPoint) {
        commitTextEditing()

        let editor = InlineAnnotationTextView(frame: .zero)
        editor.string = ""
        editor.font = .systemFont(ofSize: 18, weight: .semibold)
        editor.isRichText = false
        editor.importsGraphics = false
        editor.isAutomaticQuoteSubstitutionEnabled = false
        editor.isAutomaticDashSubstitutionEnabled = false
        editor.isAutomaticTextReplacementEnabled = false
        editor.isHorizontallyResizable = false
        editor.isVerticallyResizable = false
        editor.textContainerInset = CGSize(width: 8, height: 5)
        editor.textContainer?.widthTracksTextView = true
        editor.textContainer?.heightTracksTextView = false
        editor.drawsBackground = true
        editor.delegate = self
        editor.wantsLayer = true
        editor.layer?.cornerRadius = 7
        editor.layer?.cornerCurve = .continuous
        editor.layer?.borderWidth = 1
        editor.setAccessibilityLabel("Annotation text")
        editor.onCommit = { [weak self] in
            self?.commitTextEditing()
            self?.onConfirmRequested?()
        }
        editor.onCancel = { [weak self] in
            self?.discardTextEditing()
            self?.onCancelRequested?()
        }

        let width = min(180, max(96, imageRect.width))
        let origin = CGPoint(
            x: min(point.x, imageRect.maxX - width),
            y: min(point.y, imageRect.maxY - 34)
        )
        editor.frame = CGRect(origin: origin, size: CGSize(width: width, height: 34))
        addSubview(editor)
        textEditor = editor
        textColorPreset = colorPreset
        editingTextBackgroundStyle = textBackgroundStyle
        updateTextEditorStyle()
        resizeTextEditor()
        window?.makeFirstResponder(editor)
    }

    private func commitTextEditing() {
        guard let editor = textEditor else { return }
        let text = editor.string.trimmingCharacters(in: .whitespacesAndNewlines)
        let textRect = CGRect(
            x: editor.frame.minX + editor.textContainerInset.width,
            y: editor.frame.minY + editor.textContainerInset.height,
            width: max(1, editor.frame.width - editor.textContainerInset.width * 2),
            height: max(1, editor.frame.height - editor.textContainerInset.height * 2)
        )
        let colorPreset = textColorPreset
        let backgroundStyle = editingTextBackgroundStyle
        discardTextEditing()
        if !text.isEmpty, let colorPreset, let backgroundStyle {
            append(.text(text, textRect, colorPreset, backgroundStyle))
        }
    }

    private func discardTextEditing() {
        guard let editor = textEditor else { return }
        textEditor = nil
        textColorPreset = nil
        editingTextBackgroundStyle = nil
        editor.delegate = nil
        editor.onCommit = nil
        editor.onCancel = nil
        editor.removeFromSuperview()
        window?.makeFirstResponder(superview ?? self)
    }

    func textDidEndEditing(_ notification: Notification) {
        commitTextEditing()
    }

    func textDidChange(_ notification: Notification) {
        resizeTextEditor()
    }

    private func updateTextEditorStyle() {
        guard let editor = textEditor else { return }
        textColorPreset = colorPreset
        editingTextBackgroundStyle = textBackgroundStyle
        editor.textColor = colorPreset.color
        editor.insertionPointColor = colorPreset.color
        editor.backgroundColor = textBackgroundStyle.color ?? .clear
        editor.layer?.borderColor = colorPreset.color.withAlphaComponent(0.8).cgColor
        editor.needsDisplay = true
    }

    private func resizeTextEditor() {
        guard let editor = textEditor, let font = editor.font else { return }
        let horizontalPadding = editor.textContainerInset.width * 2
        let verticalPadding = editor.textContainerInset.height * 2
        let maximumWidth = max(96, imageRect.maxX - editor.frame.minX)
        let measuredText = editor.string.isEmpty ? editor.placeholder : editor.string
        let textBounds = (measuredText as NSString).boundingRect(
            with: CGSize(
                width: max(1, maximumWidth - horizontalPadding),
                height: .greatestFiniteMagnitude
            ),
            options: [.usesLineFragmentOrigin, .usesFontLeading],
            attributes: [.font: font]
        )
        let width = min(maximumWidth, max(120, ceil(textBounds.width) + horizontalPadding + 2))
        let maximumHeight = max(34, imageRect.maxY - editor.frame.minY)
        let height = min(maximumHeight, max(34, ceil(textBounds.height) + verticalPadding + 2))
        editor.setFrameSize(CGSize(width: width, height: height))
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
