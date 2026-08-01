import AppKit
import KiriCore
import SwiftUI

struct LibraryView: View {
    @ObservedObject var model: AppModel
    private let columns = [GridItem(.adaptive(minimum: 190, maximum: 260), spacing: 16)]

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
                        LazyVGrid(columns: columns, spacing: 16) {
                            ForEach(model.filteredAssets) { asset in
                                CaptureCard(asset: asset, model: model)
                            }
                        }
                        .padding(24)
                    }
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color(nsColor: .underPageBackgroundColor))
        }
        .background(Color(nsColor: .windowBackgroundColor))
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }

    private var header: some View {
        HStack(spacing: 14) {
            VStack(alignment: .leading, spacing: 3) {
                Text(model.showingTrash ? "Trash" : "Library")
                    .font(.title2.weight(.semibold))
                Text(sectionSummary)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()

            searchField

            Button {
                model.showingTrash.toggle()
                model.searchQuery = ""
            } label: {
                Label(
                    model.showingTrash ? "Library" : "Trash",
                    systemImage: model.showingTrash ? "photo.on.rectangle" : "trash"
                )
            }
            .buttonStyle(.bordered)
            .controlSize(.large)

            Button {
                model.startCapture()
            } label: {
                HStack(spacing: 7) {
                    Image(systemName: "viewfinder")
                    Text("Capture")
                    Text(model.captureShortcutLabel)
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.white.opacity(0.78))
                }
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 13)
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
        .frame(width: 230, height: 32)
        .background(Color(nsColor: .controlBackgroundColor))
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay {
            RoundedRectangle(cornerRadius: 8)
                .stroke(Color.primary.opacity(0.1))
        }
        .disabled(sectionAssets.isEmpty)
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
                    .fill(Color.accentColor.opacity(0.12))
                Image(systemName: "viewfinder")
                    .font(.system(size: 29, weight: .medium))
                    .foregroundStyle(Color.accentColor)
            }
            .frame(width: 68, height: 68)

            VStack(spacing: 7) {
                Text("Ready for your first capture")
                    .font(.title2.weight(.semibold))
                Text("Capture a region, mark what matters, and paste it anywhere.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }

            Button {
                model.startCapture()
            } label: {
                Label("Capture Region", systemImage: "viewfinder")
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
                OnboardingStep(number: "1", title: "Drag", detail: "Choose a region")
                Image(systemName: "chevron.right")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                OnboardingStep(number: "2", title: "Annotate", detail: "Only if needed")
                Image(systemName: "chevron.right")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                OnboardingStep(number: "3", title: "Return", detail: "Copy and finish")
            }
        }
        .padding(.horizontal, 40)
        .padding(.vertical, 32)
        .background(Color(nsColor: .windowBackgroundColor))
        .clipShape(RoundedRectangle(cornerRadius: 20))
        .overlay {
            RoundedRectangle(cornerRadius: 20)
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
                .foregroundStyle(Color.accentColor)
                .frame(width: 22, height: 22)
                .background(Color.accentColor.opacity(0.12))
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

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ZStack {
                RoundedRectangle(cornerRadius: 10)
                    .fill(Color.black.opacity(0.06))
                if let image = NSImage(contentsOf: model.assetFileURL(asset)) {
                    Image(nsImage: image)
                        .resizable()
                        .scaledToFit()
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                        .padding(5)
                } else {
                    Image(systemName: iconName)
                        .font(.system(size: 30))
                        .foregroundStyle(.secondary)
                }
            }
            .aspectRatio(16 / 10, contentMode: .fit)

            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(asset.createdAt, style: .date)
                        .font(.subheadline.weight(.medium))
                    Text("\(asset.pixelWidth) × \(asset.pixelHeight) · \(asset.kind.rawValue)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                if asset.isFavorite {
                    Image(systemName: "star.fill")
                        .foregroundStyle(.yellow)
                }
            }

            HStack(spacing: 6) {
                if asset.trashedAt == nil {
                    smallButton("doc.on.doc", help: "Copy") { model.copy(asset) }
                    smallButton(
                        asset.isFavorite ? "star.slash" : "star",
                        help: asset.isFavorite ? "Remove Favorite" : "Favorite"
                    ) {
                        model.toggleFavorite(asset)
                    }
                    smallButton("folder", help: "Show in Finder") { model.reveal(asset) }
                    Spacer()
                    smallButton("trash", help: "Move to Trash") { model.moveToTrash(asset) }
                } else {
                    Button("Restore") { model.restore(asset) }
                        .buttonStyle(.borderless)
                    Spacer()
                    smallButton("trash.fill", help: "Delete Permanently") {
                        model.permanentlyDelete(asset)
                    }
                }
            }
        }
        .padding(12)
        .background(.background)
        .clipShape(RoundedRectangle(cornerRadius: 14))
        .overlay {
            RoundedRectangle(cornerRadius: 14)
                .stroke(Color.primary.opacity(0.08))
        }
    }

    private var iconName: String {
        switch asset.kind {
        case .image: "photo"
        case .video: "video"
        case .gif: "sparkles.rectangle.stack"
        case .longImage: "rectangle.portrait"
        }
    }

    private func smallButton(
        _ systemName: String,
        help: String,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: systemName)
        }
        .buttonStyle(.borderless)
        .help(help)
        .accessibilityLabel(help)
    }
}
