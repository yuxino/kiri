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
    private var interaction: SelectionInteraction?
    private var selection: CGRect = .null
    private var hoverPoint: CGPoint?
    private var snapCandidate: CGRect?
    private var pendingWindowSelection: CGRect?
    private var annotationCanvas: AnnotationCanvasView?
    private var toolbar: NSVisualEffectView?
    private var toolButtons: [AnnotationTool: CaptureActionButton] = [:]
    private var undoButton: CaptureActionButton?
    private var redoButton: CaptureActionButton?
    private var clearAnnotationsItem: NSMenuItem?

    init(image: CGImage, windowRectsFrontToBack: [CGRect]) {
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
            border.lineWidth = phase == .annotating ? 2 : 1.5
            NSColor(
                calibratedRed: phase == .annotating ? 0.47 : 0.62,
                green: phase == .annotating ? 0.41 : 0.62,
                blue: phase == .annotating ? 0.86 : 0.91,
                alpha: 1
            ).setStroke()
            border.stroke()

            if phase == .selecting, SelectionGeometry.isValid(selection) {
                drawHandles()
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
        if event.clickCount >= 2, SelectionGeometry.isValid(selection), selection.contains(point) {
            beginAnnotation()
            return
        }

        dragStart = point
        pendingWindowSelection = nil
        if let handle = SelectionGeometry.hitTest(point, selection: selection, radius: 10) {
            interaction = .resizing(handle: handle, original: selection)
        } else if SelectionGeometry.isValid(selection), selection.contains(point) {
            interaction = .moving(original: selection)
            NSCursor.closedHand.set()
        } else {
            pendingWindowSelection = snapCandidate
            interaction = .creating
            selection = .null
        }
        hoverPoint = point
        needsDisplay = true
    }

    override func mouseDragged(with event: NSEvent) {
        guard phase == .selecting, let dragStart, let interaction else { return }
        let current = clampedPoint(convert(event.locationInWindow, from: nil))
        switch interaction {
        case .creating:
            if hypot(current.x - dragStart.x, current.y - dragStart.y) >= 3 {
                pendingWindowSelection = nil
                snapCandidate = nil
                selection = SelectionGeometry.clamped(
                    SelectionGeometry.normalized(from: dragStart, to: current),
                    to: bounds
                )
            }
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
        guard phase == .selecting else { return }
        mouseDragged(with: event)
        if !SelectionGeometry.isValid(selection), let pendingWindowSelection {
            selection = pendingWindowSelection
        }
        if !SelectionGeometry.isValid(selection) {
            selection = .null
        }
        dragStart = nil
        interaction = nil
        pendingWindowSelection = nil
        snapCandidate = nil
        if let hoverPoint {
            updateCursor(at: hoverPoint)
        }
        needsDisplay = true
    }

    override func mouseMoved(with event: NSEvent) {
        guard phase == .selecting else { return }
        let point = clampedPoint(convert(event.locationInWindow, from: nil))
        hoverPoint = point
        if !SelectionGeometry.isValid(selection), interaction == nil {
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
            if phase == .annotating {
                returnToSelection()
            } else {
                onCancel?()
            }
            return
        }

        if phase == .selecting, isReturn, SelectionGeometry.isValid(selection) {
            beginAnnotation()
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
        addSubview(canvas)
        annotationCanvas = canvas

        let toolbar = makeToolbar()
        addSubview(toolbar)
        self.toolbar = toolbar
        layoutAnnotationUI()
        updateToolButtons(selected: canvas.tool)
        updateHistoryControls(canUndo: false, canRedo: false)
        window?.makeFirstResponder(self)
        needsDisplay = true
    }

    private func returnToSelection() {
        phase = .selecting
        annotationCanvas?.removeFromSuperview()
        annotationCanvas = nil
        toolbar?.removeFromSuperview()
        toolbar = nil
        toolButtons.removeAll()
        undoButton = nil
        redoButton = nil
        clearAnnotationsItem = nil
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
        guard let rendered = annotationCanvas?.renderedImage() else { return }
        onComplete?(rendered, action)
    }

    private func makeToolbar() -> NSVisualEffectView {
        let effect = NSVisualEffectView()
        effect.material = .popover
        effect.blendingMode = .withinWindow
        effect.state = .active
        effect.wantsLayer = true
        effect.layer?.cornerRadius = 12
        effect.layer?.borderWidth = 1
        effect.layer?.borderColor = CaptureUIColors.surfaceBorder.cgColor
        effect.layer?.masksToBounds = true

        let stack = NSStackView()
        stack.orientation = .horizontal
        stack.alignment = .centerY
        stack.spacing = 3
        stack.edgeInsets = NSEdgeInsets(top: 6, left: 6, bottom: 6, right: 6)
        stack.translatesAutoresizingMaskIntoConstraints = false

        let tools: [(AnnotationTool, String, String, Selector)] = [
            (.pen, "pencil.tip", "Pen (P)", #selector(usePen)),
            (.rectangle, "rectangle", "Rectangle (R)", #selector(useRectangle)),
            (.arrow, "arrow.up.right", "Arrow (A)", #selector(useArrow)),
            (.text, "character.textbox", "Text (T)", #selector(useText)),
            (.mosaic, "square.grid.3x3", "Mosaic (M)", #selector(useMosaic))
        ]
        for (tool, symbol, help, action) in tools {
            let button = CaptureActionButton(
                symbol: symbol,
                label: help,
                style: .tool,
                target: self,
                action: action
            )
            toolButtons[tool] = button
            stack.addArrangedSubview(button)
        }
        stack.addArrangedSubview(separator())
        let undoButton = actionButton(
            symbol: "arrow.uturn.backward",
            label: "Undo (⌘Z)",
            action: #selector(undo)
        )
        undoButton.setActionEnabled(false)
        self.undoButton = undoButton
        stack.addArrangedSubview(undoButton)

        let redoButton = actionButton(
            symbol: "arrow.uturn.forward",
            label: "Redo (⇧⌘Z)",
            action: #selector(redo)
        )
        redoButton.setActionEnabled(false)
        self.redoButton = redoButton
        stack.addArrangedSubview(redoButton)
        stack.addArrangedSubview(separator())
        stack.addArrangedSubview(
            CaptureActionButton(
                symbol: "checkmark",
                label: "Done",
                style: .primary,
                showsTitle: true,
                target: self,
                action: #selector(finishCapture)
            )
        )
        stack.addArrangedSubview(
            actionButton(
                symbol: "ellipsis",
                label: "More Actions",
                action: #selector(showMoreActions(_:))
            )
        )

        effect.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: effect.topAnchor),
            stack.leadingAnchor.constraint(equalTo: effect.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: effect.trailingAnchor),
            stack.bottomAnchor.constraint(equalTo: effect.bottomAnchor)
        ])
        return effect
    }

    private func actionButton(
        symbol: String,
        label: String,
        action: Selector
    ) -> CaptureActionButton {
        CaptureActionButton(
            symbol: symbol,
            label: label,
            style: .secondary,
            target: self,
            action: action
        )
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

    @objc private func useArrow() {
        selectTool(.arrow)
    }

    @objc private func useText() {
        guard let text = AnnotationTextPrompt.requestText() else {
            window?.makeFirstResponder(self)
            return
        }
        annotationCanvas?.beginTextPlacement(text)
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

    @objc private func showMoreActions(_ sender: NSButton) {
        let menu = NSMenu()
        menu.autoenablesItems = false
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

    private func updateCursor(at point: CGPoint) {
        guard interaction == nil else { return }
        if let handle = SelectionGeometry.hitTest(point, selection: selection, radius: 10) {
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
        let attributes = labelAttributes(monospaced: true)
        let size = text.size(withAttributes: attributes)
        var origin = CGPoint(x: selection.minX, y: selection.minY - size.height - 5)
        if origin.y < 4 {
            origin.y = selection.minY + 5
        }
        text.draw(at: origin, withAttributes: attributes)
    }

    private func drawHint() {
        let text = "Drag to adjust · Double-click or Return to annotate · Esc to cancel" as NSString
        drawHint(text, near: selection)
    }

    private func drawWindowHint(for rect: CGRect) {
        let text = "Click to select window · Drag for a region" as NSString
        drawHint(text, near: rect)
    }

    private func drawHint(_ text: NSString, near rect: CGRect) {
        let attributes = labelAttributes(monospaced: false)
        let size = text.size(withAttributes: attributes)
        var origin = CGPoint(x: rect.maxX - size.width, y: rect.maxY + 6)
        origin.x = min(max(origin.x, 6), bounds.maxX - size.width - 6)
        if origin.y + size.height > bounds.maxY - 6 {
            origin.y = rect.maxY - size.height - 6
        }
        text.draw(at: origin, withAttributes: attributes)
    }

    private func labelAttributes(monospaced: Bool) -> [NSAttributedString.Key: Any] {
        [
            .font: monospaced
                ? NSFont.monospacedSystemFont(ofSize: 11, weight: .medium)
                : NSFont.systemFont(ofSize: 11, weight: .medium),
            .foregroundColor: NSColor.white,
            .backgroundColor: NSColor.black.withAlphaComponent(0.72)
        ]
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
