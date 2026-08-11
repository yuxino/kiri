import AppKit

@MainActor
final class EditorWindowController: NSWindowController, NSWindowDelegate {
    typealias Completion = (CGImage, Bool, URL?) -> Void

    private let completion: Completion
    private var onClose: (() -> Void)?
    private let canvas: AnnotationCanvasView
    private var toolButtons: [AnnotationTool: CaptureActionButton] = [:]
    private var colorButtons: [AnnotationColorPreset: AnnotationColorSwatchButton] = [:]
    private var colorGroupContainer: CaptureToolGroupView?
    private var sizeControlContainer: NSStackView?
    private var sizeControlTitle: NSTextField?
    private var sizeSlider: NSSlider?
    private var sizeValueLabel: NSTextField?
    private var textBackgroundButton: CaptureActionButton?
    private var mosaicIntensityButton: CaptureActionButton?
    private var undoButton: CaptureActionButton?
    private var redoButton: CaptureActionButton?
    private var clearButton: CaptureActionButton?

    init(
        image: CGImage,
        completion: @escaping Completion,
        onClose: @escaping () -> Void
    ) {
        self.completion = completion
        self.onClose = onClose
        canvas = AnnotationCanvasView(image: image)
        let window = NSWindow(
            contentRect: CGRect(x: 0, y: 0, width: 880, height: 620),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = L10n.text("Kiri Editor")
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.backgroundColor = NSColor(calibratedWhite: 0.075, alpha: 1)
        window.minSize = CGSize(width: 860, height: 520)
        window.center()
        super.init(window: window)
        window.delegate = self
        window.contentViewController = makeContentViewController()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    func windowWillClose(_ notification: Notification) {
        let callback = onClose
        onClose = nil
        callback?()
    }

    private func makeContentViewController() -> NSViewController {
        let controller = NSViewController()
        let root = NSView()
        root.translatesAutoresizingMaskIntoConstraints = false
        root.wantsLayer = true
        root.layer?.backgroundColor = NSColor(calibratedWhite: 0.075, alpha: 1).cgColor

        let toolbarSurface = NSVisualEffectView()
        toolbarSurface.material = .underWindowBackground
        toolbarSurface.blendingMode = .withinWindow
        toolbarSurface.state = .active
        toolbarSurface.translatesAutoresizingMaskIntoConstraints = false
        toolbarSurface.wantsLayer = true
        toolbarSurface.layer?.borderWidth = 0

        let toolbar = NSStackView()
        toolbar.orientation = .horizontal
        toolbar.alignment = .centerY
        toolbar.spacing = 5
        toolbar.edgeInsets = NSEdgeInsets(top: 10, left: 12, bottom: 10, right: 12)
        toolbar.translatesAutoresizingMaskIntoConstraints = false

        let tools: [(AnnotationTool, String, String, String, Selector)] = [
            (.select, "cursorarrow", L10n.text("Select (V)"), "v", #selector(useSelect)),
            (.pen, "pencil.tip", L10n.text("Pen (P)"), "p", #selector(usePen)),
            (.rectangle, "rectangle.dashed", L10n.text("Rectangle (R)"), "r", #selector(useRectangle)),
            (.line, "line.diagonal", L10n.text("Line (L)"), "l", #selector(useLine)),
            (.arrow, "arrow.up.right", L10n.text("Arrow (A)"), "a", #selector(useArrow)),
            (.text, "textformat", L10n.text("Text (T)"), "t", #selector(useText)),
            (.mosaic, "square.grid.3x3.fill", L10n.text("Mosaic (M)"), "m", #selector(useMosaic))
        ]
        for (tool, symbol, label, keyEquivalent, action) in tools {
            let button = CaptureActionButton(
                symbol: symbol,
                label: label,
                style: .tool,
                target: self,
                action: action
            )
            button.keyEquivalent = keyEquivalent
            button.keyEquivalentModifierMask = []
            toolButtons[tool] = button
            toolbar.addArrangedSubview(button)
        }
        let sizeControl = makeSizeControl()
        sizeControlContainer = sizeControl.container
        sizeControlTitle = sizeControl.title
        sizeSlider = sizeControl.slider
        sizeValueLabel = sizeControl.valueLabel
        toolbar.addArrangedSubview(sizeControl.container)
        toolbar.addArrangedSubview(CaptureDividerView(height: 24))

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
        toolbar.addArrangedSubview(colorGroupContainer)
        let textBackgroundButton = CaptureActionButton(
            symbol: "character.textbox",
            label: L10n.text("Text Background"),
            style: .tool,
            target: self,
            action: #selector(showTextBackgroundMenu(_:))
        )
        self.textBackgroundButton = textBackgroundButton
        toolbar.addArrangedSubview(textBackgroundButton)
        let mosaicIntensityButton = CaptureActionButton(
            symbol: "square.grid.3x3.fill",
            label: L10n.text("Mosaic Strength"),
            style: .tool,
            target: self,
            action: #selector(showMosaicIntensityMenu(_:))
        )
        self.mosaicIntensityButton = mosaicIntensityButton
        toolbar.addArrangedSubview(mosaicIntensityButton)
        toolbar.addArrangedSubview(CaptureDividerView(height: 24))

        let undoButton = historyButton(
            symbol: "arrow.uturn.backward",
            label: L10n.text("Undo (⌘Z)"),
            action: #selector(undo),
            keyEquivalent: "z",
            modifiers: [.command]
        )
        self.undoButton = undoButton
        toolbar.addArrangedSubview(undoButton)

        let redoButton = historyButton(
            symbol: "arrow.uturn.forward",
            label: L10n.text("Redo (⇧⌘Z)"),
            action: #selector(redo),
            keyEquivalent: "z",
            modifiers: [.command, .shift]
        )
        self.redoButton = redoButton
        toolbar.addArrangedSubview(redoButton)

        let clearButton = CaptureActionButton(
            symbol: "trash",
            label: L10n.text("Clear Annotations"),
            style: .secondary,
            target: self,
            action: #selector(clearAnnotations)
        )
        clearButton.setActionEnabled(false)
        self.clearButton = clearButton
        toolbar.addArrangedSubview(clearButton)
        toolbar.addArrangedSubview(NSView())

        let cancelButton = CaptureActionButton(
            symbol: "xmark.circle",
            label: L10n.text("Cancel (Esc)"),
            style: .secondary,
            target: self,
            action: #selector(cancel)
        )
        cancelButton.keyEquivalent = "\u{1b}"
        cancelButton.keyEquivalentModifierMask = []
        toolbar.addArrangedSubview(cancelButton)
        toolbar.addArrangedSubview(
            CaptureActionButton(
                symbol: "square.and.arrow.down",
                label: L10n.text("Save As…"),
                style: .secondary,
                target: self,
                action: #selector(save)
            )
        )
        let copyButton = CaptureActionButton(
            symbol: "doc.on.doc",
            label: L10n.text("Copy"),
            style: .primary,
            showsTitle: true,
            target: self,
            action: #selector(copyImage)
        )
        copyButton.keyEquivalent = "\r"
        copyButton.keyEquivalentModifierMask = []
        toolbar.addArrangedSubview(copyButton)

        canvas.translatesAutoresizingMaskIntoConstraints = false
        canvas.onToolChange = { [weak self] tool in
            self?.updateToolButtons(selected: tool)
        }
        canvas.onHistoryChange = { [weak self] canUndo, canRedo in
            self?.updateHistoryControls(canUndo: canUndo, canRedo: canRedo)
        }
        canvas.onConfirmRequested = { [weak self] in
            self?.copyImage()
        }
        canvas.onCancelRequested = { [weak self] in
            self?.cancel()
        }

        root.addSubview(toolbarSurface)
        toolbarSurface.addSubview(toolbar)
        root.addSubview(canvas)
        NSLayoutConstraint.activate([
            toolbarSurface.topAnchor.constraint(equalTo: root.topAnchor),
            toolbarSurface.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            toolbarSurface.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            toolbarSurface.heightAnchor.constraint(equalToConstant: 56),
            toolbar.topAnchor.constraint(equalTo: toolbarSurface.topAnchor),
            toolbar.leadingAnchor.constraint(equalTo: toolbarSurface.leadingAnchor),
            toolbar.trailingAnchor.constraint(equalTo: toolbarSurface.trailingAnchor),
            toolbar.bottomAnchor.constraint(equalTo: toolbarSurface.bottomAnchor),
            canvas.topAnchor.constraint(equalTo: toolbarSurface.bottomAnchor),
            canvas.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            canvas.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            canvas.bottomAnchor.constraint(equalTo: root.bottomAnchor)
        ])
        updateToolButtons(selected: canvas.tool)
        updateColorButtons(selected: canvas.colorPreset)
        updateTextBackgroundControl()
        updateMosaicIntensityControl()
        updateHistoryControls(canUndo: false, canRedo: false)
        controller.view = root
        return controller
    }

    private func makeSizeControl() -> (
        container: NSStackView,
        title: NSTextField,
        slider: NSSlider,
        valueLabel: NSTextField
    ) {
        let container = NSStackView()
        container.orientation = .horizontal
        container.alignment = .centerY
        container.spacing = 5

        let title = NSTextField(labelWithString: L10n.text("Line"))
        title.font = .systemFont(ofSize: 10, weight: .semibold)
        title.textColor = .secondaryLabelColor

        let slider = CaptureTrackingSlider(
            value: 3,
            minValue: 1,
            maxValue: 16,
            target: self,
            action: #selector(changeToolSize(_:))
        )
        slider.isContinuous = true
        slider.controlSize = .small
        slider.setAccessibilityLabel(L10n.text("Tool size"))
        slider.translatesAutoresizingMaskIntoConstraints = false
        slider.widthAnchor.constraint(equalToConstant: 92).isActive = true
        slider.onTrackingBegan = { [weak self] in
            guard self?.canvas.tool == .text else { return }
            self?.canvas.beginTextFontSizeAdjustment()
        }
        slider.onTrackingEnded = { [weak self] in
            guard self?.canvas.tool == .text else { return }
            self?.canvas.endTextFontSizeAdjustment()
        }

        let valueLabel = NSTextField(labelWithString: "3 px")
        valueLabel.font = .monospacedDigitSystemFont(ofSize: 9, weight: .medium)
        valueLabel.textColor = .secondaryLabelColor
        valueLabel.alignment = .right
        valueLabel.translatesAutoresizingMaskIntoConstraints = false
        valueLabel.widthAnchor.constraint(equalToConstant: 36).isActive = true

        container.addArrangedSubview(title)
        container.addArrangedSubview(slider)
        container.addArrangedSubview(valueLabel)
        return (container, title, slider, valueLabel)
    }

    private func historyButton(
        symbol: String,
        label: String,
        action: Selector,
        keyEquivalent: String,
        modifiers: NSEvent.ModifierFlags
    ) -> CaptureActionButton {
        let button = CaptureActionButton(
            symbol: symbol,
            label: label,
            style: .secondary,
            target: self,
            action: action
        )
        button.keyEquivalent = keyEquivalent
        button.keyEquivalentModifierMask = modifiers
        button.setActionEnabled(false)
        return button
    }

    private func updateToolButtons(selected: AnnotationTool) {
        for (tool, button) in toolButtons {
            button.setToolSelected(tool == selected)
        }
        updateColorButtons(selected: canvas.colorPreset)
        updateTextBackgroundControl()
        updateMosaicIntensityControl()
        textBackgroundButton?.isHidden = selected != .text
        mosaicIntensityButton?.isHidden = selected != .mosaic
        sizeControlContainer?.isHidden = selected == .select
        colorGroupContainer?.isHidden = selected == .select || selected == .mosaic
        configureSizeControl(for: selected)
    }

    private func updateHistoryControls(canUndo: Bool, canRedo: Bool) {
        undoButton?.setActionEnabled(canUndo)
        redoButton?.setActionEnabled(canRedo)
        clearButton?.setActionEnabled(canUndo)
    }

    private func updateColorButtons(selected: AnnotationColorPreset) {
        for (preset, button) in colorButtons {
            button.setColorSelected(preset == selected)
        }
    }

    private func updateTextBackgroundControl() {
        let style = canvas.textBackgroundStyle
        let label = L10n.format("Text background: %@", style.name)
        textBackgroundButton?.setToolSelected(style != .transparent)
        textBackgroundButton?.toolTip = label
        textBackgroundButton?.setAccessibilityLabel(label)
    }

    private func updateMosaicIntensityControl() {
        let intensity = canvas.mosaicIntensity
        let label = L10n.format("Mosaic strength: %@", intensity.name)
        mosaicIntensityButton?.toolTip = label
        mosaicIntensityButton?.setAccessibilityLabel(label)
    }

    private func configureSizeControl(for tool: AnnotationTool) {
        let configuration: (String, Double, Double, CGFloat, String) = switch tool {
        case .select:
            (L10n.text("Select"), 0, 1, 0, "")
        case .pen:
            (L10n.text("Brush"), 1, 24, canvas.penWidth, "px")
        case .rectangle, .line, .arrow:
            (L10n.text("Line"), 1, 16, canvas.shapeWidth, "px")
        case .text:
            (L10n.text("Font"), 12, 64, canvas.textFontSize, "pt")
        case .mosaic:
            (L10n.text("Brush"), 12, 120, canvas.mosaicBrushDiameter, "px")
        }
        sizeControlTitle?.stringValue = configuration.0
        sizeSlider?.minValue = configuration.1
        sizeSlider?.maxValue = configuration.2
        sizeSlider?.doubleValue = Double(configuration.3)
        sizeValueLabel?.stringValue = "\(Int(configuration.3.rounded())) \(configuration.4)"
    }

    @objc private func changeToolSize(_ sender: NSSlider) {
        let value = CGFloat(sender.doubleValue.rounded())
        sender.doubleValue = Double(value)
        switch canvas.tool {
        case .select:
            return
        case .pen:
            canvas.penWidth = value
        case .rectangle, .line, .arrow:
            canvas.shapeWidth = value
        case .text:
            canvas.updateTextFontSize(value)
        case .mosaic:
            canvas.mosaicBrushDiameter = value
        }
        configureSizeControl(for: canvas.tool)
    }

    @objc private func useSelect() {
        canvas.tool = .select
    }

    @objc private func usePen() {
        canvas.tool = .pen
    }

    @objc private func useRectangle() {
        canvas.tool = .rectangle
    }

    @objc private func useLine() {
        canvas.tool = .line
    }

    @objc private func useArrow() {
        canvas.tool = .arrow
    }

    @objc private func useText() {
        canvas.beginTextPlacement()
    }

    @objc private func selectAnnotationColor(_ sender: AnnotationColorSwatchButton) {
        canvas.colorPreset = sender.preset
        updateColorButtons(selected: sender.preset)
    }

    @objc private func showTextBackgroundMenu(_ sender: NSButton) {
        let menu = NSMenu()
        menu.autoenablesItems = false
        let options: [(
            AnnotationTextBackgroundStyle,
            String,
            String,
            Selector
        )] = [
            (.transparent, L10n.text("Transparent"), "circle.slash", #selector(useTransparentTextBackground)),
            (.dark, L10n.text("Dark"), "moon.fill", #selector(useDarkTextBackground)),
            (.light, L10n.text("Light"), "sun.max.fill", #selector(useLightTextBackground))
        ]
        for (option, title, symbol, action) in options {
            let item = NSMenuItem(title: title, action: action, keyEquivalent: "")
            item.target = self
            item.image = NSImage(systemSymbolName: symbol, accessibilityDescription: title)
            item.state = option == canvas.textBackgroundStyle ? .on : .off
            menu.addItem(item)
        }
        menu.popUp(
            positioning: nil,
            at: CGPoint(x: sender.bounds.minX, y: sender.bounds.maxY + 4),
            in: sender
        )
    }

    @objc private func useTransparentTextBackground() {
        selectTextBackground(.transparent)
    }

    @objc private func useDarkTextBackground() {
        selectTextBackground(.dark)
    }

    @objc private func useLightTextBackground() {
        selectTextBackground(.light)
    }

    private func selectTextBackground(_ style: AnnotationTextBackgroundStyle) {
        canvas.textBackgroundStyle = style
        updateTextBackgroundControl()
        useText()
    }

    @objc private func showMosaicIntensityMenu(_ sender: NSButton) {
        let menu = NSMenu()
        menu.autoenablesItems = false
        let options: [(MosaicIntensityPreset, String, Selector)] = [
            (.soft, L10n.text("Soft"), #selector(useSoftMosaic)),
            (.standard, L10n.text("Standard"), #selector(useStandardMosaic)),
            (.strong, L10n.text("Strong"), #selector(useStrongMosaic))
        ]
        for (option, title, action) in options {
            let item = NSMenuItem(title: title, action: action, keyEquivalent: "")
            item.target = self
            item.image = NSImage(systemSymbolName: "square.grid.3x3.fill", accessibilityDescription: title)
            item.state = option == canvas.mosaicIntensity ? .on : .off
            menu.addItem(item)
        }
        menu.popUp(
            positioning: nil,
            at: CGPoint(x: sender.bounds.minX, y: sender.bounds.maxY + 4),
            in: sender
        )
    }

    @objc private func useSoftMosaic() {
        selectMosaicIntensity(.soft)
    }

    @objc private func useStandardMosaic() {
        selectMosaicIntensity(.standard)
    }

    @objc private func useStrongMosaic() {
        selectMosaicIntensity(.strong)
    }

    private func selectMosaicIntensity(_ intensity: MosaicIntensityPreset) {
        canvas.mosaicIntensity = intensity
        updateMosaicIntensityControl()
        useMosaic()
    }

    @objc private func useMosaic() {
        canvas.tool = .mosaic
    }

    @objc private func undo() {
        canvas.undo()
    }

    @objc private func redo() {
        canvas.redo()
    }

    @objc private func clearAnnotations() {
        canvas.clearAnnotations()
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
        panel.nameFieldStringValue = "kiri-\(CaptureFilename.timestamp()).png"
        guard panel.runModal() == .OK, let url = panel.url, let image = canvas.renderedImage() else {
            return
        }
        completion(image, false, url)
        close()
    }
}
