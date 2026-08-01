import AppKit

@MainActor
final class QuickAccessController: NSObject {
    private var panel: NSPanel?
    private var dismissTimer: Timer?
    private var onClose: (() -> Void)?

    func show(
        image: NSImage,
        fileURL: URL,
        onCopy: @escaping () -> Void,
        onSave: @escaping () -> Void,
        onPin: @escaping () -> Void,
        onEdit: @escaping () -> Void,
        onClose: @escaping () -> Void
    ) {
        close()
        self.onClose = onClose

        let content = QuickAccessView(image: image, fileURL: fileURL)
        content.onCopy = onCopy
        content.onSave = onSave
        content.onPin = { [weak self] in
            onPin()
            self?.close()
        }
        content.onEdit = {
            onEdit()
        }
        content.onClose = { [weak self] in
            self?.close()
        }
        content.onHoverChange = { [weak self] hovering in
            if hovering {
                self?.dismissTimer?.invalidate()
                self?.dismissTimer = nil
            } else {
                self?.scheduleDismiss()
            }
        }

        let size = CGSize(width: 310, height: 222)
        let panel = NSPanel(
            contentRect: CGRect(origin: .zero, size: size),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.level = .floating
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.backgroundColor = .clear
        panel.isOpaque = false
        panel.hasShadow = true
        panel.isReleasedWhenClosed = false
        panel.hidesOnDeactivate = false
        panel.contentView = content
        panel.setFrameOrigin(Self.origin(for: size))
        panel.orderFrontRegardless()
        self.panel = panel
        scheduleDismiss()
    }

    func close() {
        dismissTimer?.invalidate()
        dismissTimer = nil
        panel?.orderOut(nil)
        panel?.close()
        panel = nil
        let callback = onClose
        onClose = nil
        callback?()
    }

    private func scheduleDismiss() {
        dismissTimer?.invalidate()
        dismissTimer = Timer.scheduledTimer(withTimeInterval: 12, repeats: false) { [weak self] _ in
            Task { @MainActor in
                self?.close()
            }
        }
    }

    private static func origin(for size: CGSize) -> CGPoint {
        let mouse = NSEvent.mouseLocation
        let screen = NSScreen.screens.first(where: { NSMouseInRect(mouse, $0.frame, false) })
            ?? NSScreen.main
        let frame = screen?.visibleFrame ?? CGRect(x: 0, y: 0, width: 900, height: 700)
        return CGPoint(
            x: frame.maxX - size.width - 18,
            y: frame.minY + 18
        )
    }
}

private final class QuickAccessView: NSVisualEffectView {
    var onCopy: (() -> Void)?
    var onSave: (() -> Void)?
    var onPin: (() -> Void)?
    var onEdit: (() -> Void)?
    var onClose: (() -> Void)?
    var onHoverChange: ((Bool) -> Void)?

    private var hoverTrackingArea: NSTrackingArea?

    init(image: NSImage, fileURL: URL) {
        super.init(frame: .zero)
        material = .popover
        blendingMode = .behindWindow
        state = .active
        wantsLayer = true
        layer?.cornerRadius = 12
        layer?.borderWidth = 1
        layer?.borderColor = CaptureUIColors.surfaceBorder.cgColor
        layer?.masksToBounds = true

        let preview = DraggableImageView(image: image, fileURL: fileURL)
        preview.imageScaling = .scaleProportionallyUpOrDown
        preview.wantsLayer = true
        preview.layer?.cornerRadius = 9
        preview.layer?.masksToBounds = true
        preview.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.18).cgColor
        preview.translatesAutoresizingMaskIntoConstraints = false

        let actions = NSStackView(views: [
            CaptureActionButton(
                symbol: "doc.on.doc.fill",
                label: "Copy",
                style: .primary,
                showsTitle: true,
                target: self,
                action: #selector(copyCapture)
            ),
            CaptureActionButton(
                symbol: "ellipsis",
                label: "More Actions",
                style: .secondary,
                target: self,
                action: #selector(showMoreActions(_:))
            ),
            NSView(),
            CaptureActionButton(
                symbol: "xmark",
                label: "Dismiss",
                style: .secondary,
                target: self,
                action: #selector(dismiss)
            )
        ])
        actions.orientation = .horizontal
        actions.alignment = .centerY
        actions.spacing = 4
        actions.translatesAutoresizingMaskIntoConstraints = false

        let hint = NSTextField(labelWithString: "Saved to History  ·  Drag preview to share")
        hint.font = .systemFont(ofSize: 10, weight: .medium)
        hint.textColor = .secondaryLabelColor
        hint.alignment = .center
        hint.translatesAutoresizingMaskIntoConstraints = false

        addSubview(preview)
        addSubview(actions)
        addSubview(hint)
        NSLayoutConstraint.activate([
            preview.topAnchor.constraint(equalTo: topAnchor, constant: 10),
            preview.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            preview.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            preview.heightAnchor.constraint(equalToConstant: 150),

            actions.topAnchor.constraint(equalTo: preview.bottomAnchor, constant: 7),
            actions.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            actions.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            actions.heightAnchor.constraint(equalToConstant: 32),

            hint.topAnchor.constraint(equalTo: actions.bottomAnchor, constant: 2),
            hint.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            hint.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            hint.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -6)
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let hoverTrackingArea {
            removeTrackingArea(hoverTrackingArea)
        }
        let area = NSTrackingArea(
            rect: bounds,
            options: [.activeAlways, .mouseEnteredAndExited, .inVisibleRect],
            owner: self
        )
        addTrackingArea(area)
        hoverTrackingArea = area
    }

    override func mouseEntered(with event: NSEvent) {
        onHoverChange?(true)
    }

    override func mouseExited(with event: NSEvent) {
        onHoverChange?(false)
    }

    @objc private func copyCapture() {
        onCopy?()
    }

    @objc private func showMoreActions(_ sender: NSButton) {
        let menu = NSMenu()
        menu.addItem(
            menuItem("Save As…", symbol: "square.and.arrow.down", action: #selector(saveCapture))
        )
        menu.addItem(
            menuItem("Pin on Screen", symbol: "pin", action: #selector(pinCapture))
        )
        menu.addItem(
            menuItem("Open in Editor", symbol: "slider.horizontal.3", action: #selector(editCapture))
        )
        menu.popUp(
            positioning: nil,
            at: CGPoint(x: sender.bounds.minX, y: sender.bounds.maxY + 4),
            in: sender
        )
    }

    private func menuItem(_ title: String, symbol: String, action: Selector) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: "")
        item.target = self
        item.image = NSImage(systemSymbolName: symbol, accessibilityDescription: title)
        return item
    }

    @objc private func saveCapture() {
        onSave?()
    }

    @objc private func pinCapture() {
        onPin?()
    }

    @objc private func editCapture() {
        onEdit?()
    }

    @objc private func dismiss() {
        onClose?()
    }
}

private final class DraggableImageView: NSImageView, NSDraggingSource {
    private let fileURL: URL
    private var mouseDownPoint: CGPoint?
    private var startedDragging = false

    init(image: NSImage, fileURL: URL) {
        self.fileURL = fileURL
        super.init(frame: .zero)
        self.image = image
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override func mouseDown(with event: NSEvent) {
        mouseDownPoint = convert(event.locationInWindow, from: nil)
        startedDragging = false
    }

    override func mouseDragged(with event: NSEvent) {
        guard !startedDragging, let mouseDownPoint else { return }
        let point = convert(event.locationInWindow, from: nil)
        guard hypot(point.x - mouseDownPoint.x, point.y - mouseDownPoint.y) >= 3 else {
            return
        }
        startedDragging = true

        let item = NSDraggingItem(pasteboardWriter: fileURL as NSURL)
        item.setDraggingFrame(bounds, contents: image)
        beginDraggingSession(with: [item], event: event, source: self)
    }

    func draggingSession(
        _ session: NSDraggingSession,
        sourceOperationMaskFor context: NSDraggingContext
    ) -> NSDragOperation {
        .copy
    }

    func ignoreModifierKeys(for session: NSDraggingSession) -> Bool {
        true
    }
}
