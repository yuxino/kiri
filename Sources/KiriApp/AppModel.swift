import AppKit
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
    @Published private(set) var captureShortcutPreset: CaptureShortcutPreset

    let libraryRoot: URL

    private let library: AssetLibrary
    private let captureCoordinator = CaptureCoordinator()
    private let shortcutMonitor = GlobalShortcutMonitor()
    private var overlayController: SelectionOverlayController?
    private var editorController: EditorWindowController?
    private var pinnedControllers: [UUID: PinnedImageController] = [:]
    private var hasStarted = false

    init() {
        captureShortcutPreset = UserDefaults.standard
            .string(forKey: Self.shortcutDefaultsKey)
            .flatMap(CaptureShortcutPreset.init(storedIdentifier:))
            ?? .optionCommand2
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
        captureShortcutPreset.shortcut.displayLabel
    }

    var capturePermissionRecoveryLabel: String? {
        switch capturePermissionRecoveryAction {
        case .openSettings:
            "Open Settings"
        case .quitKiri:
            "Quit Kiri"
        case nil:
            nil
        }
    }

    func start() {
        guard !hasStarted else { return }
        hasStarted = true
        do {
            try registerShortcut(captureShortcutPreset)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func selectShortcut(_ preset: CaptureShortcutPreset) {
        guard preset != captureShortcutPreset else { return }
        do {
            if hasStarted {
                try registerShortcut(preset)
            }
            captureShortcutPreset = preset
            UserDefaults.standard.set(
                preset.rawValue,
                forKey: Self.shortcutDefaultsKey
            )
            errorMessage = nil
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

        guard let data = pngData(for: image) else {
            errorMessage = "Could not encode the capture as PNG."
            return
        }

        Task {
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
        guard let data = pngData(for: image) else {
            errorMessage = "Could not encode the capture as PNG."
            return
        }

        Task {
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

    private func pngData(for image: CGImage) -> Data? {
        NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])
    }

    private static func nsImage(from image: CGImage) -> NSImage {
        NSImage(
            cgImage: image,
            size: NSSize(width: image.width, height: image.height)
        )
    }

    private func registerShortcut(_ preset: CaptureShortcutPreset) throws {
        try shortcutMonitor.start(shortcut: preset.shortcut) { [weak self] in
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

    private static let shortcutDefaultsKey = "captureShortcutPreset"
}

enum CapturePermissionRecoveryAction {
    case openSettings
    case quitKiri
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
    private static let hotKeyID = EventHotKeyID(signature: 0x4B49_5249, id: 1)

    private var eventHandler: EventHandlerRef?
    private var hotKey: EventHotKeyRef?
    private var activeShortcut: CaptureShortcut?
    private var action: (@MainActor () -> Void)?

    func start(
        shortcut: CaptureShortcut,
        action: @escaping @MainActor () -> Void
    ) throws {
        if activeShortcut == shortcut, hotKey != nil {
            self.action = action
            return
        }

        try installEventHandlerIfNeeded()

        var newHotKey: EventHotKeyRef?
        let status = RegisterEventHotKey(
            try Self.keyCode(for: shortcut.key),
            Self.modifierFlags(for: shortcut.modifiers),
            Self.hotKeyID,
            GetApplicationEventTarget(),
            OptionBits(kEventHotKeyExclusive),
            &newHotKey
        )
        guard status == noErr, let newHotKey else {
            if status == eventHotKeyExistsErr {
                throw GlobalShortcutError.alreadyInUse(shortcut.displayLabel)
            }
            throw GlobalShortcutError.registrationFailed(
                shortcut.displayLabel,
                status
            )
        }

        let previousHotKey = hotKey
        hotKey = newHotKey
        activeShortcut = shortcut
        self.action = action
        if let previousHotKey {
            UnregisterEventHotKey(previousHotKey)
        }
    }

    private func installEventHandlerIfNeeded() throws {
        guard eventHandler == nil else { return }
        var eventType = EventTypeSpec(
            eventClass: OSType(kEventClassKeyboard),
            eventKind: UInt32(kEventHotKeyPressed)
        )
        let context = Unmanaged.passUnretained(self).toOpaque()
        let status = InstallEventHandler(
            GetApplicationEventTarget(),
            Self.handleEvent,
            1,
            &eventType,
            context,
            &eventHandler
        )
        guard status == noErr else {
            throw GlobalShortcutError.eventHandlerInstallationFailed(status)
        }
    }

    private func performAction() {
        action?()
    }

    private static let handleEvent: EventHandlerUPP = {
        _,
        event,
        context in
        guard let event, let context else {
            return OSStatus(eventNotHandledErr)
        }

        var hotKeyID = EventHotKeyID()
        let status = GetEventParameter(
            event,
            EventParamName(kEventParamDirectObject),
            EventParamType(typeEventHotKeyID),
            nil,
            MemoryLayout<EventHotKeyID>.size,
            nil,
            &hotKeyID
        )
        guard status == noErr,
              hotKeyID.signature == GlobalShortcutMonitor.hotKeyID.signature,
              hotKeyID.id == GlobalShortcutMonitor.hotKeyID.id else {
            return OSStatus(eventNotHandledErr)
        }

        let monitor = Unmanaged<GlobalShortcutMonitor>
            .fromOpaque(context)
            .takeUnretainedValue()
        Task { @MainActor in
            monitor.performAction()
        }
        return noErr
    }

    private static func keyCode(for key: String) throws -> UInt32 {
        switch key.uppercased() {
        case "A":
            UInt32(kVK_ANSI_A)
        case "2":
            UInt32(kVK_ANSI_2)
        default:
            throw GlobalShortcutError.unsupportedKey(key)
        }
    }

    private static func modifierFlags(
        for modifiers: Set<CaptureShortcutModifier>
    ) -> UInt32 {
        modifiers.reduce(into: UInt32(0)) { flags, modifier in
            switch modifier {
            case .control: flags |= UInt32(controlKey)
            case .option: flags |= UInt32(optionKey)
            case .shift: flags |= UInt32(shiftKey)
            case .command: flags |= UInt32(cmdKey)
            }
        }
    }
}

private enum GlobalShortcutError: LocalizedError {
    case alreadyInUse(String)
    case eventHandlerInstallationFailed(OSStatus)
    case registrationFailed(String, OSStatus)
    case unsupportedKey(String)

    var errorDescription: String? {
        switch self {
        case let .alreadyInUse(label):
            "The \(label) shortcut is already in use by another app."
        case let .eventHandlerInstallationFailed(status):
            "Could not install the global shortcut handler (error \(status))."
        case let .registrationFailed(label, status):
            "Could not reserve the \(label) shortcut (error \(status))."
        case let .unsupportedKey(key):
            "The \(key.uppercased()) key is not supported as a capture shortcut."
        }
    }
}
