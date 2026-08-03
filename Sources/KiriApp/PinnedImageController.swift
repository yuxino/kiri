import AppKit

@MainActor
final class PinnedImageController: NSObject {
    private var panel: NSPanel?
    var onClose: (() -> Void)?

    func show(image: NSImage) {
        let size = Self.initialSize(for: image.size)
        let content = PinnedImageView(image: image)
        content.onClose = { [weak self] in
            self?.close()
        }

        let panel = NSPanel(
            contentRect: CGRect(origin: .zero, size: size),
            styleMask: [.borderless, .resizable, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.level = .floating
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.backgroundColor = .windowBackgroundColor
        panel.isOpaque = true
        panel.hasShadow = true
        panel.isReleasedWhenClosed = false
        panel.hidesOnDeactivate = false
        panel.isMovableByWindowBackground = true
        panel.contentMinSize = CGSize(width: 140, height: 90)
        panel.contentView = content
        panel.setFrameOrigin(Self.origin(for: size))
        panel.orderFrontRegardless()
        self.panel = panel
    }

    func close() {
        panel?.orderOut(nil)
        panel?.close()
        panel = nil
        let callback = onClose
        onClose = nil
        callback?()
    }

    private static func initialSize(for imageSize: CGSize) -> CGSize {
        guard imageSize.width > 0, imageSize.height > 0 else {
            return CGSize(width: 360, height: 240)
        }
        let maximum = CGSize(width: 520, height: 420)
        let scale = min(
            maximum.width / imageSize.width,
            maximum.height / imageSize.height,
            1
        )
        return CGSize(
            width: max(180, imageSize.width * scale),
            height: max(120, imageSize.height * scale)
        )
    }

    private static func origin(for size: CGSize) -> CGPoint {
        let mouse = NSEvent.mouseLocation
        let screen = NSScreen.screens.first(where: { NSMouseInRect(mouse, $0.frame, false) })
            ?? NSScreen.main
        let frame = screen?.visibleFrame ?? CGRect(x: 0, y: 0, width: 900, height: 700)
        return CGPoint(
            x: min(max(mouse.x - size.width / 2, frame.minX + 12), frame.maxX - size.width - 12),
            y: min(max(mouse.y - size.height / 2, frame.minY + 12), frame.maxY - size.height - 12)
        )
    }
}

private final class PinnedImageView: NSView {
    var onClose: (() -> Void)?

    init(image: NSImage) {
        super.init(frame: .zero)
        wantsLayer = true
        layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor

        let imageView = PinnedContentImageView(image: image)
        imageView.imageScaling = .scaleProportionallyUpOrDown
        imageView.translatesAutoresizingMaskIntoConstraints = false

        let closeButton = NSButton(
            image: NSImage(
                systemSymbolName: "xmark.circle.fill",
                accessibilityDescription: L10n.text("Close")
            )
                ?? NSImage(),
            target: self,
            action: #selector(close)
        )
        closeButton.bezelStyle = .inline
        closeButton.imagePosition = .imageOnly
        closeButton.translatesAutoresizingMaskIntoConstraints = false

        addSubview(imageView)
        addSubview(closeButton)
        NSLayoutConstraint.activate([
            imageView.topAnchor.constraint(equalTo: topAnchor, constant: 5),
            imageView.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 5),
            imageView.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -5),
            imageView.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -5),
            closeButton.topAnchor.constraint(equalTo: topAnchor, constant: 7),
            closeButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -7),
            closeButton.widthAnchor.constraint(equalToConstant: 24),
            closeButton.heightAnchor.constraint(equalToConstant: 24)
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override func mouseDown(with event: NSEvent) {
        window?.performDrag(with: event)
    }

    @objc private func close() {
        onClose?()
    }
}

private final class PinnedContentImageView: NSImageView {
    override func mouseDown(with event: NSEvent) {
        window?.performDrag(with: event)
    }
}
