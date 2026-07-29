import AppKit
import Combine
import KiriCore

@MainActor
final class AppModel: ObservableObject {
    @Published private(set) var assets: [CaptureAsset] = []
    @Published var searchQuery = ""
    @Published var showingTrash = false
    @Published var errorMessage: String?

    let libraryRoot: URL

    private let library: AssetLibrary
    private let captureCoordinator = CaptureCoordinator()
    private let shortcutMonitor = GlobalShortcutMonitor()
    private var overlayController: SelectionOverlayController?
    private var editorController: EditorWindowController?
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

    func start() {
        guard !hasStarted else { return }
        hasStarted = true
        shortcutMonitor.start { [weak self] in
            self?.startCapture()
        }
    }

    func startCapture() {
        errorMessage = nil
        let sourceApplication = NSWorkspace.shared.frontmostApplication?.localizedName
        Task {
            do {
                let capture = try await captureCoordinator.captureActiveDisplay()
                let controller = SelectionOverlayController(capture: capture)
                overlayController = controller
                controller.present(
                    onSelect: { [weak self] image in
                        self?.overlayController = nil
                        self?.presentEditor(image: image, sourceApplication: sourceApplication)
                    },
                    onCancel: { [weak self] in
                        self?.overlayController = nil
                    }
                )
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    func refresh() async {
        assets = await library.allAssets(includeTrashed: true)
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
        NSPasteboard.general.clearContents()
        if !NSPasteboard.general.writeObjects([image]) {
            errorMessage = "Could not copy the capture."
        }
    }

    func reveal(_ asset: CaptureAsset) {
        NSWorkspace.shared.activateFileViewerSelecting([assetFileURL(asset)])
    }

    func assetFileURL(_ asset: CaptureAsset) -> URL {
        libraryRoot
            .appendingPathComponent("Assets", isDirectory: true)
            .appendingPathComponent(asset.filename)
    }

    private func presentEditor(image: CGImage, sourceApplication: String?) {
        let controller = EditorWindowController(image: image) { [weak self] rendered, copy, saveURL in
            self?.store(
                image: rendered,
                sourceApplication: sourceApplication,
                copyToClipboard: copy,
                saveURL: saveURL
            )
            self?.editorController = nil
        }
        editorController = controller
        controller.showWindow(nil)
        NSApplication.shared.activate(ignoringOtherApps: true)
    }

    private func store(
        image: CGImage,
        sourceApplication: String?,
        copyToClipboard: Bool,
        saveURL: URL?
    ) {
        guard let data = pngData(for: image) else {
            errorMessage = "Could not encode the capture as PNG."
            return
        }

        Task {
            do {
                _ = try await library.importData(
                    data,
                    kind: .image,
                    fileExtension: "png",
                    pixelWidth: image.width,
                    pixelHeight: image.height,
                    sourceApplication: sourceApplication
                )
                if let saveURL {
                    try data.write(to: saveURL, options: [.atomic])
                }
                if copyToClipboard {
                    let imageObject = NSImage(cgImage: image, size: .zero)
                    NSPasteboard.general.clearContents()
                    guard NSPasteboard.general.writeObjects([imageObject]) else {
                        throw CaptureExportError.clipboardWriteFailed
                    }
                }
                await refresh()
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    private func pngData(for image: CGImage) -> Data? {
        NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])
    }

    private static func makeLibrary() -> (root: URL, library: AssetLibrary, warning: String?) {
        do {
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

private enum CaptureExportError: LocalizedError {
    case clipboardWriteFailed

    var errorDescription: String? {
        "The capture was saved to the library, but could not be copied."
    }
}

@MainActor
private final class GlobalShortcutMonitor {
    private var globalMonitor: Any?
    private var localMonitor: Any?

    func start(action: @escaping @MainActor () -> Void) {
        guard globalMonitor == nil, localMonitor == nil else { return }
        globalMonitor = NSEvent.addGlobalMonitorForEvents(matching: .keyDown) { event in
            Self.handle(event, action: action)
        }
        localMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            guard Self.matches(event) else { return event }
            Task { @MainActor in action() }
            return nil
        }
    }

    private static func handle(_ event: NSEvent, action: @escaping @MainActor () -> Void) {
        guard matches(event) else { return }
        Task { @MainActor in action() }
    }

    private static func matches(_ event: NSEvent) -> Bool {
        let required: NSEvent.ModifierFlags = [.command, .shift]
        return event.charactersIgnoringModifiers == "2"
            && event.modifierFlags.intersection(required) == required
    }
}
