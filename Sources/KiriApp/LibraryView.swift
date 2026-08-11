import AppKit
@preconcurrency import AVFoundation
import ImageIO
import KiriCore
import SwiftUI

private extension Color {
    static let kiriAccent = KiriUI.Palette.accent
    static let kiriCanvas = KiriUI.Palette.canvas
    static let kiriCard = KiriUI.Palette.card
    static let kiriElevated = KiriUI.Palette.elevated
}

struct LibraryView: View {
    @ObservedObject var model: AppModel
    @FocusState private var searchIsFocused: Bool
    @State private var confirmsEmptyTrash = false
    private let columns = [
        GridItem(.adaptive(minimum: 210, maximum: 280), spacing: KiriUI.Spacing.roomy)
    ]

    var body: some View {
        VStack(spacing: 0) {
            header
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
        }
        .background {
            Color.kiriCanvas
            .ignoresSafeArea()
        }
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
        .sheet(isPresented: $confirmsEmptyTrash) {
            KiriDestructiveConfirmationView(
                title: L10n.text("Empty Trash?"),
                message: L10n.text("All captures in Trash will be permanently deleted. This cannot be undone."),
                confirmTitle: L10n.text("Empty Trash")
            ) {
                model.emptyTrash()
            }
        }
    }

    private var header: some View {
        ViewThatFits(in: .horizontal) {
            wideHeader
            compactHeader
        }
        .padding(.horizontal, KiriUI.Spacing.page)
        .padding(.vertical, 15)
        .background(.regularMaterial)
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(KiriUI.Palette.border.opacity(0.8))
                .frame(height: 1)
        }
    }

    private var wideHeader: some View {
        HStack(spacing: KiriUI.Spacing.standard) {
            titleBlock
                .layoutPriority(1)
            Spacer(minLength: 0)
            searchField
                .frame(width: KiriUI.Header.searchWidth)
            sectionPicker
            if model.showingTrash {
                emptyTrashButton
            }
            captureActions
        }
        .frame(maxWidth: .infinity)
    }

    private var compactHeader: some View {
        VStack(spacing: KiriUI.Spacing.standard) {
            HStack(spacing: KiriUI.Spacing.standard) {
                titleBlock
                    .layoutPriority(1)
                Spacer(minLength: 0)
                if model.showingTrash {
                    emptyTrashButton
                }
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

    private var emptyTrashButton: some View {
        Button {
            confirmsEmptyTrash = true
        } label: {
            Label(L10n.text("Empty Trash"), systemImage: "trash.slash")
                .font(.system(size: 12, weight: .medium))
                .labelStyle(.titleAndIcon)
        }
        .buttonStyle(.bordered)
        .tint(.red)
        .disabled(!model.assets.contains { $0.trashedAt != nil })
        .help(L10n.text("Permanently delete all captures in Trash"))
        .accessibilityLabel(L10n.text("Empty Trash"))
    }

    private var titleBlock: some View {
        HStack(spacing: KiriUI.Spacing.compact) {
            KiriBrandMark(size: 38)

            VStack(alignment: .leading, spacing: 2) {
                Text(L10n.text(model.showingTrash ? "Trash" : "Library"))
                    .font(.system(size: 17, weight: .bold, design: .rounded))
                Text(sectionSummary)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .contentTransition(.numericText())
            }
        }
        .fixedSize(horizontal: true, vertical: true)
    }

    private var sectionPicker: some View {
        Picker(L10n.text("Section"), selection: $model.showingTrash) {
            Label(L10n.text("Library"), systemImage: "photo.on.rectangle")
                .tag(false)
            Label(L10n.text("Trash"), systemImage: "trash")
                .tag(true)
        }
        .pickerStyle(.segmented)
        .labelsHidden()
        .controlSize(.large)
        .frame(width: KiriUI.Header.sectionPickerWidth)
        .onChange(of: model.showingTrash) {
            model.searchQuery = ""
        }
        .accessibilityLabel(L10n.text("Library section"))
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
                Text(L10n.text(model.isCaptureStarting ? "Preparing…" : "Capture"))
                if !model.isCaptureStarting {
                    Text(model.captureShortcutLabel)
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.white.opacity(0.78))
                }
            }
            .fixedSize(horizontal: true, vertical: false)
        }
        .buttonStyle(KiriPrimaryButtonStyle())
        .disabled(model.captureIsUnavailable)
        .help(L10n.text("Capture or record a region, with optional annotation tools"))
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
                .help(L10n.text("Dismiss"))
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 11)
            .background(Color.orange.opacity(0.10), in: RoundedRectangle(cornerRadius: KiriUI.Radius.control))
            .overlay {
                RoundedRectangle(cornerRadius: KiriUI.Radius.control)
                    .stroke(Color.orange.opacity(0.22))
            }
            .padding(.horizontal, KiriUI.Spacing.page)
            .padding(.top, KiriUI.Spacing.compact)
        }
    }

    private var searchField: some View {
        HStack(spacing: 7) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
            TextField(L10n.text("Search captures"), text: $model.searchQuery)
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
                .help(L10n.text("Clear Search"))
            }
        }
        .padding(.horizontal, 10)
        .frame(height: KiriUI.Header.controlHeight)
        .background(Color.kiriElevated)
        .clipShape(RoundedRectangle(cornerRadius: KiriUI.Radius.control))
        .overlay {
            RoundedRectangle(cornerRadius: KiriUI.Radius.control)
                .stroke(searchIsFocused ? Color.kiriAccent.opacity(0.58) : KiriUI.Palette.border)
        }
        .shadow(color: searchIsFocused ? Color.kiriAccent.opacity(0.10) : .clear, radius: 7)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(L10n.text("Search captures"))
    }

    private var loadingState: some View {
        VStack(spacing: 10) {
            ProgressView()
                .controlSize(.small)
            Text(L10n.text("Loading Library…"))
                .font(.callout)
                .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private var emptyState: some View {
        if hasSearchQuery {
            LibraryStatusView(
                systemImage: "magnifyingglass",
                title: L10n.text("No matching captures"),
                message: L10n.text("Try a different search, or clear the current one.")
            ) {
                Button(L10n.text("Clear Search")) {
                    model.searchQuery = ""
                }
            }
        } else if model.showingTrash {
            LibraryStatusView(
                systemImage: "trash",
                title: L10n.text("Trash is empty"),
                message: L10n.text("Captures you delete stay recoverable here.")
            )
        } else {
            onboardingState
        }
    }

    private var onboardingState: some View {
        VStack(spacing: 20) {
            ZStack(alignment: .topTrailing) {
                KiriBrandMark(size: 72)
                Image(systemName: "sparkles")
                    .font(.system(size: 14, weight: .bold))
                    .foregroundStyle(KiriUI.Palette.coral)
                    .padding(2)
                    .background(.thinMaterial, in: Circle())
                    .offset(x: 8, y: -7)
            }

            VStack(spacing: 7) {
                Text(L10n.text("Ready for your first capture"))
                    .font(.system(size: 22, weight: .bold, design: .rounded))
                Text(L10n.text("Choose a capture mode, then select the region you need."))
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }

            Button {
                model.startCapture()
            } label: {
                Label(L10n.text("Capture"), systemImage: "viewfinder")
                    .frame(minWidth: 150)
            }
            .buttonStyle(KiriPrimaryButtonStyle())

            Text(L10n.format("or press  %@", model.captureShortcutLabel))
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)

            Rectangle()
                .fill(KiriUI.Palette.border)
                .frame(width: 400, height: 1)

            HStack(spacing: 18) {
                OnboardingStep(number: "1", title: L10n.text("Mode"), detail: L10n.text("Screenshot · Record · OCR"))
                Image(systemName: "chevron.right")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                OnboardingStep(number: "2", title: L10n.text("Select"), detail: L10n.text("Choose a region"))
                Image(systemName: "chevron.right")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                OnboardingStep(number: "3", title: L10n.text("Finish"), detail: L10n.text("Copy or save"))
            }
        }
        .padding(.horizontal, 40)
        .padding(.vertical, 34)
        .background {
            ZStack {
                Color.kiriCard
                LinearGradient(
                    colors: [
                        KiriUI.Palette.accent.opacity(0.09),
                        .clear,
                        KiriUI.Palette.cyan.opacity(0.06)
                    ],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
            }
        }
        .kiriSurface(radius: KiriUI.Radius.surface, elevated: true)
    }

    private var sectionAssets: [CaptureAsset] {
        model.assets.filter { asset in
            model.showingTrash ? asset.trashedAt != nil : asset.trashedAt == nil
        }
    }

    private var sectionSummary: String {
        let count = sectionAssets.count
        return L10n.format(count == 1 ? "%d capture" : "%d captures", count)
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
        VStack(alignment: .leading, spacing: KiriUI.Spacing.standard) {
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
                    .buttonStyle(KiriPrimaryButtonStyle())
                    .controlSize(.large)
                    .transition(.scale(scale: 0.96).combined(with: .opacity))
                }
            }
            .overlay(alignment: .topLeading) {
                kindBadge
                    .padding(KiriUI.Spacing.compact)
            }
            .frame(maxWidth: .infinity)
            .frame(height: KiriUI.Card.thumbnailHeight)
            .contentShape(Rectangle())
            .onTapGesture(count: 2) {
                model.open(asset)
            }
            .help(L10n.text("Double-click to open"))

            VStack(alignment: .leading, spacing: KiriUI.Card.metadataSpacing) {
                HStack(alignment: .firstTextBaseline, spacing: KiriUI.Spacing.compact) {
                    Text(asset.createdAt, format: .dateTime.month(.abbreviated).day().hour().minute())
                        .font(.subheadline.weight(.medium))
                        .lineLimit(1)
                    Spacer(minLength: 0)
                    if asset.isFavorite {
                        Image(systemName: "star.fill")
                            .foregroundStyle(.yellow)
                            .accessibilityHidden(true)
                    }
                }
                metadataLine
            }

            HStack(spacing: KiriUI.Card.actionSpacing) {
                if asset.trashedAt == nil {
                    Button {
                        performPrimaryAction()
                    } label: {
                        Label(primaryActionTitle, systemImage: primaryActionSymbol)
                    }
                    .buttonStyle(.bordered)
                    .tint(Color.kiriAccent)
                    .controlSize(.small)
                    .help(primaryActionTitle)

                    Spacer()
                    iconButton(
                        asset.isFavorite ? "star.slash" : "star",
                        help: L10n.text(asset.isFavorite ? "Remove Favorite" : "Favorite")
                    ) {
                        model.toggleFavorite(asset)
                    }
                    iconButton("trash", help: L10n.text("Move to Trash"), role: .destructive) {
                        model.moveToTrash(asset)
                    }
                    actionMenu
                } else {
                    Button {
                        model.restore(asset)
                    } label: {
                        Label(L10n.text("Restore"), systemImage: "arrow.uturn.backward")
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
                    .frame(width: 28, height: 26)
                    .help(L10n.text("Delete Permanently"))
                    .accessibilityLabel(L10n.text("Delete Permanently"))
                }
            }
        }
        .padding(KiriUI.Card.padding)
        .background(Color.kiriCard)
        .clipShape(RoundedRectangle(cornerRadius: KiriUI.Radius.card))
        .overlay {
            RoundedRectangle(cornerRadius: KiriUI.Radius.card)
                .stroke(
                    isHovered ? Color.kiriAccent.opacity(0.52) : KiriUI.Palette.border,
                    lineWidth: isHovered ? 1.25 : 1
                )
        }
        .shadow(color: Color.kiriAccent.opacity(isHovered ? 0.12 : 0), radius: 16, y: 7)
        .shadow(color: .black.opacity(isHovered ? 0.08 : 0.035), radius: isHovered ? 10 : 5, y: 4)
        .offset(y: isHovered ? -1 : 0)
        .animation(.easeOut(duration: KiriUI.Motion.hover), value: isHovered)
        .onHover { isHovered = $0 }
        .onDrag {
            NSItemProvider(contentsOf: model.assetFileURL(asset)) ?? NSItemProvider()
        }
        .contextMenu {
            if asset.trashedAt == nil {
                if asset.kind == .image {
                    Button(L10n.text("Copy"), systemImage: "doc.on.doc") { model.copy(asset) }
                }
                Button(L10n.text("Open"), systemImage: "arrow.up.right.square") { model.open(asset) }
                Button(L10n.text("Show in Finder"), systemImage: "folder") { model.reveal(asset) }
                if asset.kind == .video {
                    Button(L10n.text("Convert to GIF"), systemImage: "sparkles.rectangle.stack") {
                        model.convertToGIF(asset)
                    }
                    .disabled(!model.canConvertToGIF(asset) || model.isConvertingToGIF(asset))
                }
                Button(
                    L10n.text(asset.isFavorite ? "Remove Favorite" : "Favorite"),
                    systemImage: asset.isFavorite ? "star.slash" : "star"
                ) {
                    model.toggleFavorite(asset)
                }
                Divider()
                Button(L10n.text("Move to Trash"), systemImage: "trash", role: .destructive) {
                    model.moveToTrash(asset)
                }
            } else {
                Button(L10n.text("Restore"), systemImage: "arrow.uturn.backward") {
                    model.restore(asset)
                }
                Divider()
                Button(L10n.text("Delete Permanently"), systemImage: "trash.fill", role: .destructive) {
                    confirmsPermanentDelete = true
                }
            }
        }
        .sheet(isPresented: $confirmsPermanentDelete) {
            KiriDestructiveConfirmationView(
                title: L10n.text("Delete this capture permanently?"),
                message: L10n.text("This cannot be undone."),
                confirmTitle: L10n.text("Delete Permanently")
            ) {
                model.permanentlyDelete(asset)
            }
        }
    }

    @ViewBuilder
    private var metadataLine: some View {
        HStack(spacing: KiriUI.Card.metadataSpacing) {
            Text(pixelSize)
                .font(.caption.monospacedDigit())
                .fixedSize()
            if let duration = asset.duration {
                metadataSeparator
                Text(RecordingPolicy.elapsedLabel(duration))
                    .font(.caption.monospacedDigit())
                    .fixedSize()
            }
            if let source = asset.sourceApplication, !source.isEmpty {
                metadataSeparator
                Text(source)
                    .font(.caption)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
        }
        .foregroundStyle(.secondary)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var metadataSeparator: some View {
        Text("·")
            .foregroundStyle(.tertiary)
            .accessibilityHidden(true)
    }

    private var pixelSize: String {
        "\(asset.pixelWidth) × \(asset.pixelHeight)"
    }

    private var kindBadge: some View {
        Label(kindTitle, systemImage: iconName)
            .font(.caption2.weight(.semibold))
            .foregroundStyle(Color.kiriAccent)
            .padding(.horizontal, 7)
            .frame(height: 24)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: KiriUI.Radius.badge))
            .overlay {
                RoundedRectangle(cornerRadius: KiriUI.Radius.badge)
                    .stroke(Color.primary.opacity(0.12))
            }
            .accessibilityLabel(kindTitle)
    }

    private var kindTitle: String {
        switch asset.kind {
        case .image: L10n.text("Image")
        case .video: L10n.text("Video")
        case .gif: "GIF"
        }
    }

    private var iconName: String {
        switch asset.kind {
        case .image: "photo"
        case .video: "video"
        case .gif: "sparkles.rectangle.stack"
        }
    }

    private var actionMenu: some View {
        Menu {
            if asset.kind == .video {
                Button(
                    L10n.text(model.isConvertingToGIF(asset) ? "Converting to GIF…" : "Convert to GIF"),
                    systemImage: "sparkles.rectangle.stack"
                ) {
                    model.convertToGIF(asset)
                }
                .disabled(!model.canConvertToGIF(asset) || model.isConvertingToGIF(asset))
                Divider()
            }
            Button(L10n.text("Open"), systemImage: "arrow.up.right.square") {
                model.open(asset)
            }
            Button(L10n.text("Show in Finder"), systemImage: "folder") {
                model.reveal(asset)
            }
        } label: {
            Image(systemName: "ellipsis")
                .frame(width: 28, height: 26)
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .help(L10n.text("More Actions"))
        .accessibilityLabel(L10n.text("More Actions"))
    }

    private var primaryActionTitle: String {
        L10n.text(asset.kind == .image ? "Copy" : "Open")
    }

    private var primaryActionSymbol: String {
        asset.kind == .image ? "doc.on.doc" : "play.fill"
    }

    private func performPrimaryAction() {
        if asset.kind == .image {
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
        .frame(width: 28, height: 26)
        .contentShape(Rectangle())
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
                .fill(
                    LinearGradient(
                        colors: [
                            KiriUI.Palette.accent.opacity(0.075),
                            KiriUI.Palette.cyan.opacity(0.04)
                        ],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )

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
            .help(L10n.text("Dismiss"))
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

private struct KiriDestructiveConfirmationView: View {
    @Environment(\.dismiss) private var dismiss
    let title: String
    let message: String
    let confirmTitle: String
    let onConfirm: () -> Void

    var body: some View {
        VStack(spacing: KiriUI.Spacing.roomy) {
            ZStack {
                RoundedRectangle(cornerRadius: 18)
                    .fill(KiriUI.warmGradient.opacity(0.16))
                    .frame(width: 58, height: 58)
                Image(systemName: "trash.fill")
                    .font(.system(size: 23, weight: .semibold))
                    .foregroundStyle(KiriUI.Palette.coral)
            }
            .accessibilityHidden(true)

            VStack(spacing: 8) {
                Text(title)
                    .font(.system(size: 18, weight: .bold, design: .rounded))
                    .multilineTextAlignment(.center)
                Text(message)
                    .font(.system(size: 12.5))
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
            }

            HStack(spacing: KiriUI.Spacing.compact) {
                Button(L10n.text("Cancel")) {
                    dismiss()
                }
                .buttonStyle(.bordered)
                .controlSize(.large)
                .keyboardShortcut(.cancelAction)

                Button(role: .destructive) {
                    onConfirm()
                    dismiss()
                } label: {
                    Text(confirmTitle)
                        .frame(minWidth: 118)
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                .tint(KiriUI.Palette.coral)
            }
        }
        .padding(26)
        .frame(width: 370)
        .background {
            ZStack {
                KiriUI.Palette.canvas
                RadialGradient(
                    colors: [KiriUI.Palette.coral.opacity(0.08), .clear],
                    center: .top,
                    startRadius: 0,
                    endRadius: 220
                )
            }
        }
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
