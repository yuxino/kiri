import AppKit
import KiriCore

enum LongScreenshotCaptureError: LocalizedError {
    case selectionTooSmall
    case displayChanged
    case sectionUnavailable

    var errorDescription: String? {
        switch self {
        case .selectionTooSmall:
            L10n.text("The selected area is too small for a long screenshot.")
        case .displayChanged:
            L10n.text("Keep the pointer on the selected display while capturing the next section.")
        case .sectionUnavailable:
            L10n.text("The selected area is not available on the current display.")
        }
    }
}

@MainActor
final class LongScreenshotCaptureController: NSObject, NSWindowDelegate {
    private static let minimumSelectionSide: CGFloat = 16

    private let captureCoordinator: CaptureCoordinator
    private let region: CGRect
    private let displayFrame: CGRect
    private let displayID: CGDirectDisplayID
    private var sections: [CGImage]
    private let onFinish: ([CGImage]) -> Void
    private let onCancel: () -> Void
    private let onCaptureFailure: (Error) -> Void
    private var panel: LongScreenshotPanel?
    private var captureTask: Task<Void, Never>?
    private var isCapturing = false
    private var isClosed = false

    init(
        captureCoordinator: CaptureCoordinator,
        initialSection: CGImage,
        region: CGRect,
        displayFrame: CGRect,
        displayID: CGDirectDisplayID,
        onFinish: @escaping ([CGImage]) -> Void,
        onCancel: @escaping () -> Void,
        onCaptureFailure: @escaping (Error) -> Void
    ) {
        self.captureCoordinator = captureCoordinator
        self.region = region.standardized
        self.displayFrame = displayFrame
        self.displayID = displayID
        sections = [initialSection]
        self.onFinish = onFinish
        self.onCancel = onCancel
        self.onCaptureFailure = onCaptureFailure
        super.init()
    }

    func show() {
        guard !isClosed else { return }
        guard isRegionLargeEnough else {
            fail(LongScreenshotCaptureError.selectionTooSmall)
            return
        }

        let panel = LongScreenshotPanel(
            title: L10n.text("Long Screenshot"),
            contentRect: CGRect(x: 0, y: 0, width: 390, height: 172)
        )
        panel.onNext = { [weak self] in
            self?.captureNextSection()
        }
        panel.onUndo = { [weak self] in
            self?.undoLastSection()
        }
        panel.onFinish = { [weak self] in
            self?.finish()
        }
        panel.onCancel = { [weak self] in
            self?.cancel()
        }
        panel.delegate = self
        panel.update(sectionCount: sections.count, isCapturing: false, error: nil)

        let origin = CGPoint(
            x: displayFrame.midX - panel.frame.width / 2,
            y: displayFrame.maxY - panel.frame.height - 56
        )
        panel.setFrameOrigin(origin)
        self.panel = panel
        panel.orderFrontRegardless()
        panel.makeKey()
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        cancel()
        return false
    }

    private var isRegionLargeEnough: Bool {
        region.width >= Self.minimumSelectionSide
            && region.height >= Self.minimumSelectionSide
    }

    private func captureNextSection() {
        guard !isClosed, !isCapturing else { return }
        isCapturing = true
        panel?.update(sectionCount: sections.count, isCapturing: true, error: nil)
        panel?.orderOut(nil)

        captureTask = Task { @MainActor [weak self] in
            guard let self else { return }
            defer {
                captureTask = nil
            }
            do {
                try await Task.sleep(for: .milliseconds(140))
                try Task.checkCancellation()
                let capture = try await captureCoordinator.captureActiveDisplay()
                try Task.checkCancellation()
                guard !isClosed else { return }
                let section = try cropSection(from: capture)
                sections.append(section)
                isCapturing = false
                panel?.update(sectionCount: sections.count, isCapturing: false, error: nil)
                panel?.orderFrontRegardless()
                panel?.makeKey()
            } catch is CancellationError {
                guard !isClosed else { return }
                isCapturing = false
                panel?.update(sectionCount: sections.count, isCapturing: false, error: nil)
                panel?.orderFrontRegardless()
                panel?.makeKey()
            } catch {
                guard !isClosed else { return }
                isCapturing = false
                panel?.update(
                    sectionCount: sections.count,
                    isCapturing: false,
                    error: error.localizedDescription
                )
                panel?.orderFrontRegardless()
                panel?.makeKey()
                onCaptureFailure(error)
            }
        }
    }

    private func cropSection(from capture: CapturedDisplay) throws -> CGImage {
        guard capture.displayID == displayID else {
            throw LongScreenshotCaptureError.displayChanged
        }
        guard capture.screenFrame.size.width > 0,
              capture.screenFrame.size.height > 0 else {
            throw LongScreenshotCaptureError.sectionUnavailable
        }

        let pixelRect = SelectionGeometry.pixelRect(
            forTopLeftRect: region,
            canvasSize: capture.screenFrame.size,
            imageSize: CGSize(width: capture.image.width, height: capture.image.height)
        ).intersection(
            CGRect(x: 0, y: 0, width: capture.image.width, height: capture.image.height)
        )
        guard pixelRect.width >= 1, pixelRect.height >= 1,
              let section = capture.image.cropping(to: pixelRect) else {
            throw LongScreenshotCaptureError.sectionUnavailable
        }
        return section
    }

    private func undoLastSection() {
        guard !isClosed, !isCapturing, sections.count > 1 else { return }
        sections.removeLast()
        panel?.update(sectionCount: sections.count, isCapturing: false, error: nil)
    }

    private func finish() {
        guard !isClosed, !isCapturing, !sections.isEmpty else { return }
        isClosed = true
        closePanel()
        onFinish(sections)
    }

    private func cancel() {
        guard !isClosed else { return }
        isClosed = true
        captureTask?.cancel()
        closePanel()
        onCancel()
    }

    private func fail(_ error: Error) {
        guard !isClosed else { return }
        isClosed = true
        closePanel()
        onCaptureFailure(error)
        onCancel()
    }

    private func closePanel() {
        panel?.delegate = nil
        panel?.orderOut(nil)
        panel?.close()
        panel = nil
        NSCursor.arrow.set()
    }
}

private final class LongScreenshotPanel: NSPanel {
    var onNext: (() -> Void)?
    var onUndo: (() -> Void)?
    var onFinish: (() -> Void)?
    var onCancel: (() -> Void)?

    private let sectionLabel = NSTextField(labelWithString: "")
    private let statusLabel = NSTextField(labelWithString: "")
    private let errorLabel = NSTextField(labelWithString: "")
    private let nextButton = NSButton()
    private let undoButton = NSButton()
    private let finishButton = NSButton()
    private let cancelButton = NSButton()

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    init(title: String, contentRect: CGRect) {
        super.init(
            contentRect: contentRect,
            styleMask: [.titled, .utilityWindow, .closable],
            backing: .buffered,
            defer: false
        )
        self.title = title
        titleVisibility = .visible
        titlebarAppearsTransparent = true
        isFloatingPanel = true
        level = .floating
        hidesOnDeactivate = false
        isMovableByWindowBackground = true
        isReleasedWhenClosed = false
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        becomesKeyOnlyIfNeeded = false
        minSize = contentRect.size
        maxSize = contentRect.size
        setupContent()
    }

    override func sendEvent(_ event: NSEvent) {
        guard event.type == .keyDown else {
            super.sendEvent(event)
            return
        }
        switch event.keyCode {
        case 53:
            onCancel?()
        case 36, 76, 49:
            onNext?()
        default:
            super.sendEvent(event)
        }
    }

    override func cancelOperation(_ sender: Any?) {
        onCancel?()
    }

    override func performClose(_ sender: Any?) {
        onCancel?()
    }

    func update(sectionCount: Int, isCapturing: Bool, error: String?) {
        sectionLabel.stringValue = L10n.format("Long Screenshot Sections: %d", sectionCount)
        statusLabel.stringValue = isCapturing
            ? L10n.text("Capturing next section…")
            : L10n.text("Scroll the original app, then capture the next section.")
        errorLabel.stringValue = error ?? ""
        errorLabel.isHidden = error == nil
        nextButton.isEnabled = !isCapturing
        undoButton.isEnabled = !isCapturing && sectionCount > 1
        finishButton.isEnabled = !isCapturing && sectionCount > 0
        cancelButton.isEnabled = !isCapturing
    }

    private func setupContent() {
        let effect = NSVisualEffectView()
        effect.material = .popover
        effect.state = .active
        effect.wantsLayer = true
        effect.layer?.cornerRadius = 12
        effect.layer?.cornerCurve = .continuous
        effect.layer?.borderWidth = 1
        effect.layer?.borderColor = CaptureUIColors.surfaceBorder.cgColor

        let content = NSView()
        content.translatesAutoresizingMaskIntoConstraints = false
        effect.addSubview(content)
        NSLayoutConstraint.activate([
            content.topAnchor.constraint(equalTo: effect.topAnchor, constant: 12),
            content.leadingAnchor.constraint(equalTo: effect.leadingAnchor, constant: 14),
            content.trailingAnchor.constraint(equalTo: effect.trailingAnchor, constant: -14),
            content.bottomAnchor.constraint(equalTo: effect.bottomAnchor, constant: -12)
        ])

        sectionLabel.font = NSFont.systemFont(ofSize: 12, weight: .semibold)
        sectionLabel.textColor = .secondaryLabelColor
        sectionLabel.alignment = .right

        let heading = NSTextField(labelWithString: L10n.text("Long Screenshot"))
        heading.font = NSFont.systemFont(ofSize: 14, weight: .semibold)

        statusLabel.font = NSFont.systemFont(ofSize: 12)
        statusLabel.textColor = .secondaryLabelColor
        statusLabel.lineBreakMode = .byTruncatingTail

        errorLabel.font = NSFont.systemFont(ofSize: 11, weight: .medium)
        errorLabel.textColor = .systemRed
        errorLabel.maximumNumberOfLines = 2
        errorLabel.lineBreakMode = .byWordWrapping
        errorLabel.isHidden = true

        let headingRow = NSStackView(views: [heading, sectionLabel])
        headingRow.orientation = .horizontal
        headingRow.alignment = .centerY
        headingRow.spacing = 8
        headingRow.distribution = .fill

        let buttonRow = NSStackView()
        buttonRow.orientation = .horizontal
        buttonRow.alignment = .centerY
        buttonRow.spacing = 6
        buttonRow.addArrangedSubview(makeButton(
            nextButton,
            title: L10n.text("Capture Next Section"),
            symbol: "arrow.down.to.line",
            primary: true,
            action: #selector(nextAction)
        ))
        buttonRow.addArrangedSubview(makeButton(
            undoButton,
            title: L10n.text("Undo Last"),
            symbol: "arrow.uturn.backward",
            primary: false,
            action: #selector(undoAction)
        ))

        let finishRow = NSStackView()
        finishRow.orientation = .horizontal
        finishRow.alignment = .centerY
        finishRow.spacing = 6
        finishRow.addArrangedSubview(makeButton(
            finishButton,
            title: L10n.text("Finish & Copy"),
            symbol: "checkmark",
            primary: true,
            action: #selector(finishAction)
        ))
        finishRow.addArrangedSubview(makeButton(
            cancelButton,
            title: L10n.text("Cancel (Esc)"),
            symbol: "xmark",
            primary: false,
            action: #selector(cancelAction)
        ))

        let rows = NSStackView(views: [headingRow, statusLabel, errorLabel, buttonRow, finishRow])
        rows.orientation = .vertical
        rows.alignment = .width
        rows.spacing = 7
        rows.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(rows)
        NSLayoutConstraint.activate([
            rows.topAnchor.constraint(equalTo: content.topAnchor),
            rows.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            rows.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            rows.bottomAnchor.constraint(equalTo: content.bottomAnchor),
            headingRow.heightAnchor.constraint(equalToConstant: 20),
            buttonRow.heightAnchor.constraint(equalToConstant: 30),
            finishRow.heightAnchor.constraint(equalToConstant: 30)
        ])

        contentView = effect
    }

    private func makeButton(
        _ button: NSButton,
        title: String,
        symbol: String,
        primary: Bool,
        action: Selector
    ) -> NSButton {
        button.title = title
        button.image = NSImage(
            systemSymbolName: symbol,
            accessibilityDescription: title
        )?.withSymbolConfiguration(
            NSImage.SymbolConfiguration(pointSize: 11, weight: .semibold)
        )
        button.imagePosition = .imageLeading
        button.imageHugsTitle = true
        button.target = self
        button.action = action
        button.bezelStyle = primary ? .rounded : .texturedRounded
        button.controlSize = .small
        button.setAccessibilityLabel(title)
        button.toolTip = title
        return button
    }

    @objc private func nextAction() {
        onNext?()
    }

    @objc private func undoAction() {
        onUndo?()
    }

    @objc private func finishAction() {
        onFinish?()
    }

    @objc private func cancelAction() {
        onCancel?()
    }
}
