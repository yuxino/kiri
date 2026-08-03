import AppKit
import KiriCore

enum AnnotationTool: CaseIterable {
    case select
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

enum MosaicIntensityPreset: CaseIterable, Equatable {
    case soft
    case standard
    case strong

    var name: String {
        switch self {
        case .soft: "Soft"
        case .standard: "Standard"
        case .strong: "Strong"
        }
    }

    var viewBlockSize: CGFloat {
        switch self {
        case .soft: 7
        case .standard: 12
        case .strong: 20
        }
    }
}

private enum AnnotationMark {
    case pen([CGPoint], AnnotationColorPreset, CGFloat)
    case rectangle(CGRect, AnnotationColorPreset, CGFloat)
    case line(CGPoint, CGPoint, AnnotationColorPreset, CGFloat)
    case arrow(CGPoint, CGPoint, AnnotationColorPreset, CGFloat)
    case text(String, CGRect, AnnotationColorPreset, AnnotationTextBackgroundStyle, CGFloat)
    case mosaic([CGPoint], CGFloat, MosaicIntensityPreset)
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
    var tool: AnnotationTool = .select {
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
    var mosaicIntensity: MosaicIntensityPreset = .standard {
        didSet {
            needsDisplay = true
        }
    }
    var penWidth: CGFloat = 3 {
        didSet { needsDisplay = true }
    }
    var shapeWidth: CGFloat = 3 {
        didSet { needsDisplay = true }
    }
    var textFontSize: CGFloat = 18 {
        didSet {
            updateTextEditorStyle()
            resizeTextEditor()
            needsDisplay = true
        }
    }
    var mosaicBrushDiameter: CGFloat = 36 {
        didSet {
            needsDisplay = true
        }
    }

    private var history = AnnotationHistory<AnnotationMark>()
    private var draftPoints: [CGPoint] = []
    private var dragStart: CGPoint?
    private var dragCurrent: CGPoint?
    private var hoverPoint: CGPoint?
    private var textEditor: InlineAnnotationTextView?
    private var textColorPreset: AnnotationColorPreset?
    private var editingTextBackgroundStyle: AnnotationTextBackgroundStyle?
    private var editingTextFontSize: CGFloat?
    private var editingTextMarkIndex: Int?
    private var selectedMarkIndex: Int?
    private var selectionDragOriginalMark: AnnotationMark?
    private var selectionDragPreviewMark: AnnotationMark?

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

        for (index, mark) in history.elements.enumerated() where mark.isMosaic {
            guard index != editingTextMarkIndex else { continue }
            draw(displayMark(mark, at: index))
        }
        for (index, mark) in history.elements.enumerated() where !mark.isMosaic {
            guard index != editingTextMarkIndex else { continue }
            draw(displayMark(mark, at: index))
        }
        if tool == .pen, draftPoints.count > 1 {
            draw(.pen(draftPoints, colorPreset, penWidth))
        } else if tool == .mosaic, !draftPoints.isEmpty {
            drawMosaicStroke(
                points: draftPoints,
                brushDiameter: mosaicBrushDiameter,
                intensity: mosaicIntensity
            )
        } else if let dragStart, let dragCurrent {
            switch tool {
            case .rectangle:
                draw(.rectangle(Self.rect(from: dragStart, to: dragCurrent), colorPreset, shapeWidth))
            case .line:
                draw(.line(dragStart, dragCurrent, colorPreset, shapeWidth))
            case .arrow:
                draw(.arrow(dragStart, dragCurrent, colorPreset, shapeWidth))
            case .select, .pen, .text, .mosaic:
                break
            }
        }
        if tool == .mosaic, let cursorPoint = dragCurrent ?? hoverPoint {
            drawMosaicBrushCursor(at: cursorPoint)
        }
        if tool == .select,
           let selectedMarkIndex,
           history.elements.indices.contains(selectedMarkIndex) {
            drawSelectionOutline(
                for: displayMark(history.elements[selectedMarkIndex], at: selectedMarkIndex)
            )
        }
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(
            NSTrackingArea(
                rect: bounds,
                options: [.activeAlways, .mouseMoved, .mouseEnteredAndExited, .cursorUpdate, .inVisibleRect],
                owner: self
            )
        )
    }

    override func mouseMoved(with event: NSEvent) {
        hoverPoint = clampedPoint(convert(event.locationInWindow, from: nil))
        needsDisplay = true
    }

    override func mouseExited(with event: NSEvent) {
        hoverPoint = nil
        needsDisplay = true
    }

    override func cursorUpdate(with event: NSEvent) {
        if tool == .mosaic {
            NSCursor.crosshair.set()
        } else if tool == .select {
            let point = clampedPoint(convert(event.locationInWindow, from: nil))
            if textMarkIndex(at: point) != nil {
                NSCursor.openHand.set()
            } else {
                NSCursor.arrow.set()
            }
        } else {
            NSCursor.arrow.set()
        }
    }

    override func mouseDown(with event: NSEvent) {
        let point = clampedPoint(convert(event.locationInWindow, from: nil))
        if tool == .select {
            guard let index = textMarkIndex(at: point) else {
                selectedMarkIndex = nil
                selectionDragOriginalMark = nil
                selectionDragPreviewMark = nil
                needsDisplay = true
                return
            }
            selectedMarkIndex = index
            if event.clickCount >= 2 {
                beginTextEditing(markIndex: index)
                return
            }
            dragStart = point
            dragCurrent = point
            selectionDragOriginalMark = history.elements[index]
            selectionDragPreviewMark = nil
            NSCursor.closedHand.set()
            needsDisplay = true
            return
        }
        if tool == .text {
            beginTextEditing(at: point)
            return
        }
        dragStart = point
        dragCurrent = point
        draftPoints = tool == .pen || tool == .mosaic ? [point] : []
    }

    override func mouseDragged(with event: NSEvent) {
        let point = clampedPoint(convert(event.locationInWindow, from: nil))
        if tool == .select,
           let start = dragStart,
           let original = selectionDragOriginalMark {
            dragCurrent = point
            selectionDragPreviewMark = original.translated(
                by: CGSize(width: point.x - start.x, height: point.y - start.y),
                within: imageRect
            )
            needsDisplay = true
            return
        }
        dragCurrent = point
        if tool == .pen || tool == .mosaic {
            if let last = draftPoints.last,
               hypot(point.x - last.x, point.y - last.y) >= 0.5 {
                draftPoints.append(point)
            }
        }
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        mouseDragged(with: event)
        if tool == .select {
            if let selectedMarkIndex,
               let start = dragStart,
               let end = dragCurrent,
               hypot(end.x - start.x, end.y - start.y) >= 1,
               let preview = selectionDragPreviewMark,
               history.replace(at: selectedMarkIndex, with: preview) != nil {
                publishHistoryState()
            }
            dragStart = nil
            dragCurrent = nil
            selectionDragOriginalMark = nil
            selectionDragPreviewMark = nil
            NSCursor.arrow.set()
            needsDisplay = true
            return
        }
        guard let start = dragStart, let end = dragCurrent else { return }
        switch tool {
        case .pen where draftPoints.count > 1:
            append(.pen(draftPoints, colorPreset, penWidth))
        case .rectangle:
            append(.rectangle(Self.rect(from: start, to: end), colorPreset, shapeWidth))
        case .line where Self.hasVisibleLength(from: start, to: end):
            append(.line(start, end, colorPreset, shapeWidth))
        case .arrow:
            if Self.hasVisibleLength(from: start, to: end) {
                append(.arrow(start, end, colorPreset, shapeWidth))
            }
        case .mosaic where !draftPoints.isEmpty:
            append(.mosaic(draftPoints, mosaicBrushDiameter, mosaicIntensity))
        case .select, .text:
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
        selectedMarkIndex = nil
        publishHistoryState()
        needsDisplay = true
    }

    func redo() {
        commitTextEditing()
        guard history.redo() != nil else { return }
        selectedMarkIndex = nil
        publishHistoryState()
        needsDisplay = true
    }

    func clearAnnotations() {
        discardTextEditing()
        guard history.canUndo || history.canRedo else { return }
        history.clear()
        selectedMarkIndex = nil
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

    private func displayMark(_ mark: AnnotationMark, at index: Int) -> AnnotationMark {
        if index == selectedMarkIndex, let selectionDragPreviewMark {
            selectionDragPreviewMark
        } else {
            mark
        }
    }

    private func textMarkIndex(at point: CGPoint) -> Int? {
        for index in history.elements.indices.reversed() {
            guard let rect = history.elements[index].textRect else { continue }
            if rect.insetBy(dx: -7, dy: -6).contains(point) {
                return index
            }
        }
        return nil
    }

    private func drawSelectionOutline(for mark: AnnotationMark) {
        guard let rect = mark.textRect else { return }
        let outline = NSBezierPath(
            roundedRect: rect.insetBy(dx: -7, dy: -6),
            xRadius: 6,
            yRadius: 6
        )
        outline.lineWidth = 1.5
        outline.setLineDash([4, 3], count: 2, phase: 0)
        NSColor.white.withAlphaComponent(0.96).setStroke()
        outline.stroke()
        outline.lineWidth = 1
        CaptureUIColors.accent.setStroke()
        outline.stroke()
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
        case let .pen(points, colorPreset, width):
            colorPreset.color.setStroke()
            guard let first = points.first else { return }
            let path = NSBezierPath()
            path.move(to: first)
            points.dropFirst().forEach { path.line(to: $0) }
            path.lineWidth = width
            path.lineCapStyle = .round
            path.lineJoinStyle = .round
            path.stroke()
        case let .rectangle(rect, colorPreset, width):
            colorPreset.color.setStroke()
            let path = NSBezierPath(roundedRect: rect, xRadius: 2, yRadius: 2)
            path.lineWidth = width
            path.stroke()
        case let .line(start, end, colorPreset, width):
            colorPreset.color.setStroke()
            drawLine(from: start, to: end, width: width)
        case let .arrow(start, end, colorPreset, width):
            colorPreset.color.setStroke()
            drawArrow(from: start, to: end, width: width)
        case let .text(text, rect, colorPreset, backgroundStyle, fontSize):
            drawText(
                text,
                in: rect,
                fontSize: fontSize,
                colorPreset: colorPreset,
                backgroundStyle: backgroundStyle
            )
        case let .mosaic(points, brushDiameter, intensity):
            drawMosaicStroke(points: points, brushDiameter: brushDiameter, intensity: intensity)
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
        case let .pen(points, colorPreset, width):
            colorPreset.color.setStroke()
            guard let first = points.first else { return }
            let path = NSBezierPath()
            path.move(to: convert(first))
            points.dropFirst().forEach { path.line(to: convert($0)) }
            path.lineWidth = max(1, width * min(scaleX, scaleY))
            path.lineCapStyle = .round
            path.lineJoinStyle = .round
            path.stroke()
        case let .rectangle(rect, colorPreset, width):
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
            path.lineWidth = max(1, width * min(scaleX, scaleY))
            path.stroke()
        case let .line(start, end, colorPreset, width):
            colorPreset.color.setStroke()
            drawLine(
                from: convert(start),
                to: convert(end),
                width: max(1, width * min(scaleX, scaleY))
            )
        case let .arrow(start, end, colorPreset, width):
            colorPreset.color.setStroke()
            drawArrow(
                from: convert(start),
                to: convert(end),
                width: max(1, width * min(scaleX, scaleY))
            )
        case let .text(text, rect, colorPreset, backgroundStyle, viewFontSize):
            let fontSize = max(1, viewFontSize * min(scaleX, scaleY))
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
        case let .mosaic(points, brushDiameter, intensity):
            guard !points.isEmpty else { return }
            let viewBounds = mosaicStrokeBounds(
                points: points,
                diameter: brushDiameter
            ).intersection(imageRect)
            guard !viewBounds.isNull, viewBounds.width >= 1, viewBounds.height >= 1 else { return }
            let topLeft = convert(CGPoint(x: viewBounds.minX, y: viewBounds.maxY))
            let bottomRight = convert(CGPoint(x: viewBounds.maxX, y: viewBounds.minY))
            let outputRect = CGRect(
                x: min(topLeft.x, bottomRight.x),
                y: min(topLeft.y, bottomRight.y),
                width: abs(bottomRight.x - topLeft.x),
                height: abs(bottomRight.y - topLeft.y)
            )
            guard let mosaic = pixelatedCrop(for: viewBounds, intensity: intensity) else { return }
            let convertedPoints = points.map(convert)
            NSGraphicsContext.saveGraphicsState()
            clipToMosaicStroke(
                points: convertedPoints,
                diameter: brushDiameter * min(scaleX, scaleY)
            )
            NSGraphicsContext.current?.imageInterpolation = .none
            mosaic.draw(
                in: outputRect,
                from: NSRect(origin: .zero, size: mosaic.size),
                operation: .copy,
                fraction: 1
            )
            NSGraphicsContext.restoreGraphicsState()
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

    private func beginTextEditing(markIndex: Int) {
        guard history.elements.indices.contains(markIndex),
              case let .text(text, rect, preset, background, fontSize) = history.elements[markIndex] else {
            return
        }
        colorPreset = preset
        textBackgroundStyle = background
        textFontSize = fontSize
        tool = .text
        beginTextEditing(
            at: rect.origin,
            existingMarkIndex: markIndex,
            initialText: text,
            existingTextRect: rect
        )
    }

    private func beginTextEditing(at point: CGPoint) {
        beginTextEditing(
            at: point,
            existingMarkIndex: nil,
            initialText: "",
            existingTextRect: nil
        )
    }

    private func beginTextEditing(
        at point: CGPoint,
        existingMarkIndex: Int?,
        initialText: String,
        existingTextRect: CGRect?
    ) {
        commitTextEditing()

        let editor = InlineAnnotationTextView(frame: .zero)
        editor.string = initialText
        editor.font = .systemFont(ofSize: textFontSize, weight: .semibold)
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

        if let existingTextRect {
            editor.frame = CGRect(
                x: existingTextRect.minX - editor.textContainerInset.width,
                y: existingTextRect.minY - editor.textContainerInset.height,
                width: existingTextRect.width + editor.textContainerInset.width * 2,
                height: existingTextRect.height + editor.textContainerInset.height * 2
            )
        } else {
            let width = min(180, max(96, imageRect.width))
            let origin = CGPoint(
                x: min(point.x, imageRect.maxX - width),
                y: min(point.y, imageRect.maxY - 34)
            )
            editor.frame = CGRect(origin: origin, size: CGSize(width: width, height: 34))
        }
        addSubview(editor)
        textEditor = editor
        editingTextMarkIndex = existingMarkIndex
        selectedMarkIndex = existingMarkIndex
        textColorPreset = colorPreset
        editingTextBackgroundStyle = textBackgroundStyle
        editingTextFontSize = textFontSize
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
        let fontSize = editingTextFontSize
        let markIndex = editingTextMarkIndex
        discardTextEditing()
        guard let colorPreset, let backgroundStyle, let fontSize else { return }
        if text.isEmpty {
            if let markIndex, history.remove(at: markIndex) != nil {
                selectedMarkIndex = nil
                publishHistoryState()
                needsDisplay = true
            }
            return
        }
        let mark = AnnotationMark.text(text, textRect, colorPreset, backgroundStyle, fontSize)
        if let markIndex {
            if history.replace(at: markIndex, with: mark) != nil {
                selectedMarkIndex = markIndex
                publishHistoryState()
                needsDisplay = true
            }
        } else {
            append(mark)
        }
    }

    private func discardTextEditing() {
        guard let editor = textEditor else { return }
        textEditor = nil
        textColorPreset = nil
        editingTextBackgroundStyle = nil
        editingTextFontSize = nil
        editingTextMarkIndex = nil
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
        editingTextFontSize = textFontSize
        editor.font = .systemFont(ofSize: textFontSize, weight: .semibold)
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

    private func drawMosaicStroke(
        points: [CGPoint],
        brushDiameter: CGFloat,
        intensity: MosaicIntensityPreset
    ) {
        guard !points.isEmpty else { return }
        let strokeBounds = mosaicStrokeBounds(
            points: points,
            diameter: brushDiameter
        ).intersection(imageRect)
        guard !strokeBounds.isNull,
              let mosaic = pixelatedCrop(for: strokeBounds, intensity: intensity) else { return }
        NSGraphicsContext.saveGraphicsState()
        clipToMosaicStroke(points: points, diameter: brushDiameter)
        NSGraphicsContext.current?.imageInterpolation = .none
        mosaic.draw(
            in: strokeBounds,
            from: NSRect(origin: .zero, size: mosaic.size),
            operation: .copy,
            fraction: 1
        )
        NSGraphicsContext.restoreGraphicsState()
    }

    private func mosaicStrokeBounds(points: [CGPoint], diameter: CGFloat) -> CGRect {
        guard let first = points.first else { return .null }
        var minX = first.x
        var maxX = first.x
        var minY = first.y
        var maxY = first.y
        for point in points.dropFirst() {
            minX = min(minX, point.x)
            maxX = max(maxX, point.x)
            minY = min(minY, point.y)
            maxY = max(maxY, point.y)
        }
        let radius = diameter / 2
        return CGRect(
            x: minX - radius,
            y: minY - radius,
            width: maxX - minX + diameter,
            height: maxY - minY + diameter
        )
    }

    private func clipToMosaicStroke(points: [CGPoint], diameter: CGFloat) {
        guard let context = NSGraphicsContext.current?.cgContext,
              let first = points.first else { return }
        context.beginPath()
        if points.count == 1 {
            context.addEllipse(
                in: CGRect(
                    x: first.x - diameter / 2,
                    y: first.y - diameter / 2,
                    width: diameter,
                    height: diameter
                )
            )
        } else {
            context.move(to: first)
            for point in points.dropFirst() {
                context.addLine(to: point)
            }
            context.setLineWidth(diameter)
            context.setLineCap(.round)
            context.setLineJoin(.round)
            context.replacePathWithStrokedPath()
        }
        context.clip()
    }

    private func drawMosaicBrushCursor(at point: CGPoint) {
        let diameter = mosaicBrushDiameter
        let cursorRect = CGRect(
            x: point.x - diameter / 2,
            y: point.y - diameter / 2,
            width: diameter,
            height: diameter
        )
        let cursor = NSBezierPath(ovalIn: cursorRect)
        NSColor.black.withAlphaComponent(0.72).setStroke()
        cursor.lineWidth = 3
        cursor.stroke()
        NSColor.white.withAlphaComponent(0.95).setStroke()
        cursor.lineWidth = 1.5
        cursor.stroke()
    }

    private func pixelatedCrop(
        for viewRect: CGRect,
        intensity: MosaicIntensityPreset
    ) -> NSImage? {
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

        let blockSize = intensity.viewBlockSize * max(scaleX, scaleY)
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

    var textRect: CGRect? {
        if case let .text(_, rect, _, _, _) = self {
            rect
        } else {
            nil
        }
    }

    func translated(by offset: CGSize, within bounds: CGRect) -> AnnotationMark {
        guard case let .text(text, rect, color, background, fontSize) = self else {
            return self
        }
        let x = min(
            max(rect.minX + offset.width, bounds.minX),
            max(bounds.minX, bounds.maxX - rect.width)
        )
        let y = min(
            max(rect.minY + offset.height, bounds.minY),
            max(bounds.minY, bounds.maxY - rect.height)
        )
        return .text(
            text,
            CGRect(origin: CGPoint(x: x, y: y), size: rect.size),
            color,
            background,
            fontSize
        )
    }
}
