import AppKit

@MainActor
final class EditorWindowController: NSWindowController, NSWindowDelegate {
    typealias Completion = (CGImage, Bool, URL?) -> Void

    private let completion: Completion
    private var onClose: (() -> Void)?
    private let canvas: AnnotationCanvasView
    private var toolButtons: [AnnotationTool: CaptureActionButton] = [:]
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
        window.title = "kiri"
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

        let toolbarSurface = NSVisualEffectView()
        toolbarSurface.material = .headerView
        toolbarSurface.blendingMode = .withinWindow
        toolbarSurface.state = .active
        toolbarSurface.translatesAutoresizingMaskIntoConstraints = false

        let toolbar = NSStackView()
        toolbar.orientation = .horizontal
        toolbar.alignment = .centerY
        toolbar.spacing = 4
        toolbar.edgeInsets = NSEdgeInsets(top: 8, left: 10, bottom: 8, right: 10)
        toolbar.translatesAutoresizingMaskIntoConstraints = false

        let tools: [(AnnotationTool, String, String, String, Selector)] = [
            (.pen, "pencil.tip", "Pen (P)", "p", #selector(usePen)),
            (.rectangle, "rectangle.dashed", "Rectangle (R)", "r", #selector(useRectangle)),
            (.line, "line.diagonal", "Line (L)", "l", #selector(useLine)),
            (.arrow, "arrow.up.right", "Arrow (A)", "a", #selector(useArrow)),
            (.text, "textformat", "Text (T)", "t", #selector(useText)),
            (.mosaic, "square.grid.3x3.fill", "Mosaic (M)", "m", #selector(useMosaic))
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
        toolbar.addArrangedSubview(CaptureDividerView(height: 24))

        let undoButton = historyButton(
            symbol: "arrow.uturn.backward",
            label: "Undo (⌘Z)",
            action: #selector(undo),
            keyEquivalent: "z",
            modifiers: [.command]
        )
        self.undoButton = undoButton
        toolbar.addArrangedSubview(undoButton)

        let redoButton = historyButton(
            symbol: "arrow.uturn.forward",
            label: "Redo (⇧⌘Z)",
            action: #selector(redo),
            keyEquivalent: "z",
            modifiers: [.command, .shift]
        )
        self.redoButton = redoButton
        toolbar.addArrangedSubview(redoButton)

        let clearButton = CaptureActionButton(
            symbol: "trash",
            label: "Clear Annotations",
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
            label: "Cancel (Esc)",
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
                label: "Save As…",
                style: .secondary,
                target: self,
                action: #selector(save)
            )
        )
        let copyButton = CaptureActionButton(
            symbol: "doc.on.doc",
            label: "Copy",
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

        root.addSubview(toolbarSurface)
        toolbarSurface.addSubview(toolbar)
        root.addSubview(canvas)
        NSLayoutConstraint.activate([
            toolbarSurface.topAnchor.constraint(equalTo: root.topAnchor),
            toolbarSurface.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            toolbarSurface.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            toolbarSurface.heightAnchor.constraint(equalToConstant: 50),
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
        updateHistoryControls(canUndo: false, canRedo: false)
        controller.view = root
        return controller
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
    }

    private func updateHistoryControls(canUndo: Bool, canRedo: Bool) {
        undoButton?.setActionEnabled(canUndo)
        redoButton?.setActionEnabled(canRedo)
        clearButton?.setActionEnabled(canUndo)
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
        guard let text = AnnotationTextPrompt.requestText() else { return }
        canvas.beginTextPlacement(text)
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
