import AppKit
import ApplicationServices
import Carbon.HIToolbox
import Combine
import KiriCore

@MainActor
final class AppModel: ObservableObject {
    @Published private(set) var assets: [CaptureAsset] = []
    @Published private(set) var hasLoadedLibrary = false
    @Published private(set) var libraryRevision = 0
    @Published private(set) var isCaptureStarting = false
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
    private var editorController: EditorWindowController?
    private var pinnedControllers: [UUID: PinnedImageController] = [:]
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

    var capturePermissionRecoveryLabel: String? {
        switch capturePermissionRecoveryAction {
        case .openSettings:
            "Open Settings"
        case .quitKiri:
            "Quit Kiri"
        case .openAccessibilitySettings:
            "Open Accessibility Settings"
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
            if case .accessibilityPermissionRequired = error {
                capturePermissionRecoveryAction = .openAccessibilitySettings
            }
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func startCapture() {
        guard overlayController == nil, !isCaptureStarting else { return }
        isCaptureStarting = true
        errorMessage = nil
        let frontmostApplication = NSWorkspace.shared.frontmostApplication
        let isKiriFrontmost = frontmostApplication?.processIdentifier
            == ProcessInfo.processInfo.processIdentifier
        let sourceApplication = isKiriFrontmost ? nil : frontmostApplication?.localizedName
        let hiddenWindows = hideCaptureOriginWindowsIfNeeded(frontmostApplication)
        Task {
            defer { isCaptureStarting = false }
            do {
                if !hiddenWindows.isEmpty {
                    try? await Task.sleep(for: .milliseconds(120))
                }
                let capture = try await captureCoordinator.captureActiveDisplay()
                let controller = SelectionOverlayController(capture: capture)
                overlayController = controller
                controller.present(
                    onComplete: { [weak self] image, action in
                        self?.overlayController = nil
                        self?.restoreCaptureOriginWindows(hiddenWindows)
                        self?.completeCapture(
                            image: image,
                            action: action,
                            sourceApplication: sourceApplication
                        )
                    },
                    onCancel: { [weak self] in
                        self?.overlayController = nil
                        self?.restoreCaptureOriginWindows(hiddenWindows)
                    }
                )
            } catch let error as CaptureCoordinatorError {
                restoreCaptureOriginWindows(hiddenWindows)
                handleCaptureCoordinatorError(error)
            } catch {
                restoreCaptureOriginWindows(hiddenWindows)
                errorMessage = error.localizedDescription
            }
        }
    }

    private func hideCaptureOriginWindowsIfNeeded(
        _ frontmostApplication: NSRunningApplication?
    ) -> [NSWindow] {
        guard frontmostApplication?.processIdentifier == ProcessInfo.processInfo.processIdentifier else {
            return []
        }
        let windows = NSApplication.shared.windows.filter { window in
            window.isVisible && window.level == .normal && window.styleMask.contains(.titled)
        }
        windows.forEach { $0.orderOut(nil) }
        return windows
    }

    private func restoreCaptureOriginWindows(_ windows: [NSWindow]) {
        guard !windows.isEmpty else { return }
        windows.forEach { $0.orderFront(nil) }
        NSApplication.shared.activate(ignoringOtherApps: true)
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
                showNotice(title: "Moved to Trash", symbol: "trash")
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
                showNotice(title: "Restored to Library", symbol: "arrow.uturn.backward")
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
                showNotice(title: "Deleted Permanently", symbol: "trash.fill")
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    func copy(_ asset: CaptureAsset) {
        guard let image = NSImage(contentsOf: assetFileURL(asset)) else {
            errorMessage = "The capture file is unavailable."
            return
        }
        if !writeToClipboard(image) {
            errorMessage = "Could not copy the capture."
        } else {
            showNotice(title: "Copied to Clipboard", symbol: "checkmark.circle.fill")
        }
    }

    func open(_ asset: CaptureAsset) {
        NSWorkspace.shared.open(assetFileURL(asset))
    }

    func reveal(_ asset: CaptureAsset) {
        NSWorkspace.shared.activateFileViewerSelecting([assetFileURL(asset)])
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
                showNotice(title: "Copied to Clipboard", symbol: "checkmark.circle.fill")
            }
        }

        Task {
            guard let data = await Task.detached(priority: .utility, operation: {
                Self.pngData(for: image)
            }).value else {
                errorMessage = "Could not encode the capture as PNG."
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
                errorMessage = "Could not encode the capture as PNG."
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
                        showNotice(title: "Copied to Clipboard", symbol: "checkmark.circle.fill")
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
            showNotice(title: "Saved", symbol: "checkmark.circle.fill")
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
            return (fallback, library, "Using a temporary library: \(error.localizedDescription)")
        }
    }

}

enum CapturePermissionRecoveryAction {
    case openSettings
    case quitKiri
    case openAccessibilitySettings
}

struct AppNotice: Identifiable, Equatable {
    let id = UUID()
    let title: String
    let symbol: String
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
        "Could not copy the capture to the clipboard."
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

        let options = ["AXTrustedCheckOptionPrompt": true] as CFDictionary
        guard AXIsProcessTrustedWithOptions(options) else {
            throw GlobalShortcutError.accessibilityPermissionRequired
        }

        let mask = (CGEventMask(1) << CGEventType.keyDown.rawValue)
            | (CGEventMask(1) << CGEventType.keyUp.rawValue)
        guard let eventTap = CGEvent.tapCreate(
            tap: .cgSessionEventTap,
            place: .headInsertEventTap,
            options: .defaultTap,
            eventsOfInterest: mask,
            callback: globalShortcutEventTapCallback,
            userInfo: Unmanaged.passUnretained(self).toOpaque()
        ) else {
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
    case eventTapCreationFailed

    var errorDescription: String? {
        switch self {
        case .accessibilityPermissionRequired:
            "Enable Kiri in Accessibility settings, then quit and reopen it to reserve ⇧⌘A exclusively."
        case .eventTapCreationFailed:
            "Kiri could not create the exclusive ⇧⌘A keyboard filter. Check Accessibility settings, then quit and reopen Kiri."
        }
    }
}
