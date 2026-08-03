import AppKit
import KiriCore

enum CaptureSessionAction {
    case copy
    case save
    case pin
    case edit
}

@MainActor
final class SelectionOverlayController {
    private let capture: CapturedDisplay
    private var window: NSWindow?

    init(capture: CapturedDisplay) {
        self.capture = capture
    }

    func present(
        onComplete: @escaping (CGImage, CaptureSessionAction) -> Void,
        onCancel: @escaping () -> Void
    ) {
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
        window.isReleasedWhenClosed = false
        window.acceptsMouseMovedEvents = true
        window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]

        let sessionView = CaptureSessionView(
            image: capture.image,
            windowRectsFrontToBack: capture.windowRectsFrontToBack
        )
        sessionView.onCancel = { [weak self] in
            self?.close()
            onCancel()
        }
        sessionView.onComplete = { [weak self] image, action in
            self?.close()
            onComplete(image, action)
        }

        window.contentView = sessionView
        self.window = window
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(sessionView)
        NSCursor.crosshair.set()
    }

    private func close() {
        NSCursor.arrow.set()
        window?.orderOut(nil)
        window?.close()
        window = nil
    }
}

private final class CaptureSessionView: NSView {
    var onComplete: ((CGImage, CaptureSessionAction) -> Void)?
    var onCancel: (() -> Void)?

    private let image: CGImage
    private let windowRectsFrontToBack: [CGRect]
    private var phase: CapturePhase = .selecting
    private var dragStart: CGPoint?
    private var isSelecting = false
    private var selection: CGRect = .null
    private var hoverPoint: CGPoint?
    private var snapCandidate: CGRect?
    private var pendingWindowSelection: CGRect?
    private var annotationCanvas: AnnotationCanvasView?
    private var toolbar: NSVisualEffectView?
    private var toolButtons: [AnnotationTool: CaptureActionButton] = [:]
    private var colorButtons: [AnnotationColorPreset: AnnotationColorSwatchButton] = [:]
    private var undoButton: CaptureActionButton?
    private var redoButton: CaptureActionButton?
    private var clearAnnotationsItem: NSMenuItem?
    private var toolbarHintLabel: NSTextField?
    private var isCompleting = false

    init(
        image: CGImage,
        windowRectsFrontToBack: [CGRect]
    ) {
        self.image = image
        self.windowRectsFrontToBack = windowRectsFrontToBack
        super.init(frame: .zero)
        wantsLayer = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        window?.makeFirstResponder(self)
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        NSImage(cgImage: image, size: bounds.size).draw(in: bounds)

        let activeRect = SelectionGeometry.isValid(selection) ? selection : snapCandidate
        NSColor.black.withAlphaComponent(activeRect == nil ? 0.25 : 0.48).setFill()
        if let activeRect {
            dimOutside(activeRect)
            let border = NSBezierPath(rect: activeRect)
            border.lineWidth = phase == .annotating ? 4 : 3
            NSColor.white.withAlphaComponent(0.92).setStroke()
            border.stroke()
            border.lineWidth = phase == .annotating ? 2 : 1.5
            CaptureUIColors.accent.setStroke()
            border.stroke()

            if phase == .selecting, SelectionGeometry.isValid(selection) {
                drawDimensions()
                drawHint()
            } else if phase == .selecting, snapCandidate != nil {
                drawWindowHint(for: activeRect)
            }
        } else {
            bounds.fill()
        }

        if phase == .selecting {
            drawLoupe()
            if !SelectionGeometry.isValid(selection), snapCandidate == nil {
                drawInitialHint()
            }
        }
    }

    override func layout() {
        super.layout()
        if phase == .annotating {
            layoutAnnotationUI()
        }
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

    override func mouseDown(with event: NSEvent) {
        guard phase == .selecting else { return }
        let point = clampedPoint(convert(event.locationInWindow, from: nil))
        dragStart = point
        isSelecting = true
        pendingWindowSelection = snapCandidate
        selection = .null
        hoverPoint = point
        needsDisplay = true
    }

    override func mouseDragged(with event: NSEvent) {
        guard phase == .selecting, isSelecting, let dragStart else { return }
        let current = clampedPoint(convert(event.locationInWindow, from: nil))
        if hypot(current.x - dragStart.x, current.y - dragStart.y) >= 3 {
            pendingWindowSelection = nil
            snapCandidate = nil
            selection = SelectionGeometry.clamped(
                SelectionGeometry.normalized(from: dragStart, to: current),
                to: bounds
            )
        }
        hoverPoint = current
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        guard phase == .selecting else { return }
        mouseDragged(with: event)
        let shouldFinishSelection = isSelecting
        if !SelectionGeometry.isValid(selection), let pendingWindowSelection {
            selection = pendingWindowSelection
        }
        if !SelectionGeometry.isValid(selection) {
            selection = .null
        }
        dragStart = nil
        isSelecting = false
        pendingWindowSelection = nil
        snapCandidate = nil
        if SelectionCompletionPolicy.completesOnMouseUp(
            selection: selection,
            interactionStarted: shouldFinishSelection
        ) {
            finishSelection()
            return
        }
        if let hoverPoint {
            updateCursor(at: hoverPoint)
        }
        needsDisplay = true
    }

    override func mouseMoved(with event: NSEvent) {
        guard phase == .selecting else { return }
        let point = clampedPoint(convert(event.locationInWindow, from: nil))
        hoverPoint = point
        if !SelectionGeometry.isValid(selection), !isSelecting {
            snapCandidate = WindowSnapGeometry.candidate(
                at: point,
                windowsFrontToBack: windowRectsFrontToBack,
                within: bounds
            )
        }
        updateCursor(at: point)
        needsDisplay = true
    }

    override func mouseExited(with event: NSEvent) {
        guard phase == .selecting else { return }
        hoverPoint = nil
        if !SelectionGeometry.isValid(selection) {
            snapCandidate = nil
        }
        needsDisplay = true
    }

    override func mouseEntered(with event: NSEvent) {
        mouseMoved(with: event)
    }

    override func rightMouseDown(with event: NSEvent) {
        if phase == .annotating {
            returnToSelection()
        } else {
            onCancel?()
        }
    }

    override func cursorUpdate(with event: NSEvent) {
        guard phase == .selecting else {
            NSCursor.arrow.set()
            return
        }
        updateCursor(at: clampedPoint(convert(event.locationInWindow, from: nil)))
    }

    override func keyDown(with event: NSEvent) {
        let isReturn = event.keyCode == 36 || event.keyCode == 76
        if event.keyCode == 53 {
            onCancel?()
            return
        }

        if phase == .annotating {
            if isReturn {
                complete(.copy)
                return
            }
            if event.modifierFlags.contains(.command) {
                switch event.charactersIgnoringModifiers?.lowercased() {
                case "c":
                    complete(.copy)
                    return
                case "s":
                    complete(.save)
                    return
                case "z":
                    if event.modifierFlags.contains(.shift) {
                        redo()
                    } else {
                        undo()
                    }
                    return
                default:
                    break
                }
            }
            let commandModifiers: NSEvent.ModifierFlags = [.command, .control, .option]
            if event.modifierFlags.intersection(commandModifiers).isEmpty {
                switch event.charactersIgnoringModifiers?.lowercased() {
                case "p":
                    usePen()
                    return
                case "r":
                    useRectangle()
                    return
                case "l":
                    useLine()
                    return
                case "a":
                    useArrow()
                    return
                case "t":
                    useText()
                    return
                case "m":
                    useMosaic()
                    return
                default:
                    break
                }
            }
        }
        super.keyDown(with: event)
    }

    private func finishSelection() {
        beginAnnotation()
    }

    private func beginAnnotation() {
        guard phase == .selecting,
              let cropped = croppedSelection() else {
            return
        }
        phase = .annotating
        hoverPoint = nil
        snapCandidate = nil
        NSCursor.arrow.set()

        let canvas = AnnotationCanvasView(image: cropped)
        canvas.tool = .rectangle
        canvas.onToolChange = { [weak self] tool in
            self?.updateToolButtons(selected: tool)
        }
        canvas.onHistoryChange = { [weak self] canUndo, canRedo in
            self?.updateHistoryControls(canUndo: canUndo, canRedo: canRedo)
        }
        canvas.onConfirmRequested = { [weak self] in
            self?.complete(.copy)
        }
        canvas.onCancelRequested = { [weak self] in
            self?.onCancel?()
        }
        addSubview(canvas)
        annotationCanvas = canvas

        let toolbar = makeToolbar()
        addSubview(toolbar)
        self.toolbar = toolbar
        layoutAnnotationUI()
        updateToolButtons(selected: canvas.tool)
        updateColorButtons(selected: canvas.colorPreset)
        updateHistoryControls(canUndo: false, canRedo: false)
        window?.makeFirstResponder(self)
        needsDisplay = true
    }

    @objc private func returnToSelection() {
        phase = .selecting
        selection = .null
        annotationCanvas?.removeFromSuperview()
        annotationCanvas = nil
        toolbar?.removeFromSuperview()
        toolbar = nil
        toolButtons.removeAll()
        colorButtons.removeAll()
        undoButton = nil
        redoButton = nil
        clearAnnotationsItem = nil
        toolbarHintLabel = nil
        NSCursor.crosshair.set()
        window?.makeFirstResponder(self)
        needsDisplay = true
    }

    private func croppedSelection() -> CGImage? {
        let pixelRect = SelectionGeometry.pixelRect(
            forTopLeftRect: selection,
            canvasSize: bounds.size,
            imageSize: CGSize(width: image.width, height: image.height)
        ).intersection(
            CGRect(x: 0, y: 0, width: image.width, height: image.height)
        )
        return image.cropping(to: pixelRect)
    }

    private func complete(_ action: CaptureSessionAction) {
        guard !isCompleting, let canvas = annotationCanvas else { return }
        isCompleting = true

        // Remove the full-screen overlay before doing pixel work so Done feels
        // immediate even for a Retina-sized capture with several annotations.
        window?.orderOut(nil)
        Task { @MainActor [weak self, canvas] in
            await Task.yield()
            guard let self else { return }
            guard let rendered = canvas.renderedImage() else {
                isCompleting = false
                window?.makeKeyAndOrderFront(nil)
                return
            }
            onComplete?(rendered, action)
        }
    }

    private func makeToolbar() -> NSVisualEffectView {
        let effect = NSVisualEffectView()
        effect.material = .popover
        effect.blendingMode = .withinWindow
        effect.state = .active
        effect.wantsLayer = true
        effect.layer?.cornerRadius = 15
        effect.layer?.cornerCurve = .continuous
        effect.layer?.borderWidth = 1
        effect.layer?.borderColor = CaptureUIColors.surfaceBorder.cgColor
        effect.layer?.masksToBounds = true

        let content = NSStackView()
        content.orientation = .vertical
        content.alignment = .centerX
        content.spacing = 3
        content.edgeInsets = NSEdgeInsets(top: 7, left: 8, bottom: 6, right: 8)
        content.translatesAutoresizingMaskIntoConstraints = false

        let actions = NSStackView()
        actions.orientation = .horizontal
        actions.alignment = .centerY
        actions.spacing = 5

        actions.addArrangedSubview(CaptureSparkleView())
        let cancelButton = actionButton(
            symbol: "xmark",
            label: "Cancel (Esc)",
            hoverHint: "Cancel capture · Esc",
            action: #selector(cancelCapture)
        )
        actions.addArrangedSubview(cancelButton)
        actions.addArrangedSubview(separator())

        let toolGroup = NSStackView()
        toolGroup.orientation = .horizontal
        toolGroup.alignment = .centerY
        toolGroup.spacing = 1

        let tools: [(AnnotationTool, String, String, String, Selector)] = [
            (.pen, "pencil.tip", "Pen (P)", "Pen (P) — Draw freehand", #selector(usePen)),
            (
                .rectangle,
                "rectangle.dashed",
                "Rectangle (R)",
                "Rectangle (R) — Draw a box",
                #selector(useRectangle)
            ),
            (.line, "line.diagonal", "Line (L)", "Line (L) — Connect two points", #selector(useLine)),
            (.arrow, "arrow.up.right", "Arrow (A)", "Arrow (A) — Point something out", #selector(useArrow)),
            (
                .text,
                "textformat",
                "Text (T)",
                "Text (T) — Click the image, type, then press Return",
                #selector(useText)
            ),
            (.mosaic, "square.grid.3x3.fill", "Mosaic (M)", "Mosaic (M) — Hide sensitive content", #selector(useMosaic))
        ]
        for (tool, symbol, help, hoverHint, action) in tools {
            let button = CaptureActionButton(
                symbol: symbol,
                label: help,
                style: .tool,
                hoverHint: hoverHint,
                target: self,
                action: action
            )
            connectToolbarHint(to: button)
            toolButtons[tool] = button
            toolGroup.addArrangedSubview(button)
        }
        actions.addArrangedSubview(CaptureToolGroupView(content: toolGroup))
        actions.addArrangedSubview(separator())

        let colorGroup = NSStackView()
        colorGroup.orientation = .horizontal
        colorGroup.alignment = .centerY
        colorGroup.spacing = 1
        for preset in AnnotationColorPreset.allCases {
            let button = AnnotationColorSwatchButton(
                preset: preset,
                target: self,
                action: #selector(selectAnnotationColor(_:))
            )
            colorButtons[preset] = button
            colorGroup.addArrangedSubview(button)
        }
        actions.addArrangedSubview(CaptureToolGroupView(content: colorGroup))
        actions.addArrangedSubview(separator())

        let undoButton = actionButton(
            symbol: "arrow.uturn.backward",
            label: "Undo (⌘Z)",
            hoverHint: "Undo the last annotation · ⌘Z",
            action: #selector(undo)
        )
        undoButton.setActionEnabled(false)
        self.undoButton = undoButton
        actions.addArrangedSubview(undoButton)

        let redoButton = actionButton(
            symbol: "arrow.uturn.forward",
            label: "Redo (⇧⌘Z)",
            hoverHint: "Redo the last annotation · ⇧⌘Z",
            action: #selector(redo)
        )
        redoButton.setActionEnabled(false)
        self.redoButton = redoButton
        actions.addArrangedSubview(redoButton)
        actions.addArrangedSubview(separator())
        let doneButton = CaptureActionButton(
            symbol: "checkmark.circle.fill",
            label: "Done",
            style: .primary,
            showsTitle: true,
            hoverHint: "Done — Copy to clipboard · Return",
            target: self,
            action: #selector(finishCapture)
        )
        connectToolbarHint(to: doneButton)
        actions.addArrangedSubview(doneButton)

        let moreButton = actionButton(
            symbol: "ellipsis.circle",
            label: "More Actions",
            hoverHint: "More — Save, pin, edit, or clear",
            action: #selector(showMoreActions(_:))
        )
        actions.addArrangedSubview(moreButton)

        let hint = NSTextField(labelWithString: Self.defaultToolbarHint)
        hint.font = .systemFont(ofSize: 10, weight: .medium)
        hint.textColor = .secondaryLabelColor
        hint.alignment = .center
        hint.lineBreakMode = .byTruncatingTail
        toolbarHintLabel = hint

        content.addArrangedSubview(actions)
        content.addArrangedSubview(hint)
        effect.addSubview(content)
        NSLayoutConstraint.activate([
            content.topAnchor.constraint(equalTo: effect.topAnchor),
            content.leadingAnchor.constraint(equalTo: effect.leadingAnchor),
            content.trailingAnchor.constraint(equalTo: effect.trailingAnchor),
            content.bottomAnchor.constraint(equalTo: effect.bottomAnchor),
            hint.widthAnchor.constraint(equalTo: actions.widthAnchor)
        ])
        return effect
    }

    private func actionButton(
        symbol: String,
        label: String,
        hoverHint: String? = nil,
        action: Selector
    ) -> CaptureActionButton {
        let button = CaptureActionButton(
            symbol: symbol,
            label: label,
            style: .secondary,
            hoverHint: hoverHint,
            target: self,
            action: action
        )
        connectToolbarHint(to: button)
        return button
    }

    private func connectToolbarHint(to button: CaptureActionButton) {
        button.onHoverHintChange = { [weak self] hint in
            self?.toolbarHintLabel?.stringValue = hint ?? Self.defaultToolbarHint
        }
    }

    private func separator() -> NSView {
        CaptureDividerView(height: 24)
    }

    private func layoutAnnotationUI() {
        guard let annotationCanvas, let toolbar else { return }
        annotationCanvas.frame = selection

        let toolbarSize = toolbar.fittingSize
        let width = max(1, toolbarSize.width)
        let height = max(42, toolbarSize.height)
        var origin = CGPoint(
            x: selection.midX - width / 2,
            y: selection.maxY + 10
        )
        origin.x = min(max(origin.x, 8), max(8, bounds.maxX - width - 8))
        if origin.y + height > bounds.maxY - 8 {
            origin.y = selection.minY - height - 10
        }
        origin.y = min(max(origin.y, 8), max(8, bounds.maxY - height - 8))
        toolbar.frame = CGRect(origin: origin, size: CGSize(width: width, height: height))
    }

    private func updateToolButtons(selected: AnnotationTool) {
        for (tool, button) in toolButtons {
            button.setToolSelected(tool == selected)
        }
    }

    private func updateColorButtons(selected: AnnotationColorPreset) {
        for (preset, button) in colorButtons {
            button.setColorSelected(preset == selected)
        }
    }

    private func updateHistoryControls(canUndo: Bool, canRedo: Bool) {
        undoButton?.setActionEnabled(canUndo)
        redoButton?.setActionEnabled(canRedo)
        clearAnnotationsItem?.isEnabled = canUndo
    }

    @objc private func usePen() {
        selectTool(.pen)
    }

    @objc private func useRectangle() {
        selectTool(.rectangle)
    }

    @objc private func useLine() {
        selectTool(.line)
    }

    @objc private func useArrow() {
        selectTool(.arrow)
    }

    @objc private func useText() {
        annotationCanvas?.beginTextPlacement()
        window?.makeFirstResponder(self)
    }

    @objc private func selectAnnotationColor(_ sender: AnnotationColorSwatchButton) {
        annotationCanvas?.colorPreset = sender.preset
        updateColorButtons(selected: sender.preset)
        window?.makeFirstResponder(self)
    }

    @objc private func useMosaic() {
        selectTool(.mosaic)
    }

    @objc private func undo() {
        annotationCanvas?.undo()
        window?.makeFirstResponder(self)
    }

    @objc private func redo() {
        annotationCanvas?.redo()
        window?.makeFirstResponder(self)
    }

    @objc private func finishCapture() {
        complete(.copy)
    }

    @objc private func cancelCapture() {
        onCancel?()
    }

    @objc private func showMoreActions(_ sender: NSButton) {
        let menu = NSMenu()
        menu.autoenablesItems = false
        menu.addItem(
            menuItem("Reselect Region", symbol: "crop", action: #selector(returnToSelection))
        )
        menu.addItem(.separator())
        menu.addItem(
            menuItem("Save As…", symbol: "square.and.arrow.down", action: #selector(saveCapture))
        )
        menu.addItem(
            menuItem("Pin on Screen", symbol: "pin", action: #selector(pinCapture))
        )
        menu.addItem(
            menuItem("Open in Editor", symbol: "slider.horizontal.3", action: #selector(editCapture))
        )
        menu.addItem(.separator())
        let clearItem = menuItem(
            "Clear Annotations",
            symbol: "trash",
            action: #selector(clearAnnotations)
        )
        clearItem.isEnabled = undoButton?.isEnabled == true
        clearAnnotationsItem = clearItem
        menu.addItem(clearItem)
        menu.popUp(
            positioning: nil,
            at: CGPoint(x: sender.bounds.minX, y: sender.bounds.maxY + 4),
            in: sender
        )
        clearAnnotationsItem = nil
        window?.makeFirstResponder(self)
    }

    private func menuItem(_ title: String, symbol: String, action: Selector) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: "")
        item.target = self
        item.image = NSImage(systemSymbolName: symbol, accessibilityDescription: title)
        return item
    }

    @objc private func clearAnnotations() {
        annotationCanvas?.clearAnnotations()
        window?.makeFirstResponder(self)
    }

    @objc private func saveCapture() {
        complete(.save)
    }

    @objc private func pinCapture() {
        complete(.pin)
    }

    @objc private func editCapture() {
        complete(.edit)
    }

    private func selectTool(_ tool: AnnotationTool) {
        annotationCanvas?.tool = tool
        window?.makeFirstResponder(self)
    }

    private func clampedPoint(_ point: CGPoint) -> CGPoint {
        CGPoint(
            x: min(max(point.x, bounds.minX), bounds.maxX),
            y: min(max(point.y, bounds.minY), bounds.maxY)
        )
    }

    private func updateCursor(at _: CGPoint) {
        NSCursor.crosshair.set()
    }

    private func dimOutside(_ rect: CGRect) {
        NSRect(x: 0, y: 0, width: bounds.width, height: rect.minY).fill()
        NSRect(x: 0, y: rect.maxY, width: bounds.width, height: bounds.height - rect.maxY).fill()
        NSRect(x: 0, y: rect.minY, width: rect.minX, height: rect.height).fill()
        NSRect(
            x: rect.maxX,
            y: rect.minY,
            width: bounds.width - rect.maxX,
            height: rect.height
        ).fill()
    }

    private func drawDimensions() {
        let text = "\(Int(selection.width)) × \(Int(selection.height))" as NSString
        let attributes = labelAttributes(monospaced: true)
        let textSize = text.size(withAttributes: attributes)
        let badgeSize = CGSize(width: textSize.width + 14, height: textSize.height + 8)
        var origin = CGPoint(x: selection.minX, y: selection.minY - badgeSize.height - 6)
        origin.x = min(max(origin.x, 6), bounds.maxX - badgeSize.width - 6)
        if origin.y < 6 {
            origin.y = selection.minY + 6
        }
        drawBadge(text, origin: origin, size: badgeSize, attributes: attributes)
    }

    private func drawHint() {
        let text = "Release to edit · Esc to cancel" as NSString
        drawHint(text, near: selection)
    }

    private func drawWindowHint(for rect: CGRect) {
        let text = "Click to select window · Drag for a region" as NSString
        drawHint(text, near: rect)
    }

    private func drawInitialHint() {
        let text = "Drag to capture   ·   Click a window   ·   Esc to cancel" as NSString
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 12, weight: .medium),
            .foregroundColor: NSColor.white
        ]
        let textSize = text.size(withAttributes: attributes)
        let padding = CGSize(width: 15, height: 9)
        let pill = CGRect(
            x: bounds.midX - (textSize.width + padding.width * 2) / 2,
            y: bounds.maxY - textSize.height - padding.height * 2 - 28,
            width: textSize.width + padding.width * 2,
            height: textSize.height + padding.height * 2
        )
        let path = NSBezierPath(roundedRect: pill, xRadius: pill.height / 2, yRadius: pill.height / 2)
        NSColor.black.withAlphaComponent(0.72).setFill()
        path.fill()
        text.draw(
            at: CGPoint(x: pill.minX + padding.width, y: pill.minY + padding.height),
            withAttributes: attributes
        )
    }

    private static let defaultToolbarHint = "Return  Copy   ·   Esc  Cancel   ·   ⌘Z  Undo"

    private func drawHint(_ text: NSString, near rect: CGRect) {
        let attributes = labelAttributes(monospaced: false)
        let textSize = text.size(withAttributes: attributes)
        let badgeSize = CGSize(width: textSize.width + 16, height: textSize.height + 9)
        var origin = CGPoint(x: rect.maxX - badgeSize.width, y: rect.maxY + 7)
        origin.x = min(max(origin.x, 6), bounds.maxX - badgeSize.width - 6)
        if origin.y + badgeSize.height > bounds.maxY - 6 {
            origin.y = rect.maxY - badgeSize.height - 7
        }
        drawBadge(text, origin: origin, size: badgeSize, attributes: attributes)
    }

    private func labelAttributes(monospaced: Bool) -> [NSAttributedString.Key: Any] {
        [
            .font: monospaced
                ? NSFont.monospacedSystemFont(ofSize: 11, weight: .medium)
                : NSFont.systemFont(ofSize: 11, weight: .medium),
            .foregroundColor: NSColor.white
        ]
    }

    private func drawBadge(
        _ text: NSString,
        origin: CGPoint,
        size: CGSize,
        attributes: [NSAttributedString.Key: Any]
    ) {
        let rect = CGRect(origin: origin, size: size)
        let background = NSBezierPath(
            roundedRect: rect,
            xRadius: rect.height / 2,
            yRadius: rect.height / 2
        )
        NSColor.black.withAlphaComponent(0.76).setFill()
        background.fill()
        NSColor.white.withAlphaComponent(0.16).setStroke()
        background.lineWidth = 1
        background.stroke()

        let textSize = text.size(withAttributes: attributes)
        text.draw(
            at: CGPoint(
                x: rect.midX - textSize.width / 2,
                y: rect.midY - textSize.height / 2
            ),
            withAttributes: attributes
        )
    }

    private func drawLoupe() {
        guard let hoverPoint else { return }
        let scaleX = CGFloat(image.width) / bounds.width
        let scaleY = CGFloat(image.height) / bounds.height
        let center = CGPoint(x: hoverPoint.x * scaleX, y: hoverPoint.y * scaleY)
        let sourceRect = CGRect(
            x: center.x - 5.5,
            y: center.y - 5.5,
            width: 11,
            height: 11
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

private enum CapturePhase {
    case selecting
    case annotating
}

private enum SelectionInteraction: Equatable {
    case creating
    case moving(original: CGRect)
    case resizing(handle: SelectionHandle, original: CGRect)
}
