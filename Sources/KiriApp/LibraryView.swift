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
            if model.filteredAssets.isEmpty {
                emptyState
            } else {
                ScrollView {
                    LazyVGrid(columns: columns, spacing: 16) {
                        ForEach(model.filteredAssets) { asset in
                            CaptureCard(asset: asset, model: model)
                        }
                    }
                    .padding(20)
                }
            }
        }
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private var header: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text("kiri")
                    .font(.title2.weight(.semibold))
                Text(model.showingTrash ? "Trash" : "Capture library")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            TextField("Search captures", text: $model.searchQuery)
                .textFieldStyle(.roundedBorder)
                .frame(width: 220)
            Button(model.showingTrash ? "Library" : "Trash") {
                model.showingTrash.toggle()
            }
            Button("Capture  \(model.captureShortcutLabel)") {
                model.startCapture()
            }
            .buttonStyle(.borderedProminent)
            .tint(Color(red: 0.48, green: 0.42, blue: 0.82))
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 14)
    }

    private var emptyState: some View {
        ContentUnavailableView {
            Label(
                model.showingTrash ? "Trash is empty" : "No captures yet",
                systemImage: model.showingTrash ? "trash" : "viewfinder"
            )
        } description: {
            Text(model.showingTrash
                 ? "Deleted captures stay recoverable here."
                 : "Press \(model.captureShortcutLabel) to capture a region. Every result is saved here automatically.")
        } actions: {
            if !model.showingTrash {
                Button("Capture Region") {
                    model.startCapture()
                }
            }
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
                    smallButton("doc.on.doc") { model.copy(asset) }
                    smallButton(asset.isFavorite ? "star.slash" : "star") {
                        model.toggleFavorite(asset)
                    }
                    smallButton("folder") { model.reveal(asset) }
                    Spacer()
                    smallButton("trash") { model.moveToTrash(asset) }
                } else {
                    Button("Restore") { model.restore(asset) }
                        .buttonStyle(.borderless)
                    Spacer()
                    smallButton("trash.slash") { model.permanentlyDelete(asset) }
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

    private func smallButton(_ systemName: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: systemName)
        }
        .buttonStyle(.borderless)
        .help(systemName)
    }
}
