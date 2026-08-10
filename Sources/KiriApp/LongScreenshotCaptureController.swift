import AppKit
import KiriCore
import os

private let longScreenshotLog = Logger(
    subsystem: "io.yuxino.kiri",
    category: "LongScreenshot"
)

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
    private static let previewThumbnailWidth = 200
    private static let activePollMilliseconds: Int64 = 140
    private static let idlePollMilliseconds: Int64 = 500
    private static let settleDelay: TimeInterval = 0.35
    private static let minimumAdvanceFraction: Double = 0.12
    private static let changedFractionThreshold: Double = 0.02

    private let captureCoordinator: any DisplayCapturing
    private let region: CGRect
    private let displayFrame: CGRect
    private let displayID: CGDirectDisplayID

    // Full-resolution sections are stored as PNG data so a long recording does
    // not pin dozens of full-display pixel buffers in memory. Thumbnails drive
    // the live preview.
    private var sectionData: [Data] = []
    private var thumbnails: [CGImage] = []
    private var previewImage: CGImage?
    private var lastSection: CGImage?
    private var lastPollFrame: CGImage?

    /// Number of recorded sections; exposed for deterministic testing.
    var recordedSectionCount: Int {
        sectionData.count
    }

    private let onFinish: ([CGImage]) -> Void
    private let onCancel: () -> Void
    private let onCaptureFailure: (Error) -> Void

    private var panel: LongScreenshotPanel?
    private var recordingTask: Task<Void, Never>?
    private var recordingEscapeMonitor: Any?
    private var isRecording = false
    private var isClosed = false
    private var lastScrollActivity = Date()

    private let recordConfiguration = LongScreenshotStitcherConfiguration(
        maxOverlapFraction: 0.95,
        minimumOverlapPixels: 8,
        maximumOutputHeight: 100_000
    )

    init(
        captureCoordinator: any DisplayCapturing,
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
        let initialData = NSBitmapImageRep(cgImage: initialSection)
            .representation(using: .png, properties: [:]) ?? Data()
        sectionData = [initialData]
        let initialThumb = Self.thumbnail(of: initialSection, width: Self.previewThumbnailWidth)
        thumbnails = [initialThumb]
        previewImage = initialThumb
        lastSection = initialSection
        lastPollFrame = initialSection
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
            contentRect: CGRect(x: 0, y: 0, width: 340, height: 424)
        )
        panel.onPrimaryAction = { [weak self] in
            guard let self else { return }
            if self.isRecording {
                self.stopRecording()
            } else {
                self.startRecording()
            }
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
        panel.update(
            sectionCount: sectionData.count,
            isRecording: false,
            preview: previewImage,
            error: nil
        )

        let origin = CGPoint(
            x: displayFrame.maxX - panel.frame.width - 24,
            y: displayFrame.maxY - panel.frame.height - 24
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

    // MARK: - Recording

    func startRecording() {
        guard !isClosed, !isRecording, lastSection != nil else { return }
        isRecording = true
        lastScrollActivity = Date()
        lastPollFrame = lastSection
        longScreenshotLog.info("startRecording: sections=\(self.sectionData.count)")
        panel?.update(
            sectionCount: sectionData.count,
            isRecording: true,
            preview: previewImage,
            error: nil
        )
        installRecordingEscapeMonitor()
        recordingTask = Task { [weak self] in
            await self?.runRecordingLoop()
        }
    }

    func stopRecording() {
        guard isRecording else { return }
        isRecording = false
        recordingTask?.cancel()
        recordingTask = nil
        removeRecordingEscapeMonitor()
        panel?.update(
            sectionCount: sectionData.count,
            isRecording: false,
            preview: previewImage,
            error: nil
        )
        panel?.orderFrontRegardless()
        panel?.makeKey()
        Task { [weak self] in
            await self?.captureTailSection()
        }
    }

    private func captureTailSection() async {
        guard !isClosed else { return }
        do {
            try await Task.sleep(for: .milliseconds(160))
            let panelWindowID = CGWindowID(panel?.windowNumber ?? 0)
            let capture = try await captureCoordinator.captureActiveDisplay(
                excludingWindowIDs: panelWindowID > 0 ? [panelWindowID] : []
            )
            let frame = try cropSection(from: capture)
            guard let last = lastSection else { return }
            let overlap = LongScreenshotOverlapDetector.detectOverlap(
                between: last,
                and: frame,
                configuration: recordConfiguration
            )
            let advance = frame.height - overlap
            if overlap >= recordConfiguration.minimumOverlapPixels, advance >= 1 {
                longScreenshotLog.info("tail capture: overlap=\(overlap) advance=\(advance)")
                appendSection(frame, overlap: overlap)
            } else {
                longScreenshotLog.info("tail capture: no new content (overlap=\(overlap) advance=\(advance))")
            }
        } catch {
            longScreenshotLog.error("tail capture failed: \(String(describing: error))")
        }
    }

    private func runRecordingLoop() async {
        while !Task.isCancelled && isRecording && !isClosed {
            let interval = (panel?.isVisible == true)
                ? Self.idlePollMilliseconds
                : Self.activePollMilliseconds
            do {
                try await Task.sleep(for: .milliseconds(interval))
                try Task.checkCancellation()
                guard isRecording, !isClosed else { return }
                let panelWindowID = CGWindowID(panel?.windowNumber ?? 0)
                let capture = try await captureCoordinator.captureActiveDisplay(
                    excludingWindowIDs: panelWindowID > 0 ? [panelWindowID] : []
                )
                try Task.checkCancellation()
                guard isRecording, !isClosed else { return }
                let frame = try cropSection(from: capture)
                evaluateFrame(frame)
            } catch is CancellationError {
                return
            } catch {
                guard !isClosed else { return }
                isRecording = false
                recordingTask = nil
                removeRecordingEscapeMonitor()
                longScreenshotLog.error("capture failed: \(String(describing: error))")
                panel?.update(
                    sectionCount: sectionData.count,
                    isRecording: false,
                    preview: previewImage,
                    error: error.localizedDescription
                )
                panel?.orderFrontRegardless()
                panel?.makeKey()
                onCaptureFailure(error)
                return
            }
        }
    }

    private func evaluateFrame(_ frame: CGImage) {
        let changed = Self.changedFraction(between: lastPollFrame, and: frame)
            > Self.changedFractionThreshold
        lastPollFrame = frame

        if let last = lastSection {
            let overlap = LongScreenshotOverlapDetector.detectOverlap(
                between: last,
                and: frame,
                configuration: recordConfiguration
            )
            let minimumAdvance = max(
                8,
                Int((Double(frame.height) * Self.minimumAdvanceFraction).rounded())
            )
            let advance = frame.height - overlap
            let shouldAppend = overlap >= recordConfiguration.minimumOverlapPixels
                && advance >= minimumAdvance
            longScreenshotLog.info(
                "poll: frame=\(frame.width)x\(frame.height) changed=\(changed) overlap=\(overlap) advance=\(advance) minAdvance=\(minimumAdvance) append=\(shouldAppend) sections=\(self.sectionData.count)"
            )
            if shouldAppend {
                appendSection(frame, overlap: overlap)
                lastScrollActivity = Date()
            }
        } else {
            longScreenshotLog.info("poll: lastSection is nil, skipping")
        }

        if !changed,
           Date().timeIntervalSince(lastScrollActivity) >= Self.settleDelay,
           panel?.isVisible != true {
            panel?.orderFrontRegardless()
        }
    }

    private func appendSection(_ frame: CGImage, overlap: Int) {
        guard let data = NSBitmapImageRep(cgImage: frame)
            .representation(using: .png, properties: [:]) else {
            return
        }
        let overlapFraction = Double(overlap) / Double(max(1, frame.height))
        let thumb = Self.thumbnail(of: frame, width: Self.previewThumbnailWidth)
        thumbnails.append(thumb)
        previewImage = Self.previewByAppending(
            previewImage,
            thumb: thumb,
            overlapFraction: overlapFraction
        )
        sectionData.append(data)
        lastSection = frame
        panel?.update(
            sectionCount: sectionData.count,
            isRecording: isRecording,
            preview: previewImage,
            error: nil
        )
    }

    private func undoLastSection() {
        guard !isClosed, !isRecording, sectionData.count > 1 else { return }
        sectionData.removeLast()
        thumbnails.removeLast()
        previewImage = Self.rebuildPreview(from: thumbnails)
        lastSection = sectionData.last.flatMap(Self.image(from:))
        panel?.update(
            sectionCount: sectionData.count,
            isRecording: false,
            preview: previewImage,
            error: nil
        )
    }

    func finish() {
        guard !isClosed, !isRecording, !sectionData.isEmpty else { return }
        isClosed = true
        recordingTask?.cancel()
        recordingTask = nil
        removeRecordingEscapeMonitor()
        closePanel()
        let images = sectionData.compactMap(Self.image(from:))
        guard images.count == sectionData.count else {
            onCaptureFailure(LongScreenshotCaptureError.sectionUnavailable)
            onCancel()
            return
        }
        onFinish(images)
    }

    private func cancel() {
        guard !isClosed else { return }
        isClosed = true
        isRecording = false
        recordingTask?.cancel()
        recordingTask = nil
        removeRecordingEscapeMonitor()
        closePanel()
        onCancel()
    }

    private func fail(_ error: Error) {
        guard !isClosed else { return }
        isClosed = true
        isRecording = false
        recordingTask?.cancel()
        recordingTask = nil
        removeRecordingEscapeMonitor()
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

    // MARK: - Escape handling

    private func installRecordingEscapeMonitor() {
        removeRecordingEscapeMonitor()
        recordingEscapeMonitor = NSEvent.addGlobalMonitorForEvents(matching: .keyDown) {
            [weak self] event in
            guard event.keyCode == 53 else { return }
            Task { @MainActor in
                guard let self, self.isRecording, !self.isClosed else { return }
                self.stopRecording()
            }
        }
    }

    private func removeRecordingEscapeMonitor() {
        if let recordingEscapeMonitor {
            NSEvent.removeMonitor(recordingEscapeMonitor)
        }
        recordingEscapeMonitor = nil
    }

    // MARK: - Cropping

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

    // MARK: - Image helpers

    private static func image(from data: Data) -> CGImage? {
        guard let rep = NSBitmapImageRep(data: data) else { return nil }
        return rep.cgImage
    }

    private static func thumbnail(of image: CGImage, width: Int) -> CGImage {
        let height = max(
            1,
            Int((Double(image.height) * Double(width) / Double(max(1, image.width))).rounded())
        )
        let context = CGContext(
            data: nil,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: width * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        )!
        context.interpolationQuality = .medium
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        return context.makeImage() ?? image
    }

    private static func previewByAppending(
        _ canvas: CGImage?,
        thumb: CGImage,
        overlapFraction: Double
    ) -> CGImage? {
        let thumbWidth = thumb.width
        let thumbOverlap = min(
            thumb.height,
            Int((Double(thumb.height) * min(max(overlapFraction, 0), 1)).rounded())
        )
        let addedHeight = thumb.height - thumbOverlap
        guard addedHeight > 0 else { return canvas }
        let oldHeight = canvas?.height ?? 0
        let newHeight = oldHeight + addedHeight
        let context = CGContext(
            data: nil,
            width: thumbWidth,
            height: newHeight,
            bitsPerComponent: 8,
            bytesPerRow: thumbWidth * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        )!
        context.interpolationQuality = .none
        if let canvas {
            context.draw(
                canvas,
                in: CGRect(x: 0, y: addedHeight, width: thumbWidth, height: oldHeight)
            )
        }
        context.saveGState()
        context.clip(to: CGRect(x: 0, y: 0, width: thumbWidth, height: addedHeight))
        context.draw(
            thumb,
            in: CGRect(
                x: 0,
                y: -(thumb.height - addedHeight),
                width: thumbWidth,
                height: thumb.height
            )
        )
        context.restoreGState()
        return context.makeImage()
    }

    private static func rebuildPreview(from thumbnails: [CGImage]) -> CGImage? {
        guard let first = thumbnails.first else { return nil }
        guard thumbnails.count > 1 else { return first }
        let configuration = LongScreenshotStitcherConfiguration(
            maxOverlapFraction: 0.95,
            minimumOverlapPixels: 2,
            maximumOutputHeight: 100_000
        )
        return try? LongScreenshotStitcher(configuration: configuration)
            .stitch(thumbnails)
            .image
    }

    private static func changedFraction(between first: CGImage?, and second: CGImage) -> Double {
        guard let first,
              first.width == second.width,
              first.height == second.height,
              first.width > 0,
              first.height > 0 else {
            return 1
        }
        let sampleWidth = min(64, first.width)
        let sampleHeight = min(48, first.height)
        guard let firstSamples = grayscaleSamples(of: first, width: sampleWidth, height: sampleHeight),
              let secondSamples = grayscaleSamples(of: second, width: sampleWidth, height: sampleHeight) else {
            return 1
        }
        var differences = 0
        for index in firstSamples.indices {
            if abs(Int(firstSamples[index]) - Int(secondSamples[index])) > 12 {
                differences += 1
            }
        }
        return Double(differences) / Double(firstSamples.count)
    }

    private static func grayscaleSamples(
        of image: CGImage,
        width: Int,
        height: Int
    ) -> [UInt8]? {
        guard width > 0, height > 0 else { return nil }
        let bytesPerRow = width
        var pixels = [UInt8](repeating: 0, count: width * height)
        let rendered = pixels.withUnsafeMutableBytes { buffer -> Bool in
            guard let context = CGContext(
                data: buffer.baseAddress,
                width: width,
                height: height,
                bitsPerComponent: 8,
                bytesPerRow: bytesPerRow,
                space: CGColorSpaceCreateDeviceGray(),
                bitmapInfo: CGImageAlphaInfo.none.rawValue
            ) else {
                return false
            }
            context.interpolationQuality = .high
            context.draw(
                image,
                in: CGRect(x: 0, y: 0, width: width, height: height)
            )
            return true
        }
        return rendered ? pixels : nil
    }
}

final class LongScreenshotPanel: NSPanel {
    var onPrimaryAction: (() -> Void)?
    var onUndo: (() -> Void)?
    var onFinish: (() -> Void)?
    var onCancel: (() -> Void)?

    private let sectionLabel = NSTextField(labelWithString: "")
    private let statusLabel = NSTextField(labelWithString: "")
    private let errorLabel = NSTextField(labelWithString: "")
    private let primaryButton = NSButton()
    private let undoButton = NSButton()
    private let finishButton = NSButton()
    private let cancelButton = NSButton()
    private let previewImageView = NSImageView()
    private let previewScrollView = NSScrollView()
    private var isRecordingState = false

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
            if isRecordingState {
                onPrimaryAction?()
            } else {
                onCancel?()
            }
        case 36, 76, 49:
            onPrimaryAction?()
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

    func update(sectionCount: Int, isRecording: Bool, preview: CGImage?, error: String?) {
        isRecordingState = isRecording
        sectionLabel.stringValue = L10n.format("Sections: %d", sectionCount)
        statusLabel.stringValue = isRecording
            ? L10n.text("Recording — scroll to capture, Esc to stop")
            : L10n.text("Click Start, scroll the original app, then Stop.")
        errorLabel.stringValue = error ?? ""
        errorLabel.isHidden = error == nil
        primaryButton.title = isRecording
            ? L10n.text("Stop Recording")
            : L10n.text("Start Recording")
        primaryButton.image = NSImage(
            systemSymbolName: isRecording ? "stop.fill" : "play.fill",
            accessibilityDescription: primaryButton.title
        )?.withSymbolConfiguration(
            NSImage.SymbolConfiguration(pointSize: 11, weight: .semibold)
        )
        primaryButton.isEnabled = true
        undoButton.isEnabled = !isRecording && sectionCount > 1
        finishButton.isEnabled = !isRecording && sectionCount > 0
        cancelButton.isEnabled = true
        updatePreview(preview)
    }

    private func updatePreview(_ preview: CGImage?) {
        let clipView = previewScrollView.contentView
        let clipSize = clipView.bounds.size
        let oldDocumentHeight = previewImageView.frame.height
        let wasPinnedToBottom = oldDocumentHeight <= clipSize.height
            || (oldDocumentHeight - clipSize.height - clipView.bounds.minY) <= 2
        previewImageView.image = preview.map {
            NSImage(
                cgImage: $0,
                size: NSSize(width: $0.width, height: $0.height)
            )
        }
        let size = preview.map {
            NSSize(width: $0.width, height: $0.height)
        } ?? .zero

        var origin = NSPoint.zero
        if size.width < clipSize.width {
            origin.x = (clipSize.width - size.width) / 2
        }
        if size.height < clipSize.height {
            origin.y = (clipSize.height - size.height) / 2
        }
        previewImageView.frame = NSRect(origin: origin, size: size)

        if size.height > clipSize.height, wasPinnedToBottom {
            clipView.setBoundsOrigin(NSPoint(x: 0, y: size.height - clipSize.height))
        } else {
            clipView.setBoundsOrigin(.zero)
        }
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
        sectionLabel.setContentCompressionResistancePriority(.required, for: .horizontal)
        sectionLabel.setContentHuggingPriority(.defaultLow, for: .horizontal)

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

        previewImageView.imageScaling = .scaleNone
        previewImageView.translatesAutoresizingMaskIntoConstraints = false

        previewScrollView.translatesAutoresizingMaskIntoConstraints = false
        previewScrollView.hasVerticalScroller = true
        previewScrollView.hasHorizontalScroller = false
        previewScrollView.autohidesScrollers = true
        previewScrollView.drawsBackground = true
        previewScrollView.backgroundColor = NSColor(calibratedWhite: 0.15, alpha: 1)
        previewScrollView.wantsLayer = true
        previewScrollView.layer?.cornerRadius = 8
        previewScrollView.layer?.cornerCurve = .continuous
        previewScrollView.layer?.masksToBounds = true
        previewScrollView.documentView = previewImageView

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
            primaryButton,
            title: L10n.text("Start Recording"),
            symbol: "play.fill",
            primary: true,
            action: #selector(primaryAction)
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

        let rows = NSStackView(views: [
            headingRow,
            previewScrollView,
            statusLabel,
            errorLabel,
            buttonRow,
            finishRow
        ])
        rows.orientation = .vertical
        rows.alignment = .width
        rows.spacing = 8
        rows.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(rows)
        NSLayoutConstraint.activate([
            rows.topAnchor.constraint(equalTo: content.topAnchor),
            rows.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            rows.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            rows.bottomAnchor.constraint(equalTo: content.bottomAnchor),
            headingRow.heightAnchor.constraint(equalToConstant: 20),
            headingRow.widthAnchor.constraint(equalTo: rows.widthAnchor),
            previewScrollView.heightAnchor.constraint(equalToConstant: 240),
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

    @objc private func primaryAction() {
        onPrimaryAction?()
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
