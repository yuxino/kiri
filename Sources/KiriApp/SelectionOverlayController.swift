import AppKit
import KiriCore

enum CaptureSessionAction {
    case copy
    case save
    case pin
    case edit
}

private final class CaptureOverlayWindow: NSWindow {
    var onEscape: (() -> Void)?

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown, event.keyCode == 53 {
            onEscape?()
            return
        }
        super.sendEvent(event)
    }

    override func cancelOperation(_ sender: Any?) {
        onEscape?()
    }
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
        onRecord: @escaping (CGRect, RecordingOptions) -> Void,
        onCancel: @escaping () -> Void
    ) {
        let window = CaptureOverlayWindow(
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
        sessionView.onRecord = { [weak self] region, options in
            self?.close()
            onRecord(region, options)
        }
        window.onEscape = { [weak sessionView] in
            sessionView?.onCancel?()
        }

        window.contentView = sessionView
        self.window = window
        NSApplication.shared.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(sessionView)
        NSCursor.crosshair.set()
    }

    private func close() {
        NSCursor.arrow.set()
        (window as? CaptureOverlayWindow)?.onEscape = nil
        window?.orderOut(nil)
        window?.close()
        window = nil
    }
}

private final class CaptureSessionView: NSView {
    var onComplete: ((CGImage, CaptureSessionAction) -> Void)?
    var onRecord: ((CGRect, RecordingOptions) -> Void)?
    var onCancel: (() -> Void)?

    private let image: CGImage
    private let windowRectsFrontToBack: [CGRect]
    private var phase: CapturePhase = .selecting
    private var dragStart: CGPoint?
    private var selectionInteraction: SelectionInteraction?
    private var interactionMoved = false
    private var selection: CGRect = .null
    private var hoverPoint: CGPoint?
    private var snapCandidate: CGRect?
    private var pendingWindowSelection: CGRect?
    private var annotationCanvas: AnnotationCanvasView?
    private var toolbar: NSVisualEffectView?
    private var toolButtons: [AnnotationTool: CaptureActionButton] = [:]
    private var colorButtons: [AnnotationColorPreset: AnnotationColorSwatchButton] = [:]
    private var colorGroupContainer: CaptureToolGroupView?
    private var strokeOptionsRow: NSStackView?
    private var strokeSizeSlider: NSSlider?
    private var strokeSizeValueLabel: NSTextField?
    private var textOptionsRow: NSStackView?
    private var textBackgroundControl: NSSegmentedControl?
    private var textFontSizeSlider: NSSlider?
    private var textFontSizeValueLabel: NSTextField?
    private var mosaicOptionsRow: NSStackView?
    private var mosaicBrushSizeSlider: NSSlider?
    private var mosaicBrushSizeValueLabel: NSTextField?
    private var mosaicIntensityControl: NSSegmentedControl?
    private var undoButton: CaptureActionButton?
    private var redoButton: CaptureActionButton?
    private var clearAnnotationsItem: NSMenuItem?
    private var recordingOptionsController: RecordingOptionsPopoverController?
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
                drawSelectionHandles()
                if toolbar == nil {
                    drawHint()
                }
            } else if phase == .selecting, snapCandidate != nil {
                drawWindowHint(for: activeRect)
            }
        } else {
            bounds.fill()
        }

        if phase == .selecting {
            if !SelectionGeometry.isValid(selection) || selectionInteraction == .creating {
                drawLoupe()
            }
            if !SelectionGeometry.isValid(selection), snapCandidate == nil {
                drawInitialHint()
            }
        }
    }

    override func layout() {
        super.layout()
        if toolbar != nil {
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
        interactionMoved = false
        if SelectionGeometry.isValid(selection) {
            if let handle = SelectionGeometry.hitTest(point, selection: selection, radius: 10) {
                selectionInteraction = .resizing(handle: handle, original: selection)
                pendingWindowSelection = nil
            } else if selection.contains(point) {
                selectionInteraction = .moving(original: selection)
                pendingWindowSelection = nil
            } else {
                selectionInteraction = .creating
                tearDownAnnotationUI()
                pendingWindowSelection = WindowSnapGeometry.candidate(
                    at: point,
                    windowsFrontToBack: windowRectsFrontToBack,
                    within: bounds
                )
                selection = .null
            }
        } else {
            selectionInteraction = .creating
            tearDownAnnotationUI()
            pendingWindowSelection = snapCandidate
            selection = .null
        }
        hoverPoint = point
        needsDisplay = true
    }

    override func mouseDragged(with event: NSEvent) {
        guard phase == .selecting,
              let interaction = selectionInteraction,
              let dragStart else { return }
        let current = clampedPoint(convert(event.locationInWindow, from: nil))
        let distance = hypot(current.x - dragStart.x, current.y - dragStart.y)
        if distance >= 3 {
            interactionMoved = true
        }
        guard interactionMoved else {
            hoverPoint = current
            needsDisplay = true
            return
        }
        switch interaction {
        case .creating:
            pendingWindowSelection = nil
            snapCandidate = nil
            selection = SelectionGeometry.clamped(
                SelectionGeometry.normalized(from: dragStart, to: current),
                to: bounds
            )
        case let .moving(original):
            selection = SelectionGeometry.moved(
                original,
                by: CGSize(width: current.x - dragStart.x, height: current.y - dragStart.y),
                within: bounds
            )
        case let .resizing(handle, original):
            selection = SelectionGeometry.resized(
                original,
                using: handle,
                to: current,
                within: bounds,
                minimumSide: 16
            )
        }
        hoverPoint = current
        if toolbar != nil {
            layoutAnnotationUI()
        }
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        guard phase == .selecting else { return }
        mouseDragged(with: event)
        let interaction = selectionInteraction
        if interaction == .creating,
           !SelectionGeometry.isValid(selection),
           let pendingWindowSelection {
            selection = pendingWindowSelection
        }
        if !SelectionGeometry.isValid(selection) {
            selection = .null
        }
        dragStart = nil
        selectionInteraction = nil
        interactionMoved = false
        pendingWindowSelection = nil
        snapCandidate = nil
        if SelectionGeometry.isValid(selection) {
            prepareSelectionToolbar()
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
        if !SelectionGeometry.isValid(selection), selectionInteraction == nil {
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

        if phase == .selecting, isReturn, SelectionGeometry.isValid(selection) {
            complete(.copy)
            return
        }

        if phase == .annotating {
            if event.keyCode == 51 || event.keyCode == 117 {
                annotationCanvas?.deleteSelection()
                return
            }
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
        }
        if SelectionGeometry.isValid(selection) {
            let commandModifiers: NSEvent.ModifierFlags = [.command, .control, .option]
            if event.modifierFlags.intersection(commandModifiers).isEmpty {
                switch event.charactersIgnoringModifiers?.lowercased() {
                case "v": useSelect(); return
                case "p": usePen(); return
                case "r": useRectangle(); return
                case "l": useLine(); return
                case "a": useArrow(); return
                case "t": useText(); return
                case "m": useMosaic(); return
                default: break
                }
            }
        }
        super.keyDown(with: event)
    }

    private func prepareSelectionToolbar() {
        guard phase == .selecting,
              let cropped = croppedSelection() else {
            return
        }
        let previous = annotationCanvas
        let canvas = AnnotationCanvasView(image: cropped)
        if let previous {
            canvas.colorPreset = previous.colorPreset
            canvas.textBackgroundStyle = previous.textBackgroundStyle
            canvas.mosaicIntensity = previous.mosaicIntensity
            canvas.penWidth = previous.penWidth
            canvas.shapeWidth = previous.shapeWidth
            canvas.textFontSize = previous.textFontSize
            canvas.mosaicBrushDiameter = previous.mosaicBrushDiameter
        }
        configureAnnotationCanvas(canvas)
        canvas.isHidden = true
        previous?.removeFromSuperview()
        addSubview(canvas, positioned: .below, relativeTo: toolbar)
        annotationCanvas = canvas

        if toolbar == nil {
            let toolbar = makeToolbar()
            addSubview(toolbar)
            self.toolbar = toolbar
        }
        clearToolSelectionForAdjustingRegion()
        updateColorButtons(selected: canvas.colorPreset)
        updateHistoryControls(canUndo: false, canRedo: false)
        layoutAnnotationUI()
        window?.makeFirstResponder(self)
        needsDisplay = true
    }

    private func configureAnnotationCanvas(_ canvas: AnnotationCanvasView) {
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
    }

    private func activateAnnotationTool(_ tool: AnnotationTool) {
        guard SelectionGeometry.isValid(selection) else { return }
        if annotationCanvas == nil {
            prepareSelectionToolbar()
        }
        guard let canvas = annotationCanvas else { return }
        phase = .annotating
        hoverPoint = nil
        snapCandidate = nil
        canvas.isHidden = false
        canvas.tool = tool
        NSCursor.arrow.set()
        layoutAnnotationUI()
        window?.makeFirstResponder(self)
        needsDisplay = true
    }

    private func clearToolSelectionForAdjustingRegion() {
        for (tool, button) in toolButtons {
            button.setToolSelected(tool == .select)
        }
        strokeOptionsRow?.isHidden = true
        textOptionsRow?.isHidden = true
        mosaicOptionsRow?.isHidden = true
        colorGroupContainer?.isHidden = true
    }

    @objc private func returnToSelection() {
        phase = .selecting
        selection = .null
        tearDownAnnotationUI()
        NSCursor.crosshair.set()
        window?.makeFirstResponder(self)
        needsDisplay = true
    }

    private func tearDownAnnotationUI() {
        annotationCanvas?.removeFromSuperview()
        annotationCanvas = nil
        toolbar?.removeFromSuperview()
        toolbar = nil
        toolButtons.removeAll()
        colorButtons.removeAll()
        colorGroupContainer = nil
        strokeOptionsRow = nil
        strokeSizeSlider = nil
        strokeSizeValueLabel = nil
        textOptionsRow = nil
        textBackgroundControl = nil
        textFontSizeSlider = nil
        textFontSizeValueLabel = nil
        mosaicOptionsRow = nil
        mosaicBrushSizeSlider = nil
        mosaicBrushSizeValueLabel = nil
        mosaicIntensityControl = nil
        undoButton = nil
        redoButton = nil
        clearAnnotationsItem = nil
        recordingOptionsController?.close()
        recordingOptionsController = nil
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
        effect.layer?.cornerRadius = 13
        effect.layer?.cornerCurve = .continuous
        effect.layer?.borderWidth = 1
        effect.layer?.borderColor = CaptureUIColors.surfaceBorder.cgColor
        effect.layer?.masksToBounds = true

        let actions = NSStackView()
        actions.orientation = .horizontal
        actions.alignment = .centerY
        actions.spacing = 4
        actions.edgeInsets = NSEdgeInsets(top: 6, left: 7, bottom: 6, right: 7)
        actions.translatesAutoresizingMaskIntoConstraints = false

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
            (.select, "cursorarrow", "Select (V)", "Select and edit annotations (V)", #selector(useSelect)),
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
            toolButtons[tool] = button
            toolGroup.addArrangedSubview(button)
        }
        actions.addArrangedSubview(toolGroup)

        let strokeOptions = makeStrokeContextRow()
        strokeOptions.row.isHidden = true
        strokeOptionsRow = strokeOptions.row
        strokeSizeSlider = strokeOptions.slider
        strokeSizeValueLabel = strokeOptions.valueLabel

        let textOptions = makeTextContextRow()
        textOptions.row.isHidden = true
        textOptionsRow = textOptions.row
        textBackgroundControl = textOptions.backgroundControl
        textFontSizeSlider = textOptions.slider
        textFontSizeValueLabel = textOptions.valueLabel

        let mosaicOptions = makeMosaicContextRow()
        mosaicOptions.row.isHidden = true
        mosaicOptionsRow = mosaicOptions.row
        mosaicBrushSizeSlider = mosaicOptions.slider
        mosaicBrushSizeValueLabel = mosaicOptions.valueLabel
        mosaicIntensityControl = mosaicOptions.intensityControl

        actions.addArrangedSubview(strokeOptions.row)
        actions.addArrangedSubview(textOptions.row)
        actions.addArrangedSubview(mosaicOptions.row)
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
        let colorGroupContainer = CaptureToolGroupView(content: colorGroup)
        self.colorGroupContainer = colorGroupContainer
        actions.addArrangedSubview(colorGroupContainer)
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

        let recordButton = CaptureActionButton(
            symbol: "record.circle",
            label: "Record Region",
            style: .tool,
            hoverHint: "Record region — countdown, audio, and pointer options",
            target: self,
            action: #selector(showRecordingOptions(_:))
        )
        actions.addArrangedSubview(recordButton)

        let doneButton = CaptureActionButton(
            symbol: "checkmark",
            label: "Done (Return)",
            style: .primary,
            hoverHint: "Done — Copy to clipboard · Return",
            target: self,
            action: #selector(finishCapture)
        )
        actions.addArrangedSubview(doneButton)

        let moreButton = actionButton(
            symbol: "ellipsis.circle",
            label: "More Actions",
            hoverHint: "More — Record, save, pin, edit, or clear",
            action: #selector(showMoreActions(_:))
        )
        actions.addArrangedSubview(moreButton)

        effect.addSubview(actions)
        NSLayoutConstraint.activate([
            actions.topAnchor.constraint(equalTo: effect.topAnchor),
            actions.leadingAnchor.constraint(equalTo: effect.leadingAnchor),
            actions.trailingAnchor.constraint(equalTo: effect.trailingAnchor),
            actions.bottomAnchor.constraint(equalTo: effect.bottomAnchor)
        ])
        return effect
    }

    private func makeStrokeContextRow() -> (
        row: NSStackView,
        slider: NSSlider,
        valueLabel: NSTextField
    ) {
        let row = NSStackView()
        row.orientation = .horizontal
        row.alignment = .centerY
        row.spacing = 4
        let size = makeSizeSlider(
            value: 3,
            minimum: 1,
            maximum: 16,
            action: #selector(changeStrokeSize(_:)),
            accessibilityLabel: "Annotation line width"
        )
        row.addArrangedSubview(contextIcon("lineweight", label: "Stroke size"))
        row.addArrangedSubview(size.slider)
        row.addArrangedSubview(size.valueLabel)
        return (row, size.slider, size.valueLabel)
    }

    private func makeTextContextRow() -> (
        row: NSStackView,
        slider: NSSlider,
        valueLabel: NSTextField,
        backgroundControl: NSSegmentedControl
    ) {
        let row = NSStackView()
        row.orientation = .horizontal
        row.alignment = .centerY
        row.spacing = 4
        let size = makeSizeSlider(
            value: 18,
            minimum: 12,
            maximum: 64,
            action: #selector(changeTextFontSize(_:)),
            accessibilityLabel: "Text font size"
        )
        size.slider.onTrackingBegan = { [weak self] in
            self?.annotationCanvas?.beginTextFontSizeAdjustment()
        }
        size.slider.onTrackingEnded = { [weak self] in
            self?.annotationCanvas?.endTextFontSizeAdjustment()
        }
        let backgroundControl = NSSegmentedControl(
            images: [
                compactSymbol("square.dashed", label: "Transparent background"),
                compactSymbol("moon.fill", label: "Dark background"),
                compactSymbol("sun.max.fill", label: "Light background")
            ],
            trackingMode: .selectOne,
            target: self,
            action: #selector(selectTextBackgroundOption(_:))
        )
        configureContextControl(
            backgroundControl,
            widths: [26, 26, 26],
            label: "Text background"
        )
        backgroundControl.setToolTip("Transparent", forSegment: 0)
        backgroundControl.setToolTip("Dark", forSegment: 1)
        backgroundControl.setToolTip("Light", forSegment: 2)

        row.addArrangedSubview(contextIcon("character.textbox", label: "Text options"))
        row.addArrangedSubview(size.slider)
        row.addArrangedSubview(size.valueLabel)
        row.addArrangedSubview(backgroundControl)
        return (row, size.slider, size.valueLabel, backgroundControl)
    }

    private func makeMosaicContextRow() -> (
        row: NSStackView,
        slider: NSSlider,
        valueLabel: NSTextField,
        intensityControl: NSSegmentedControl
    ) {
        let row = NSStackView()
        row.orientation = .horizontal
        row.alignment = .centerY
        row.spacing = 4

        let size = makeSizeSlider(
            value: 36,
            minimum: 12,
            maximum: 120,
            action: #selector(changeMosaicBrushSize(_:)),
            accessibilityLabel: "Mosaic brush size"
        )

        let intensityControl = NSSegmentedControl(
            labels: ["1", "2", "3"],
            trackingMode: .selectOne,
            target: self,
            action: #selector(selectMosaicIntensity(_:))
        )
        configureContextControl(intensityControl, widths: [24, 24, 24], label: "Mosaic strength")
        for (index, preset) in MosaicIntensityPreset.allCases.enumerated() {
            intensityControl.setToolTip(preset.name, forSegment: index)
        }

        row.addArrangedSubview(contextIcon("square.grid.3x3.fill", label: "Mosaic brush"))
        row.addArrangedSubview(size.slider)
        row.addArrangedSubview(size.valueLabel)
        row.addArrangedSubview(intensityControl)
        return (row, size.slider, size.valueLabel, intensityControl)
    }

    private func makeSizeSlider(
        value: Double,
        minimum: Double,
        maximum: Double,
        action: Selector,
        accessibilityLabel: String
    ) -> (slider: CaptureTrackingSlider, valueLabel: NSTextField) {
        let slider = CaptureTrackingSlider(
            value: value,
            minValue: minimum,
            maxValue: maximum,
            target: self,
            action: action
        )
        slider.isContinuous = true
        slider.controlSize = .mini
        slider.setAccessibilityLabel(accessibilityLabel)
        slider.translatesAutoresizingMaskIntoConstraints = false
        slider.widthAnchor.constraint(equalToConstant: 76).isActive = true

        let valueLabel = NSTextField(labelWithString: "\(Int(value))")
        valueLabel.font = .monospacedDigitSystemFont(ofSize: 9, weight: .medium)
        valueLabel.textColor = .secondaryLabelColor
        valueLabel.alignment = .right
        valueLabel.translatesAutoresizingMaskIntoConstraints = false
        valueLabel.widthAnchor.constraint(equalToConstant: 28).isActive = true
        return (slider, valueLabel)
    }

    private func compactSymbol(_ symbol: String, label: String) -> NSImage {
        NSImage(
            systemSymbolName: symbol,
            accessibilityDescription: label
        )?.withSymbolConfiguration(
            NSImage.SymbolConfiguration(pointSize: 10, weight: .medium)
        ) ?? NSImage()
    }

    private func contextIcon(_ symbol: String, label: String) -> NSImageView {
        let icon = NSImageView()
        icon.image = NSImage(systemSymbolName: symbol, accessibilityDescription: label)
        icon.contentTintColor = CaptureUIColors.accent
        icon.symbolConfiguration = .init(pointSize: 11, weight: .semibold)
        icon.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            icon.widthAnchor.constraint(equalToConstant: 16),
            icon.heightAnchor.constraint(equalToConstant: 16)
        ])
        return icon
    }

    private func configureContextControl(
        _ control: NSSegmentedControl,
        widths: [CGFloat],
        label: String
    ) {
        control.segmentStyle = .capsule
        control.controlSize = .mini
        control.font = .systemFont(ofSize: 9, weight: .medium)
        control.setAccessibilityLabel(label)
        for (index, width) in widths.enumerated() {
            control.setWidth(width, forSegment: index)
        }
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
        return button
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
        updateContextualControls(selected: selected)
    }

    private func updateColorButtons(selected: AnnotationColorPreset) {
        for (preset, button) in colorButtons {
            button.setColorSelected(preset == selected)
        }
    }

    private func updateContextualControls(selected: AnnotationTool) {
        strokeOptionsRow?.isHidden = switch selected {
        case .pen, .rectangle, .line, .arrow: false
        case .select, .text, .mosaic: true
        }
        textOptionsRow?.isHidden = selected != .text
        mosaicOptionsRow?.isHidden = selected != .mosaic
        colorGroupContainer?.isHidden = selected == .select || selected == .mosaic

        if let canvas = annotationCanvas {
            updateColorButtons(selected: canvas.colorPreset)
            let isPen = selected == .pen
            strokeSizeSlider?.minValue = 1
            strokeSizeSlider?.maxValue = isPen ? 24 : 16
            strokeSizeSlider?.doubleValue = Double(isPen ? canvas.penWidth : canvas.shapeWidth)
            updateValueLabel(
                strokeSizeValueLabel,
                value: isPen ? canvas.penWidth : canvas.shapeWidth,
                unit: "px"
            )
            textFontSizeSlider?.doubleValue = Double(canvas.textFontSize)
            updateValueLabel(textFontSizeValueLabel, value: canvas.textFontSize, unit: "pt")
            mosaicBrushSizeSlider?.doubleValue = Double(canvas.mosaicBrushDiameter)
            updateValueLabel(
                mosaicBrushSizeValueLabel,
                value: canvas.mosaicBrushDiameter,
                unit: "px"
            )
        }

        if let style = annotationCanvas?.textBackgroundStyle {
            textBackgroundControl?.selectedSegment = switch style {
            case .transparent: 0
            case .dark: 1
            case .light: 2
            }
        }
        if let intensity = annotationCanvas?.mosaicIntensity,
           let index = MosaicIntensityPreset.allCases.firstIndex(of: intensity) {
            mosaicIntensityControl?.selectedSegment = index
        }
        if toolbar != nil {
            layoutAnnotationUI()
        }
    }

    private func updateValueLabel(_ label: NSTextField?, value: CGFloat, unit: String) {
        label?.stringValue = "\(Int(value.rounded()))"
        label?.toolTip = "\(Int(value.rounded())) \(unit)"
    }

    private func updateHistoryControls(canUndo: Bool, canRedo: Bool) {
        undoButton?.setActionEnabled(canUndo)
        redoButton?.setActionEnabled(canRedo)
        clearAnnotationsItem?.isEnabled = canUndo
    }

    @objc private func useSelect() {
        selectTool(.select)
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
        selectTool(.text)
    }

    @objc private func selectAnnotationColor(_ sender: AnnotationColorSwatchButton) {
        annotationCanvas?.colorPreset = sender.preset
        updateColorButtons(selected: sender.preset)
        window?.makeFirstResponder(self)
    }

    @objc private func selectTextBackgroundOption(_ sender: NSSegmentedControl) {
        annotationCanvas?.textBackgroundStyle = switch sender.selectedSegment {
        case 1: .dark
        case 2: .light
        default: .transparent
        }
        useText()
    }

    @objc private func selectMosaicIntensity(_ sender: NSSegmentedControl) {
        guard MosaicIntensityPreset.allCases.indices.contains(sender.selectedSegment) else { return }
        annotationCanvas?.mosaicIntensity = MosaicIntensityPreset.allCases[sender.selectedSegment]
        useMosaic()
    }

    @objc private func changeStrokeSize(_ sender: NSSlider) {
        guard let canvas = annotationCanvas else { return }
        let value = CGFloat(sender.doubleValue.rounded())
        sender.doubleValue = Double(value)
        if canvas.tool == .pen {
            canvas.penWidth = value
        } else {
            canvas.shapeWidth = value
        }
        updateValueLabel(strokeSizeValueLabel, value: value, unit: "px")
        window?.makeFirstResponder(self)
    }

    @objc private func changeTextFontSize(_ sender: NSSlider) {
        let value = CGFloat(sender.doubleValue.rounded())
        sender.doubleValue = Double(value)
        annotationCanvas?.updateTextFontSize(value)
        updateValueLabel(textFontSizeValueLabel, value: value, unit: "pt")
        window?.makeFirstResponder(self)
    }

    @objc private func changeMosaicBrushSize(_ sender: NSSlider) {
        let value = CGFloat(sender.doubleValue.rounded())
        sender.doubleValue = Double(value)
        annotationCanvas?.mosaicBrushDiameter = value
        updateValueLabel(mosaicBrushSizeValueLabel, value: value, unit: "px")
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

    @objc private func showRecordingOptions(_ sender: NSButton) {
        guard !isCompleting, SelectionGeometry.isValid(selection) else { return }
        recordingOptionsController?.close()
        let controller = RecordingOptionsPopoverController(
            options: RecordingPreferences.load(),
            onChange: { options in
                RecordingPreferences.save(options)
            },
            onStart: { [weak self] options in
                self?.recordRegion(options: options)
            }
        )
        recordingOptionsController = controller
        controller.show(relativeTo: sender)
    }

    private func recordRegion(options: RecordingOptions) {
        guard !isCompleting, SelectionGeometry.isValid(selection) else { return }
        isCompleting = true
        RecordingPreferences.save(options)
        recordingOptionsController?.close()
        recordingOptionsController = nil
        window?.orderOut(nil)
        onRecord?(selection.standardized, options.normalized)
    }

    @objc private func pinCapture() {
        complete(.pin)
    }

    @objc private func editCapture() {
        complete(.edit)
    }

    private func selectTool(_ tool: AnnotationTool) {
        if phase == .selecting {
            activateAnnotationTool(tool)
        } else {
            annotationCanvas?.tool = tool
        }
        window?.makeFirstResponder(self)
    }

    private func clampedPoint(_ point: CGPoint) -> CGPoint {
        CGPoint(
            x: min(max(point.x, bounds.minX), bounds.maxX),
            y: min(max(point.y, bounds.minY), bounds.maxY)
        )
    }

    private func updateCursor(at point: CGPoint) {
        guard SelectionGeometry.isValid(selection) else {
            NSCursor.crosshair.set()
            return
        }
        if let handle = SelectionGeometry.hitTest(point, selection: selection, radius: 10) {
            switch handle {
            case .top, .bottom:
                NSCursor.resizeUpDown.set()
            case .left, .right:
                NSCursor.resizeLeftRight.set()
            case .topLeft, .topRight, .bottomLeft, .bottomRight:
                NSCursor.crosshair.set()
            }
        } else if selection.contains(point) {
            if case .moving = selectionInteraction, interactionMoved {
                NSCursor.closedHand.set()
            } else {
                NSCursor.openHand.set()
            }
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

    private func drawSelectionHandles() {
        for handle in SelectionHandle.allCases {
            let center = SelectionGeometry.handlePoint(for: handle, in: selection)
            let outerRect = CGRect(x: center.x - 5, y: center.y - 5, width: 10, height: 10)
            let innerRect = outerRect.insetBy(dx: 2, dy: 2)
            let outer = NSBezierPath(ovalIn: outerRect)
            NSColor.white.setFill()
            outer.fill()
            CaptureUIColors.accent.setFill()
            NSBezierPath(ovalIn: innerRect).fill()
        }
    }

    private func drawHint() {
        let text = selectionInteraction == .creating
            ? "Release to show tools"
            : "Drag handles to resize · Drag inside to move"
        let textValue = text as NSString
        drawHint(textValue, near: selection)
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
