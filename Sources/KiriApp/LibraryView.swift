import AppKit
@preconcurrency import AVFoundation
import ImageIO
import KiriCore
import SwiftUI

private extension Color {
    static let kiriAccent = Color(nsColor: CaptureUIColors.accent)
}

struct LibraryView: View {
    @ObservedObject var model: AppModel
    @FocusState private var searchIsFocused: Bool
    private let columns = [
        GridItem(.adaptive(minimum: 210, maximum: 280), spacing: KiriUI.Spacing.roomy)
    ]

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            errorBanner
            Group {
                if !model.hasLoadedLibrary {
                    loadingState
                } else if model.filteredAssets.isEmpty {
                    emptyState
                } else {
                    ScrollView {
                        LazyVGrid(columns: columns, spacing: KiriUI.Spacing.roomy) {
                            ForEach(model.filteredAssets) { asset in
                                CaptureCard(asset: asset, model: model)
                            }
                        }
                        .padding(KiriUI.Spacing.page)
                    }
                    .id(model.showingTrash)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color(nsColor: .windowBackgroundColor))
        }
        .background(Color(nsColor: .windowBackgroundColor))
        .tint(Color(nsColor: CaptureUIColors.accent))
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .overlay(alignment: .top) {
            if let notice = model.notice {
                LibraryNoticeView(notice: notice) {
                    model.dismissNotice()
                }
                .padding(.top, 78)
                .transition(.move(edge: .top).combined(with: .opacity))
                .zIndex(10)
            }
        }
        .animation(.easeOut(duration: KiriUI.Motion.feedback), value: model.notice)
        .focusedValue(\.focusLibrarySearch) {
            searchIsFocused = true
        }
    }

    private var header: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: KiriUI.Spacing.standard) {
                titleBlock
                Spacer(minLength: 18)
                searchField
                    .frame(width: 210)
                sectionPicker
                captureActions
            }

            VStack(spacing: KiriUI.Spacing.standard) {
                HStack(spacing: KiriUI.Spacing.standard) {
                    titleBlock
                    Spacer()
                    captureActions
                }
                HStack(spacing: KiriUI.Spacing.compact) {
                    searchField
                        .frame(maxWidth: .infinity)
                    sectionPicker
                }
            }
            .frame(maxWidth: .infinity)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 12)
    }

    private var titleBlock: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(model.showingTrash ? "Trash" : "Library")
                .font(.title3.weight(.semibold))
            Text(sectionSummary)
                .font(.caption)
                .foregroundStyle(.secondary)
                .contentTransition(.numericText())
        }
        .fixedSize()
    }

    private var sectionPicker: some View {
        Picker("Section", selection: $model.showingTrash) {
            Label("Library", systemImage: "photo.on.rectangle")
                .tag(false)
            Label("Trash", systemImage: "trash")
                .tag(true)
        }
        .pickerStyle(.segmented)
        .labelsHidden()
        .frame(width: 164)
        .onChange(of: model.showingTrash) {
            model.searchQuery = ""
        }
        .accessibilityLabel("Library section")
    }

    private var captureActions: some View {
        Button {
            model.startCapture()
        } label: {
            HStack(spacing: 7) {
                if model.isCaptureStarting {
                    ProgressView()
                        .controlSize(.small)
                } else {
                    Image(systemName: "viewfinder")
                }
                Text(model.isCaptureStarting ? "Preparing…" : "Capture")
                if !model.isCaptureStarting {
                    Text(model.captureShortcutLabel)
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.white.opacity(0.78))
                }
            }
        }
        .buttonStyle(.borderedProminent)
        .controlSize(.large)
        .disabled(model.captureIsUnavailable)
        .help("Capture or record a region, with optional annotation tools")
    }

    @ViewBuilder
    private var errorBanner: some View {
        if let errorMessage = model.errorMessage {
            HStack(spacing: 9) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange)
                Text(errorMessage)
                    .font(.callout)
                Spacer()
                if let label = model.capturePermissionRecoveryLabel {
                    Button(label) {
                        model.performCapturePermissionRecovery()
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                }
                Button {
                    model.errorMessage = nil
                } label: {
                    Image(systemName: "xmark")
                }
                .buttonStyle(.plain)
                .help("Dismiss")
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 9)
            .background(Color.orange.opacity(0.1))
            Divider()
        }
    }

    private var searchField: some View {
        HStack(spacing: 7) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
            TextField("Search captures", text: $model.searchQuery)
                .textFieldStyle(.plain)
                .focused($searchIsFocused)
                .onSubmit {
                    searchIsFocused = false
                }
            if !model.searchQuery.isEmpty {
                Button {
                    model.searchQuery = ""
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.tertiary)
                }
                .buttonStyle(.plain)
                .help("Clear Search")
            }
        }
        .padding(.horizontal, 10)
        .frame(height: 32)
        .background(Color(nsColor: .controlBackgroundColor))
        .clipShape(RoundedRectangle(cornerRadius: KiriUI.Radius.control))
        .overlay {
            RoundedRectangle(cornerRadius: KiriUI.Radius.control)
                .stroke(Color.primary.opacity(0.1))
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Search captures")
    }

    private var loadingState: some View {
        VStack(spacing: 10) {
            ProgressView()
                .controlSize(.small)
            Text("Loading Library…")
                .font(.callout)
                .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private var emptyState: some View {
        if hasSearchQuery {
            LibraryStatusView(
                systemImage: "magnifyingglass",
                title: "No matching captures",
                message: "Try a different search, or clear the current one."
            ) {
                Button("Clear Search") {
                    model.searchQuery = ""
                }
            }
        } else if model.showingTrash {
            LibraryStatusView(
                systemImage: "trash",
                title: "Trash is empty",
                message: "Captures you delete stay recoverable here."
            )
        } else {
            onboardingState
        }
    }

    private var onboardingState: some View {
        VStack(spacing: 20) {
            ZStack {
                Circle()
                    .fill(Color.kiriAccent.opacity(0.12))
                Image(systemName: "viewfinder")
                    .font(.system(size: 29, weight: .medium))
                    .foregroundStyle(Color.kiriAccent)
                Image(systemName: "sparkles")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(Color(nsColor: CaptureUIColors.blossom))
                    .offset(x: 27, y: -25)
            }
            .frame(width: 68, height: 68)

            VStack(spacing: 7) {
                Text("Ready for your first capture")
                    .font(.title2.weight(.semibold))
                Text("Choose Screenshot or Record, then select the region you need.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }

            Button {
                model.startCapture()
            } label: {
                Label("Capture", systemImage: "viewfinder")
                    .frame(minWidth: 150)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)

            Text("or press  \(model.captureShortcutLabel)")
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)

            Divider()
                .frame(width: 400)

            HStack(spacing: 18) {
                OnboardingStep(number: "1", title: "Mode", detail: "Screenshot or Record")
                Image(systemName: "chevron.right")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                OnboardingStep(number: "2", title: "Select", detail: "Choose a region")
                Image(systemName: "chevron.right")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                OnboardingStep(number: "3", title: "Finish", detail: "Copy or save")
            }
        }
        .padding(.horizontal, 40)
        .padding(.vertical, 32)
        .background(Color(nsColor: .windowBackgroundColor))
        .clipShape(RoundedRectangle(cornerRadius: KiriUI.Radius.surface))
        .overlay {
            RoundedRectangle(cornerRadius: KiriUI.Radius.surface)
                .stroke(Color.primary.opacity(0.08))
        }
        .shadow(color: .black.opacity(0.04), radius: 18, y: 8)
    }

    private var sectionAssets: [CaptureAsset] {
        model.assets.filter { asset in
            model.showingTrash ? asset.trashedAt != nil : asset.trashedAt == nil
        }
    }

    private var sectionSummary: String {
        let count = sectionAssets.count
        return count == 1 ? "1 capture" : "\(count) captures"
    }

    private var hasSearchQuery: Bool {
        !model.searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
}

private struct OnboardingStep: View {
    let number: String
    let title: String
    let detail: String

    var body: some View {
        HStack(spacing: 8) {
            Text(number)
                .font(.caption.weight(.semibold))
                .foregroundStyle(Color.kiriAccent)
                .frame(width: 22, height: 22)
                .background(Color.kiriAccent.opacity(0.12))
                .clipShape(Circle())
            VStack(alignment: .leading, spacing: 1) {
                Text(title)
                    .font(.caption.weight(.semibold))
                Text(detail)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

private struct LibraryStatusView<Actions: View>: View {
    let systemImage: String
    let title: String
    let message: String
    @ViewBuilder let actions: Actions

    init(
        systemImage: String,
        title: String,
        message: String,
        @ViewBuilder actions: () -> Actions
    ) {
        self.systemImage = systemImage
        self.title = title
        self.message = message
        self.actions = actions()
    }

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: systemImage)
                .font(.system(size: 28, weight: .medium))
                .foregroundStyle(.tertiary)
            Text(title)
                .font(.headline)
            Text(message)
                .font(.callout)
                .foregroundStyle(.secondary)
            actions
        }
        .multilineTextAlignment(.center)
        .padding(32)
    }
}

private extension LibraryStatusView where Actions == EmptyView {
    init(systemImage: String, title: String, message: String) {
        self.init(systemImage: systemImage, title: title, message: message) {
            EmptyView()
        }
    }
}

private struct CaptureCard: View {
    let asset: CaptureAsset
    @ObservedObject var model: AppModel
    @State private var isHovered = false
    @State private var confirmsPermanentDelete = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            CaptureThumbnail(
                fileURL: model.assetFileURL(asset),
                fallbackSystemImage: iconName,
                reloadToken: model.libraryRevision
            )
            .overlay {
                if isHovered, asset.trashedAt == nil {
                    Button {
                        performPrimaryAction()
                    } label: {
                        Label(primaryActionTitle, systemImage: primaryActionSymbol)
                            .font(.callout.weight(.semibold))
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.large)
                    .transition(.scale(scale: 0.96).combined(with: .opacity))
                }
            }
            .aspectRatio(16 / 10, contentMode: .fit)
            .contentShape(Rectangle())
            .onTapGesture(count: 2) {
                model.open(asset)
            }
            .help("Double-click to open")

            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(asset.createdAt, format: .dateTime.month(.abbreviated).day().hour().minute())
                        .font(.subheadline.weight(.medium))
                        .lineLimit(1)
                    Text(metadata)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Spacer()
                if asset.isFavorite {
                    Image(systemName: "star.fill")
                        .foregroundStyle(.yellow)
                }
            }

            HStack(spacing: 6) {
                if asset.trashedAt == nil {
                    Button {
                        performPrimaryAction()
                    } label: {
                        Label(primaryActionTitle, systemImage: primaryActionSymbol)
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)

                    Spacer()
                    iconButton(
                        asset.isFavorite ? "star.slash" : "star",
                        help: asset.isFavorite ? "Remove Favorite" : "Favorite"
                    ) {
                        model.toggleFavorite(asset)
                    }
                    iconButton("trash", help: "Move to Trash", role: .destructive) {
                        model.moveToTrash(asset)
                    }
                    actionMenu
                } else {
                    Button {
                        model.restore(asset)
                    } label: {
                        Label("Restore", systemImage: "arrow.uturn.backward")
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    Spacer()
                    Button(role: .destructive) {
                        confirmsPermanentDelete = true
                    } label: {
                        Image(systemName: "trash.fill")
                    }
                    .buttonStyle(.borderless)
                    .help("Delete Permanently")
                    .accessibilityLabel("Delete Permanently")
                }
            }
        }
        .padding(12)
        .background(Color(nsColor: .controlBackgroundColor))
        .clipShape(RoundedRectangle(cornerRadius: KiriUI.Radius.card))
        .overlay {
            RoundedRectangle(cornerRadius: KiriUI.Radius.card)
                .stroke(
                    isHovered ? Color.kiriAccent.opacity(0.42) : Color.primary.opacity(0.09),
                    lineWidth: isHovered ? 1.5 : 1
                )
        }
        .shadow(color: .black.opacity(isHovered ? 0.10 : 0.035), radius: isHovered ? 12 : 4, y: 4)
        .scaleEffect(isHovered ? 1.008 : 1)
        .animation(.easeOut(duration: KiriUI.Motion.hover), value: isHovered)
        .onHover { isHovered = $0 }
        .onDrag {
            NSItemProvider(contentsOf: model.assetFileURL(asset)) ?? NSItemProvider()
        }
        .contextMenu {
            if asset.trashedAt == nil {
                if asset.kind == .image || asset.kind == .longImage {
                    Button("Copy", systemImage: "doc.on.doc") { model.copy(asset) }
                }
                Button("Open", systemImage: "arrow.up.right.square") { model.open(asset) }
                Button("Show in Finder", systemImage: "folder") { model.reveal(asset) }
                if asset.kind == .video {
                    Button("Convert to GIF", systemImage: "sparkles.rectangle.stack") {
                        model.convertToGIF(asset)
                    }
                    .disabled(!model.canConvertToGIF(asset) || model.isConvertingToGIF(asset))
                }
                Button(
                    asset.isFavorite ? "Remove Favorite" : "Favorite",
                    systemImage: asset.isFavorite ? "star.slash" : "star"
                ) {
                    model.toggleFavorite(asset)
                }
                Divider()
                Button("Move to Trash", systemImage: "trash", role: .destructive) {
                    model.moveToTrash(asset)
                }
            } else {
                Button("Restore", systemImage: "arrow.uturn.backward") {
                    model.restore(asset)
                }
                Divider()
                Button("Delete Permanently", systemImage: "trash.fill", role: .destructive) {
                    confirmsPermanentDelete = true
                }
            }
        }
        .confirmationDialog(
            "Delete this capture permanently?",
            isPresented: $confirmsPermanentDelete,
            titleVisibility: .visible
        ) {
            Button("Delete Permanently", role: .destructive) {
                model.permanentlyDelete(asset)
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This cannot be undone.")
        }
    }

    private var metadata: String {
        let dimensions = "\(asset.pixelWidth) × \(asset.pixelHeight)"
        let details = if let duration = asset.duration {
            "\(dimensions) · \(RecordingPolicy.elapsedLabel(duration))"
        } else {
            dimensions
        }
        guard let source = asset.sourceApplication, !source.isEmpty else {
            return details
        }
        return "\(details) · \(source)"
    }

    private var iconName: String {
        switch asset.kind {
        case .image: "photo"
        case .video: "video"
        case .gif: "sparkles.rectangle.stack"
        case .longImage: "rectangle.portrait"
        }
    }

    private var actionMenu: some View {
        Menu {
            if asset.kind == .video {
                Button(
                    model.isConvertingToGIF(asset) ? "Converting to GIF…" : "Convert to GIF",
                    systemImage: "sparkles.rectangle.stack"
                ) {
                    model.convertToGIF(asset)
                }
                .disabled(!model.canConvertToGIF(asset) || model.isConvertingToGIF(asset))
                Divider()
            }
            Button("Open", systemImage: "arrow.up.right.square") {
                model.open(asset)
            }
            Button("Show in Finder", systemImage: "folder") {
                model.reveal(asset)
            }
        } label: {
            Image(systemName: "ellipsis")
                .frame(width: 18, height: 18)
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .help("More Actions")
        .accessibilityLabel("More Actions")
    }

    private var primaryActionTitle: String {
        asset.kind == .image || asset.kind == .longImage ? "Copy" : "Open"
    }

    private var primaryActionSymbol: String {
        asset.kind == .image || asset.kind == .longImage ? "doc.on.doc" : "play.fill"
    }

    private func performPrimaryAction() {
        if asset.kind == .image || asset.kind == .longImage {
            model.copy(asset)
        } else {
            model.open(asset)
        }
    }

    private func iconButton(
        _ systemName: String,
        help: String,
        role: ButtonRole? = nil,
        action: @escaping () -> Void
    ) -> some View {
        Button(role: role, action: action) {
            Image(systemName: systemName)
        }
        .buttonStyle(.borderless)
        .help(help)
        .accessibilityLabel(help)
    }
}

private struct CaptureThumbnail: View {
    let fileURL: URL
    let fallbackSystemImage: String
    let reloadToken: Int
    @State private var image: CGImage?
    @State private var hasFinishedLoading = false

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: KiriUI.Radius.preview)
                .fill(Color.black.opacity(0.055))

            if let image {
                Image(decorative: image, scale: 1)
                    .resizable()
                    .interpolation(.high)
                    .scaledToFit()
                    .clipShape(RoundedRectangle(cornerRadius: KiriUI.Radius.control))
                    .padding(5)
            } else if hasFinishedLoading {
                Image(systemName: fallbackSystemImage)
                    .font(.system(size: 30, weight: .medium))
                    .foregroundStyle(.tertiary)
            } else {
                ProgressView()
                    .controlSize(.small)
            }
        }
        .task(id: reloadToken) {
            hasFinishedLoading = false
            image = await CaptureThumbnailLoader.load(fileURL)
            hasFinishedLoading = true
        }
    }
}

private enum CaptureThumbnailLoader {
    static func load(_ url: URL) async -> CGImage? {
        if url.pathExtension.lowercased() == "mp4" || url.pathExtension.lowercased() == "mov" {
            let asset = AVURLAsset(url: url)
            let generator = AVAssetImageGenerator(asset: asset)
            generator.appliesPreferredTrackTransform = true
            generator.maximumSize = CGSize(width: 640, height: 640)
            return try? await generator.image(at: .zero).image
        }
        return await Task.detached(priority: .userInitiated) { () -> CGImage? in
            guard !Task.isCancelled else { return nil }
            guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else {
                return nil
            }
            let options: [CFString: Any] = [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceShouldCacheImmediately: true,
                kCGImageSourceThumbnailMaxPixelSize: 640
            ]
            guard !Task.isCancelled else { return nil }
            return CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
        }.value
    }
}

private struct LibraryNoticeView: View {
    let notice: AppNotice
    let dismiss: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: notice.symbol)
                .foregroundStyle(Color.kiriAccent)
            Text(notice.title)
                .font(.callout.weight(.medium))
            Button(action: dismiss) {
                Image(systemName: "xmark")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help("Dismiss")
        }
        .padding(.horizontal, 12)
        .frame(height: 36)
        .background(.regularMaterial, in: Capsule())
        .overlay {
            Capsule()
                .stroke(Color.primary.opacity(0.12))
        }
        .shadow(color: .black.opacity(0.12), radius: 12, y: 5)
        .accessibilityElement(children: .combine)
    }
}

private struct FocusLibrarySearchKey: FocusedValueKey {
    typealias Value = () -> Void
}

extension FocusedValues {
    var focusLibrarySearch: (() -> Void)? {
        get { self[FocusLibrarySearchKey.self] }
        set { self[FocusLibrarySearchKey.self] = newValue }
    }
}
