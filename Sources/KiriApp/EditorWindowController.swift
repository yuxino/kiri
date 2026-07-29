import AppKit

@MainActor
final class EditorWindowController: NSWindowController, NSWindowDelegate {
    typealias Completion = (CGImage, Bool, URL?) -> Void

    private let completion: Completion
    private var onClose: (() -> Void)?
    private let canvas: AnnotationCanvasView

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

        let toolbar = NSStackView(views: [
            button("Pen", action: #selector(usePen)),
            button("Rectangle", action: #selector(useRectangle)),
            button("Arrow", action: #selector(useArrow)),
            button("Text", action: #selector(useText)),
            button("Mosaic", action: #selector(useMosaic)),
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
