import AppKit
import ApplicationServices
@preconcurrency import AVFoundation
import Carbon.HIToolbox
import Combine
import KiriCore

@MainActor
final class AppModel: ObservableObject {
    @Published private(set) var assets: [CaptureAsset] = []
    @Published private(set) var hasLoadedLibrary = false
    @Published private(set) var libraryRevision = 0
    @Published private(set) var isCaptureStarting = false
    @Published private(set) var isRecordingStarting = false
    @Published private(set) var isRecording = false
    @Published private(set) var isRecordingPaused = false
    @Published private(set) var isRecordingTransitioning = false
    @Published private(set) var isRecordingFinalizing = false
    @Published private(set) var recordingElapsed: TimeInterval = 0
    @Published private(set) var gifConversionAssetIDs: Set<UUID> = []
    @Published private(set) var notice: AppNotice?
    @Published var searchQuery = ""
    @Published var showingTrash = false
    @Published var errorMessage: String? {
        didSet {
            if errorMessage != oldValue {
                capturePermissionRecoveryAction = nil
            }
        }
    }
    @Published private(set) var capturePermissionRecoveryAction: CapturePermissionRecoveryAction?

    let libraryRoot: URL

    private let library: AssetLibrary
    private let captureCoordinator = CaptureCoordinator()
    private let shortcutMonitor = GlobalShortcutMonitor()
    private var overlayController: SelectionOverlayController?
    private var longScreenshotController: LongScreenshotCaptureController?
    private var editorController: EditorWindowController?
    private var pinnedControllers: [UUID: PinnedImageController] = [:]
    private var regionRecorder: RegionRecorder?
    private var recordingCountdownController: RecordingCountdownController?
    private var recordingControlPanelController: RecordingControlPanelController?
    private var recordingClickHighlighterController: RecordingClickHighlighterController?
    private var recordingClockTask: Task<Void, Never>?
    private var recordingStartedAt: Date?
    private var recordingElapsedBeforeCurrentSegment: TimeInterval = 0
    private var recordingConfiguration: RegionRecordingConfiguration?
    private var recordingSegments: [RecordedMedia] = []
    private var recordingSourceApplication: String?
    private var recordingReturnApplication: NSRunningApplication?
    private var hasStarted = false

    init() {
        let setup = Self.makeLibrary()
        libraryRoot = setup.root
        library = setup.library
        errorMessage = setup.warning
    }

    var filteredAssets: [CaptureAsset] {
        let query = searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return assets.filter { asset in
            let stateMatches = showingTrash ? asset.trashedAt != nil : asset.trashedAt == nil
            return stateMatches && (query.isEmpty || asset.searchableText.contains(query))
        }
    }

    var captureShortcutLabel: String {
        CaptureShortcut.kiriCapture.displayLabel
    }

    var recordingElapsedLabel: String {
        RecordingPolicy.elapsedLabel(recordingElapsed)
    }

    var hasRecordingSession: Bool {
        isRecording || isRecordingPaused || isRecordingTransitioning
    }

    var captureIsUnavailable: Bool {
        isCaptureStarting
            || isRecordingStarting
            || isRecording
            || isRecordingPaused
            || isRecordingTransitioning
            || isRecordingFinalizing
            || longScreenshotController != nil
    }

    var capturePermissionRecoveryLabel: String? {
        switch capturePermissionRecoveryAction {
        case .openSettings:
            L10n.text("Open Settings")
        case .quitKiri:
            L10n.text("Quit Kiri")
        case .openAccessibilitySettings:
            L10n.text("Open Accessibility Settings")
        case .openInputMonitoringSettings:
            L10n.text("Open Input Monitoring Settings")
        case .openMicrophoneSettings:
            L10n.text("Open Microphone Settings")
        case nil:
            nil
        }
    }

    func start() {
        guard !hasStarted else { return }
        hasStarted = true
        do {
            try registerShortcut()
        } catch let error as GlobalShortcutError {
            errorMessage = error.localizedDescription
            switch error {
            case .accessibilityPermissionRequired:
                capturePermissionRecoveryAction = .openAccessibilitySettings
            case .inputMonitoringPermissionRequired:
                capturePermissionRecoveryAction = .openInputMonitoringSettings
            case .eventTapCreationFailed:
                break
            }
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func startCapture() {
        guard overlayController == nil, !captureIsUnavailable else { return }
        isCaptureStarting = true
        errorMessage = nil
        let initialFrontmostApplication = NSWorkspace.shared.frontmostApplication
        let isKiriFrontmost = initialFrontmostApplication?.processIdentifier
            == ProcessInfo.processInfo.processIdentifier
        let hiddenWindows = hideKiriLibraryWindows()
        Task {
            defer { isCaptureStarting = false }
            do {
                let returnApplication = await resolveCaptureReturnApplication(
                    initialFrontmostApplication,
                    wasKiriFrontmost: isKiriFrontmost
                )
                let sourceApplication = returnApplication?.localizedName
                if !hiddenWindows.isEmpty {
                    try? await Task.sleep(for: .milliseconds(120))
                }
                let capture = try await captureCoordinator.captureActiveDisplay()
                let controller = SelectionOverlayController(capture: capture)
                overlayController = controller
                controller.present(
                    onComplete: { [weak self] image, action in
                        self?.overlayController = nil
                        self?.finishCapturePresentation(
                            returnApplication: returnApplication,
                            hiddenWindows: hiddenWindows,
                            action: action
                        )
                        self?.completeCapture(
                            image: image,
                            action: action,
                            sourceApplication: sourceApplication
                        )
                    },
                    onRecord: { [weak self] region, options in
                        self?.overlayController = nil
                        self?.keepKiriLibraryHidden(hiddenWindows)
                        self?.activate(returnApplication)
                        self?.beginRegionRecording(
                            capture: capture,
                            region: region,
                            options: options,
                            sourceApplication: sourceApplication,
                            returnApplication: returnApplication
                        )
                    },
                    onRecognizeText: { [weak self] text in
                        self?.overlayController = nil
                        self?.keepKiriLibraryHidden(hiddenWindows)
                        self?.activate(returnApplication)
                        self?.copyRecognizedText(text)
                    },
                    onLongScreenshot: { [weak self] region, initialSection in
                        self?.overlayController = nil
                        self?.keepKiriLibraryHidden(hiddenWindows)
                        self?.activate(returnApplication)
                        self?.beginLongScreenshot(
                            capture: capture,
                            region: region,
                            initialSection: initialSection,
                            initialApplication: initialFrontmostApplication,
                            returnApplication: returnApplication,
                            hiddenWindows: hiddenWindows,
                            sourceApplication: sourceApplication
                        )
                    },
                    onCancel: { [weak self] in
                        self?.overlayController = nil
                        self?.cancelCapturePresentation(
                            initialApplication: initialFrontmostApplication,
                            returnApplication: returnApplication,
                            hiddenWindows: hiddenWindows
                        )
                    }
                )
            } catch let error as CaptureCoordinatorError {
                cancelCapturePresentation(
                    initialApplication: initialFrontmostApplication,
                    returnApplication: initialFrontmostApplication,
                    hiddenWindows: hiddenWindows
                )
                handleCaptureCoordinatorError(error)
            } catch {
                cancelCapturePresentation(
                    initialApplication: initialFrontmostApplication,
                    returnApplication: initialFrontmostApplication,
                    hiddenWindows: hiddenWindows
                )
                errorMessage = error.localizedDescription
            }
        }
    }

    private func beginRegionRecording(
        capture: CapturedDisplay,
        region: CGRect,
        options: RecordingOptions,
        sourceApplication: String?,
        returnApplication: NSRunningApplication?
    ) {
        guard regionRecorder == nil, !captureIsUnavailable else { return }
        recordingSourceApplication = sourceApplication
        recordingReturnApplication = returnApplication
        isRecordingStarting = true
        errorMessage = nil
        Task {
            do {
                var effectiveOptions = options.normalized
                if #unavailable(macOS 15.0) {
                    effectiveOptions.capturesMicrophone = false
                }
                if effectiveOptions.capturesMicrophone {
                    try await ensureMicrophonePermission()
                }
                if effectiveOptions.usesCountdown {
                    let countdown = RecordingCountdownController()
                    recordingCountdownController = countdown
                    let shouldStart = await countdown.run(
                        screenFrame: capture.screenFrame,
                        region: region
                    )
                    if recordingCountdownController === countdown {
                        recordingCountdownController = nil
                    }
                    guard shouldStart else {
                        isRecordingStarting = false
                        recordingSourceApplication = nil
                        return
                    }
                }

                let recorder = RegionRecorder()
                recordingConfiguration = RegionRecordingConfiguration(
                    displayID: capture.displayID,
                    sourceRect: region,
                    backingScale: capture.backingScale,
                    options: effectiveOptions,
                    screenFrame: capture.screenFrame
                )
                recordingSegments = []
                recordingElapsedBeforeCurrentSegment = 0
                prepareRecordingClickHighlighter(
                    screenFrame: capture.screenFrame,
                    region: region,
                    enabled: effectiveOptions.highlightsClicks
                )
                prepareRecordingControlPanel(screenFrame: capture.screenFrame)
                regionRecorder = recorder
                try await recorder.start(
                    displayID: capture.displayID,
                    sourceRect: region,
                    backingScale: capture.backingScale,
                    options: effectiveOptions,
                    exceptedWindowIDs: recordingClickHighlighterController?.exceptedWindowIDs ?? []
                )
                guard regionRecorder === recorder else { return }
                isRecordingStarting = false
                isRecording = true
                isRecordingPaused = false
                isRecordingTransitioning = false
                recordingElapsed = 0
                recordingStartedAt = Date()
                startRecordingClock()
                updateRecordingControlPanel()
                recordingClickHighlighterController?.setActive(true)
                activate(recordingReturnApplication)
                showNotice(title: L10n.text("Recording Started"), symbol: "record.circle.fill")
            } catch let error as RecordingAccessError {
                resetRecordingSession()
                errorMessage = error.localizedDescription
                capturePermissionRecoveryAction = .openMicrophoneSettings
            } catch {
                resetRecordingSession()
                errorMessage = error.localizedDescription
            }
        }
    }

    private func ensureMicrophonePermission() async throws {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized:
            return
        case .notDetermined:
            guard await AVCaptureDevice.requestAccess(for: .audio) else {
                throw RecordingAccessError.microphonePermissionDenied
            }
        case .denied, .restricted:
            throw RecordingAccessError.microphonePermissionDenied
        @unknown default:
            throw RecordingAccessError.microphonePermissionDenied
        }
    }

    func toggleRecordingPause() {
        if isRecordingPaused {
            resumeRecording()
        } else {
            pauseRecording()
        }
    }

    func pauseRecording() {
        guard isRecording,
              !isRecordingTransitioning,
              let recorder = regionRecorder else { return }
        isRecording = false
        isRecordingTransitioning = true
        recordingClickHighlighterController?.setActive(false)
        stopRecordingClock()
        updateRecordingControlPanel()
        Task {
            do {
                let media = try await recorder.stop()
                recordingSegments.append(media)
                recordingElapsedBeforeCurrentSegment = recordingSegments.reduce(0) {
                    $0 + $1.duration
                }
                recordingElapsed = recordingElapsedBeforeCurrentSegment
                if regionRecorder === recorder {
                    regionRecorder = nil
                }
                recordingStartedAt = nil
                isRecordingPaused = true
                isRecordingTransitioning = false
                updateRecordingControlPanel()
                showNotice(title: L10n.text("Recording Paused"), symbol: "pause.circle.fill")
            } catch {
                failRecordingSession(with: error)
            }
        }
    }

    func resumeRecording() {
        guard isRecordingPaused,
              !isRecordingTransitioning,
              let configuration = recordingConfiguration else { return }
        isRecordingTransitioning = true
        updateRecordingControlPanel()
        let recorder = RegionRecorder()
        regionRecorder = recorder
        Task {
            do {
                try await recorder.start(
                    displayID: configuration.displayID,
                    sourceRect: configuration.sourceRect,
                    backingScale: configuration.backingScale,
                    options: configuration.options,
                    exceptedWindowIDs: recordingClickHighlighterController?.exceptedWindowIDs ?? []
                )
                guard regionRecorder === recorder else { return }
                isRecordingPaused = false
                isRecordingTransitioning = false
                isRecording = true
                recordingStartedAt = Date()
                startRecordingClock()
                updateRecordingControlPanel()
                recordingClickHighlighterController?.setActive(true)
                activate(recordingReturnApplication)
                showNotice(title: L10n.text("Recording Resumed"), symbol: "play.circle.fill")
            } catch {
                if regionRecorder === recorder {
                    regionRecorder = nil
                }
                isRecordingTransitioning = false
                isRecordingPaused = true
                updateRecordingControlPanel()
                errorMessage = error.localizedDescription
            }
        }
    }

    func stopRecording() {
        guard (isRecording || isRecordingPaused), !isRecordingTransitioning else { return }
        let activeRecorder = isRecording ? regionRecorder : nil
        isRecording = false
        isRecordingPaused = false
        isRecordingFinalizing = true
        recordingClickHighlighterController?.setActive(false)
        stopRecordingClock()
        updateRecordingControlPanel()
        activate(recordingReturnApplication)
        Task {
            var segments = recordingSegments
            do {
                if let activeRecorder {
                    let media = try await activeRecorder.stop()
                    segments.append(media)
                }
                regionRecorder = nil
                let finalMedia = try await RecordingSegmentMerger.merge(segments)
                defer {
                    let temporaryURLs = Set(segments.map(\.fileURL) + [finalMedia.fileURL])
                    temporaryURLs.forEach { try? FileManager.default.removeItem(at: $0) }
                }
                _ = try await library.importFile(
                    at: finalMedia.fileURL,
                    kind: .video,
                    fileExtension: "mp4",
                    pixelWidth: finalMedia.pixelWidth,
                    pixelHeight: finalMedia.pixelHeight,
                    duration: finalMedia.duration,
                    sourceApplication: recordingSourceApplication
                )
                await refresh()
                showNotice(title: L10n.text("Recording Saved"), symbol: "video.fill")
            } catch {
                segments.forEach { try? FileManager.default.removeItem(at: $0.fileURL) }
                errorMessage = error.localizedDescription
            }
            resetRecordingSession()
        }
    }

    private func startRecordingClock() {
        recordingClockTask?.cancel()
        recordingClockTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(250))
                guard let self, let recordingStartedAt = self.recordingStartedAt else { return }
                self.recordingElapsed = self.recordingElapsedBeforeCurrentSegment
                    + Date().timeIntervalSince(recordingStartedAt)
                self.updateRecordingControlPanel()
            }
        }
    }

    private func stopRecordingClock() {
        recordingClockTask?.cancel()
        recordingClockTask = nil
    }

    private func prepareRecordingControlPanel(screenFrame: CGRect) {
        closeRecordingControlPanel()
        let controller = RecordingControlPanelController(
            onPauseResume: { [weak self] in self?.toggleRecordingPause() },
            onStop: { [weak self] in self?.stopRecording() }
        )
        recordingControlPanelController = controller
        controller.show(screenFrame: screenFrame)
        updateRecordingControlPanel()
    }

    private func updateRecordingControlPanel() {
        recordingControlPanelController?.update(
            elapsed: recordingElapsedLabel,
            isPaused: isRecordingPaused,
            isBusy: isRecordingStarting || isRecordingTransitioning || isRecordingFinalizing
        )
    }

    private func closeRecordingControlPanel() {
        recordingControlPanelController?.close()
        recordingControlPanelController = nil
    }

    private func prepareRecordingClickHighlighter(
        screenFrame: CGRect,
        region: CGRect,
        enabled: Bool
    ) {
        closeRecordingClickHighlighter()
        guard enabled else { return }
        let selectedFrame = CGRect(
            x: screenFrame.minX + region.minX,
            y: screenFrame.maxY - region.maxY,
            width: region.width,
            height: region.height
        ).standardized
        recordingClickHighlighterController = RecordingClickHighlighterController(
            anchorPoint: CGPoint(x: selectedFrame.midX, y: selectedFrame.midY)
        )
    }

    private func closeRecordingClickHighlighter() {
        recordingClickHighlighterController?.close()
        recordingClickHighlighterController = nil
    }

    private func failRecordingSession(with error: Error) {
        recordingSegments.forEach { try? FileManager.default.removeItem(at: $0.fileURL) }
        errorMessage = error.localizedDescription
        resetRecordingSession()
    }

    private func resetRecordingSession() {
        stopRecordingClock()
        regionRecorder = nil
        recordingCountdownController = nil
        closeRecordingControlPanel()
        closeRecordingClickHighlighter()
        isRecordingStarting = false
        isRecording = false
        isRecordingPaused = false
        isRecordingTransitioning = false
        isRecordingFinalizing = false
        recordingElapsed = 0
        recordingElapsedBeforeCurrentSegment = 0
        recordingStartedAt = nil
        recordingConfiguration = nil
        recordingSegments = []
        recordingSourceApplication = nil
        recordingReturnApplication = nil
    }

    private func hideKiriLibraryWindows() -> [NSWindow] {
        let windows = NSApplication.shared.windows.filter { window in
            window.isVisible && window.level == .normal && window.styleMask.contains(.titled)
        }
        windows.forEach { $0.orderOut(nil) }
        return windows
    }

    private func resolveCaptureReturnApplication(
        _ initialApplication: NSRunningApplication?,
        wasKiriFrontmost: Bool
    ) async -> NSRunningApplication? {
        guard wasKiriFrontmost else { return initialApplication }
        NSApplication.shared.hide(nil)
        try? await Task.sleep(for: .milliseconds(100))
        let application = NSWorkspace.shared.frontmostApplication
        NSApplication.shared.unhideWithoutActivation()
        guard application?.processIdentifier != ProcessInfo.processInfo.processIdentifier else {
            return nil
        }
        return application
    }

    private func finishCapturePresentation(
        returnApplication: NSRunningApplication?,
        hiddenWindows: [NSWindow],
        action: CaptureSessionAction
    ) {
        keepKiriLibraryHidden(hiddenWindows)
        if case .copy = action {
            activate(returnApplication)
        }
    }

    private func cancelCapturePresentation(
        initialApplication: NSRunningApplication?,
        returnApplication: NSRunningApplication?,
        hiddenWindows: [NSWindow]
    ) {
        let wasKiriFrontmost = initialApplication?.processIdentifier
            == ProcessInfo.processInfo.processIdentifier
        if wasKiriFrontmost {
            hiddenWindows.forEach { $0.orderFront(nil) }
            NSApplication.shared.activate(ignoringOtherApps: true)
        } else {
            keepKiriLibraryHidden(hiddenWindows)
            activate(returnApplication)
        }
    }

    private func beginLongScreenshot(
        capture: CapturedDisplay,
        region: CGRect,
        initialSection: CGImage,
        initialApplication: NSRunningApplication?,
        returnApplication: NSRunningApplication?,
        hiddenWindows: [NSWindow],
        sourceApplication: String?
    ) {
        guard longScreenshotController == nil else { return }
        guard region.width >= 16,
              region.height >= 16,
              initialSection.width >= 1,
              initialSection.height >= 1 else {
            cancelCapturePresentation(
                initialApplication: initialApplication,
                returnApplication: returnApplication,
                hiddenWindows: hiddenWindows
            )
            errorMessage = LongScreenshotCaptureError.selectionTooSmall.localizedDescription
            return
        }

        let controller = LongScreenshotCaptureController(
            captureCoordinator: captureCoordinator,
            initialSection: initialSection,
            region: region,
            displayFrame: capture.screenFrame,
            displayID: capture.displayID,
            onFinish: { [weak self] result in
                self?.finishLongScreenshot(
                    result: result,
                    returnApplication: returnApplication,
                    hiddenWindows: hiddenWindows,
                    sourceApplication: sourceApplication
                )
            },
            onCancel: { [weak self] in
                guard let self else { return }
                longScreenshotController = nil
                cancelCapturePresentation(
                    initialApplication: initialApplication,
                    returnApplication: returnApplication,
                    hiddenWindows: hiddenWindows
                )
            },
            onCaptureFailure: { [weak self] error in
                self?.handleLongScreenshotCaptureError(error)
            }
        )
        longScreenshotController = controller
        controller.show()
        activate(returnApplication)
    }

    private func finishLongScreenshot(
        result: LongScreenshotCaptureResult,
        returnApplication: NSRunningApplication?,
        hiddenWindows: [NSWindow],
        sourceApplication: String?
    ) {
        longScreenshotController = nil
        keepKiriLibraryHidden(hiddenWindows)
        activate(returnApplication)

        Task { [weak self] in
            guard let self else { return }
            do {
                let output = try await Task.detached(priority: .userInitiated) {
                    let stitched = try KiriCore.LongScreenshotStitcher.stitch(
                        result.sections,
                        overlaps: result.overlaps
                    )
                    guard let data = Self.pngData(for: stitched.image) else {
                        throw LongScreenshotExportError.couldNotEncode
                    }
                    return (stitched.image, data)
                }.value

                let imageObject = Self.nsImage(from: output.0)
                if !writeToClipboard(imageObject) {
                    errorMessage = CaptureExportError.clipboardWriteFailed.localizedDescription
                }
                _ = try await library.importData(
                    output.1,
                    kind: .longImage,
                    fileExtension: "png",
                    pixelWidth: output.0.width,
                    pixelHeight: output.0.height,
                    sourceApplication: sourceApplication
                )
                await refresh()
                showNotice(title: L10n.text("Long Screenshot Saved"), symbol: "rectangle.portrait.fill")
            } catch {
                errorMessage = longScreenshotErrorMessage(error)
            }
        }
    }

    private func handleLongScreenshotCaptureError(_ error: Error) {
        if let captureError = error as? CaptureCoordinatorError {
            handleCaptureCoordinatorError(captureError)
        } else {
            errorMessage = error.localizedDescription
        }
    }

    private func longScreenshotErrorMessage(_ error: Error) -> String {
        let description = error.localizedDescription
        let marker = String(reflecting: error).lowercased()
        if marker.contains("tootall")
            || marker.contains("outputheight")
            || description.localizedLowercase.contains("too tall") {
            return L10n.text("The long screenshot is too tall to export.")
        }
        if error is KiriCore.LongScreenshotStitcherError {
            return L10n.text("Could not stitch the long screenshot.")
        }
        if error is LongScreenshotExportError {
            return description
        }
        return description.isEmpty
            ? L10n.text("Could not stitch the long screenshot.")
            : description
    }

    private func keepKiriLibraryHidden(_ windows: [NSWindow]) {
        windows.forEach { $0.orderOut(nil) }
    }

    private func activate(_ application: NSRunningApplication?) {
        guard let application, !application.isTerminated else { return }
        application.activate(options: [])
    }

    func performCapturePermissionRecovery() {
        switch capturePermissionRecoveryAction {
        case .openSettings:
            guard let url = URL(
                string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
            ) else {
                return
            }
            NSWorkspace.shared.open(url)
        case .quitKiri:
            NSApplication.shared.terminate(nil)
        case .openAccessibilitySettings:
            guard let url = URL(
                string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
            ) else {
                return
            }
            NSWorkspace.shared.open(url)
        case .openInputMonitoringSettings:
            guard let url = URL(
                string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
            ) else {
                return
            }
            NSWorkspace.shared.open(url)
        case .openMicrophoneSettings:
            guard let url = URL(
                string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
            ) else {
                return
            }
            NSWorkspace.shared.open(url)
        case nil:
            break
        }
    }

    func refresh() async {
        assets = await library.allAssets(includeTrashed: true)
        libraryRevision &+= 1
        hasLoadedLibrary = true
    }

    func toggleFavorite(_ asset: CaptureAsset) {
        Task {
            do {
                try await library.setFavorite(!asset.isFavorite, id: asset.id)
                await refresh()
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    func moveToTrash(_ asset: CaptureAsset) {
        Task {
            do {
                try await library.moveToTrash(id: asset.id)
                await refresh()
                showNotice(title: L10n.text("Moved to Trash"), symbol: "trash")
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    func restore(_ asset: CaptureAsset) {
        Task {
            do {
                try await library.restore(id: asset.id)
                await refresh()
                showNotice(title: L10n.text("Restored to Library"), symbol: "arrow.uturn.backward")
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    func permanentlyDelete(_ asset: CaptureAsset) {
        Task {
            do {
                try await library.permanentlyDelete(id: asset.id)
                await refresh()
                showNotice(title: L10n.text("Deleted Permanently"), symbol: "trash.fill")
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    func emptyTrash() {
        Task {
            do {
                try await library.emptyTrash()
                await refresh()
                showNotice(title: L10n.text("Trash Emptied"), symbol: "trash.slash")
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    func copy(_ asset: CaptureAsset) {
        guard let image = NSImage(contentsOf: assetFileURL(asset)) else {
            errorMessage = L10n.text("The capture file is unavailable.")
            return
        }
        if !writeToClipboard(image) {
            errorMessage = L10n.text("Could not copy the capture.")
        } else {
            showNotice(title: L10n.text("Copied to Clipboard"), symbol: "checkmark.circle.fill")
        }
    }

    func open(_ asset: CaptureAsset) {
        NSWorkspace.shared.open(assetFileURL(asset))
    }

    func reveal(_ asset: CaptureAsset) {
        NSWorkspace.shared.activateFileViewerSelecting([assetFileURL(asset)])
    }

    func canConvertToGIF(_ asset: CaptureAsset) -> Bool {
        asset.kind == .video && RecordingPolicy.isGIFEligible(duration: asset.duration)
    }

    func isConvertingToGIF(_ asset: CaptureAsset) -> Bool {
        gifConversionAssetIDs.contains(asset.id)
    }

    func convertToGIF(_ asset: CaptureAsset) {
        guard canConvertToGIF(asset), !gifConversionAssetIDs.contains(asset.id) else { return }
        gifConversionAssetIDs.insert(asset.id)
        let sourceURL = assetFileURL(asset)
        Task {
            do {
                let exported = try await GIFExporter.export(videoAt: sourceURL)
                defer { try? FileManager.default.removeItem(at: exported.fileURL) }
                _ = try await library.importFile(
                    at: exported.fileURL,
                    kind: .gif,
                    fileExtension: "gif",
                    pixelWidth: exported.pixelWidth,
                    pixelHeight: exported.pixelHeight,
                    duration: exported.duration,
                    sourceApplication: asset.sourceApplication
                )
                await refresh()
                showNotice(title: L10n.text("GIF Created"), symbol: "sparkles.rectangle.stack")
            } catch {
                errorMessage = error.localizedDescription
            }
            gifConversionAssetIDs.remove(asset.id)
        }
    }

    func assetFileURL(_ asset: CaptureAsset) -> URL {
        libraryRoot
            .appendingPathComponent("Assets", isDirectory: true)
            .appendingPathComponent(asset.filename)
    }

    private func completeCapture(
        image: CGImage,
        action: CaptureSessionAction,
        sourceApplication: String?
    ) {
        if case .copy = action {
            let imageObject = Self.nsImage(from: image)
            if !writeToClipboard(imageObject) {
                errorMessage = CaptureExportError.clipboardWriteFailed.localizedDescription
            } else {
                showNotice(title: L10n.text("Copied to Clipboard"), symbol: "checkmark.circle.fill")
            }
        }

        Task {
            guard let data = await Task.detached(priority: .utility, operation: {
                Self.pngData(for: image)
            }).value else {
                errorMessage = L10n.text("Could not encode the capture as PNG.")
                return
            }
            do {
                let asset = try await library.importData(
                    data,
                    kind: .image,
                    fileExtension: "png",
                    pixelWidth: image.width,
                    pixelHeight: image.height,
                    sourceApplication: sourceApplication
                )
                let stored = StoredCapture(
                    asset: asset,
                    image: image,
                    data: data
                )
                await refresh()
                perform(action, on: stored)
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    private func handleCaptureCoordinatorError(_ error: CaptureCoordinatorError) {
        errorMessage = error.localizedDescription
        switch error {
        case .permissionRestartRequired:
            capturePermissionRecoveryAction = .quitKiri
        case .permissionSettingsRequired:
            capturePermissionRecoveryAction = .openSettings
        case .displayUnavailable:
            break
        }
    }

    private func perform(_ action: CaptureSessionAction, on stored: StoredCapture) {
        switch action {
        case .copy:
            break
        case .save:
            saveToChosenLocation(stored.data)
        case .pin:
            pin(stored.nsImage)
        case .edit:
            presentEditor(for: stored)
        }
    }

    private func presentEditor(for stored: StoredCapture) {
        let controller = EditorWindowController(
            image: stored.image,
            completion: { [weak self] rendered, copy, saveURL in
                self?.updateStoredCapture(
                    stored,
                    with: rendered,
                    copyToClipboard: copy,
                    saveURL: saveURL
                )
            },
            onClose: { [weak self] in
                self?.editorController = nil
            }
        )
        editorController = controller
        controller.showWindow(nil)
        NSApplication.shared.activate(ignoringOtherApps: true)
    }

    private func updateStoredCapture(
        _ stored: StoredCapture,
        with image: CGImage,
        copyToClipboard: Bool,
        saveURL: URL?
    ) {
        Task {
            guard let data = await Task.detached(priority: .utility, operation: {
                Self.pngData(for: image)
            }).value else {
                errorMessage = L10n.text("Could not encode the capture as PNG.")
                return
            }
            do {
                _ = try await library.replaceData(data, for: stored.asset.id)
                if let saveURL {
                    try data.write(to: saveURL, options: [.atomic])
                }
                if copyToClipboard {
                    let imageObject = Self.nsImage(from: image)
                    if !writeToClipboard(imageObject) {
                        errorMessage = CaptureExportError.clipboardWriteFailed.localizedDescription
                    } else {
                        showNotice(title: L10n.text("Copied to Clipboard"), symbol: "checkmark.circle.fill")
                    }
                }
                await refresh()
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    private func saveToChosenLocation(_ data: Data) {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.png]
        panel.nameFieldStringValue = "kiri-\(CaptureFilename.timestamp()).png"
        NSApplication.shared.activate(ignoringOtherApps: true)
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            try data.write(to: url, options: [.atomic])
            showNotice(title: L10n.text("Saved"), symbol: "checkmark.circle.fill")
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func dismissNotice() {
        notice = nil
    }

    private func showNotice(title: String, symbol: String) {
        let nextNotice = AppNotice(title: title, symbol: symbol)
        notice = nextNotice
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(2))
            guard self?.notice?.id == nextNotice.id else { return }
            self?.notice = nil
        }
    }

    private func pin(_ image: NSImage) {
        let id = UUID()
        let controller = PinnedImageController()
        controller.onClose = { [weak self] in
            self?.pinnedControllers[id] = nil
        }
        pinnedControllers[id] = controller
        controller.show(image: image)
    }

    private func writeToClipboard(_ image: NSImage) -> Bool {
        NSPasteboard.general.clearContents()
        return NSPasteboard.general.writeObjects([image])
    }

    private func writeToClipboard(_ text: String) -> Bool {
        NSPasteboard.general.clearContents()
        return NSPasteboard.general.setString(text, forType: .string)
    }

    private func copyRecognizedText(_ text: String) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        if writeToClipboard(text) {
            showNotice(title: L10n.text("Text Copied"), symbol: "doc.on.clipboard.fill")
        } else {
            errorMessage = CaptureExportError.clipboardWriteFailed.localizedDescription
        }
    }

    nonisolated private static func pngData(for image: CGImage) -> Data? {
        NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])
    }

    private static func nsImage(from image: CGImage) -> NSImage {
        NSImage(
            cgImage: image,
            size: NSSize(width: image.width, height: image.height)
        )
    }

    private func registerShortcut() throws {
        try shortcutMonitor.start(shortcut: .kiriCapture) { [weak self] in
            self?.startCapture()
        }
    }

    private static func makeLibrary() -> (root: URL, library: AssetLibrary, warning: String?) {
        do {
#if DEBUG
            if let override = ProcessInfo.processInfo.environment["KIRI_LIBRARY_ROOT"],
               !override.isEmpty {
                let root = URL(fileURLWithPath: override, isDirectory: true)
                return (root, try AssetLibrary(rootURL: root), nil)
            }
#endif
            let root = try AssetLibrary.defaultRootURL()
            return (root, try AssetLibrary(rootURL: root), nil)
        } catch {
            let fallback = FileManager.default.temporaryDirectory
                .appendingPathComponent("kiri-library", isDirectory: true)
            guard let library = try? AssetLibrary(rootURL: fallback) else {
                preconditionFailure("kiri could not create its local capture library")
            }
            return (
                fallback,
                library,
                L10n.format("Using a temporary library: %@", error.localizedDescription)
            )
        }
    }

}

enum CapturePermissionRecoveryAction {
    case openSettings
    case quitKiri
    case openAccessibilitySettings
    case openInputMonitoringSettings
    case openMicrophoneSettings
}

enum RecordingAccessError: LocalizedError {
    case microphonePermissionDenied

    var errorDescription: String? {
        L10n.text("Microphone access is off. Enable it in System Settings to record your voice.")
    }
}

struct AppNotice: Identifiable, Equatable {
    let id = UUID()
    let title: String
    let symbol: String
}

private struct RegionRecordingConfiguration {
    let displayID: CGDirectDisplayID
    let sourceRect: CGRect
    let backingScale: CGFloat
    let options: RecordingOptions
    let screenFrame: CGRect
}

private struct StoredCapture {
    let asset: CaptureAsset
    let image: CGImage
    let data: Data

    var nsImage: NSImage {
        NSImage(
            cgImage: image,
            size: NSSize(width: image.width, height: image.height)
        )
    }
}

private enum CaptureExportError: LocalizedError {
    case clipboardWriteFailed

    var errorDescription: String? {
        L10n.text("Could not copy the capture to the clipboard.")
    }
}

private enum LongScreenshotExportError: LocalizedError {
    case couldNotEncode

    var errorDescription: String? {
        L10n.text("Could not encode the long screenshot as PNG.")
    }
}

@MainActor
private final class GlobalShortcutMonitor {
    private var eventTap: CFMachPort?
    private var runLoopSource: CFRunLoopSource?
    private var action: (@MainActor () -> Void)?

    func start(
        shortcut: CaptureShortcut,
        action: @escaping @MainActor () -> Void
    ) throws {
        precondition(shortcut == .kiriCapture)
        if eventTap != nil {
            self.action = action
            return
        }

        guard CGPreflightListenEventAccess() || CGRequestListenEventAccess() else {
            throw GlobalShortcutError.inputMonitoringPermissionRequired
        }

        let mask = (CGEventMask(1) << CGEventType.keyDown.rawValue)
            | (CGEventMask(1) << CGEventType.keyUp.rawValue)
        let eventTap = CGEvent.tapCreate(
            tap: .cgSessionEventTap,
            place: .headInsertEventTap,
            options: .defaultTap,
            eventsOfInterest: mask,
            callback: globalShortcutEventTapCallback,
            userInfo: Unmanaged.passUnretained(self).toOpaque()
        )
        guard let eventTap else {
            let options = ["AXTrustedCheckOptionPrompt": true] as CFDictionary
            guard AXIsProcessTrustedWithOptions(options) else {
                throw GlobalShortcutError.accessibilityPermissionRequired
            }
            throw GlobalShortcutError.eventTapCreationFailed
        }
        let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, eventTap, 0)
        CFRunLoopAddSource(CFRunLoopGetMain(), source, .commonModes)
        CGEvent.tapEnable(tap: eventTap, enable: true)

        self.eventTap = eventTap
        runLoopSource = source
        self.action = action
    }

    fileprivate func performAction() {
        action?()
    }

    fileprivate func reenableEventTap() {
        guard let eventTap else { return }
        CGEvent.tapEnable(tap: eventTap, enable: true)
    }
}

private func isKiriCaptureEvent(_ event: CGEvent) -> Bool {
    guard event.getIntegerValueField(.keyboardEventKeycode)
        == Int64(kVK_ANSI_A) else {
        return false
    }
    let modifierMask: CGEventFlags = [
        .maskCommand,
        .maskShift,
        .maskControl,
        .maskAlternate
    ]
    return event.flags.intersection(modifierMask) == [.maskCommand, .maskShift]
}

private func globalShortcutEventTapCallback(
    _: CGEventTapProxy,
    type: CGEventType,
    event: CGEvent,
    context: UnsafeMutableRawPointer?
) -> Unmanaged<CGEvent>? {
    guard let context else {
        return Unmanaged.passUnretained(event)
    }

    let monitor = Unmanaged<GlobalShortcutMonitor>
        .fromOpaque(context)
        .takeUnretainedValue()
    if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
        Task { @MainActor in
            monitor.reenableEventTap()
        }
        return Unmanaged.passUnretained(event)
    }
    guard isKiriCaptureEvent(event) else {
        return Unmanaged.passUnretained(event)
    }

    if type == .keyDown,
       event.getIntegerValueField(.keyboardEventAutorepeat) == 0 {
        Task { @MainActor in
            monitor.performAction()
        }
    }
    return nil
}

private enum GlobalShortcutError: LocalizedError {
    case accessibilityPermissionRequired
    case inputMonitoringPermissionRequired
    case eventTapCreationFailed

    var errorDescription: String? {
        switch self {
        case .accessibilityPermissionRequired:
            L10n.text("Enable Kiri in Accessibility settings, then quit and reopen it to reserve ⇧⌘A exclusively.")
        case .inputMonitoringPermissionRequired:
            L10n.text("Enable Kiri in Input Monitoring settings, then quit and reopen it to reserve ⇧⌘A exclusively.")
        case .eventTapCreationFailed:
            L10n.text("Kiri could not create the exclusive ⇧⌘A keyboard filter. Check Input Monitoring and Accessibility, then quit and reopen Kiri.")
        }
    }
}
