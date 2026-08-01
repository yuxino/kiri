import AppKit
import ImageIO
import KiriCore
import SwiftUI

@main
struct LibrarySnapshotMain {
    @MainActor
    static func main() async throws {
        NSApplication.shared.setActivationPolicy(.prohibited)

        guard ProcessInfo.processInfo.arguments.count == 6,
              let width = Double(ProcessInfo.processInfo.arguments[4]),
              let height = Double(ProcessInfo.processInfo.arguments[5]) else {
            throw SnapshotError.invalidArguments
        }
        let libraryRoot = URL(
            fileURLWithPath: ProcessInfo.processInfo.arguments[1],
            isDirectory: true
        )
        let outputURL = URL(fileURLWithPath: ProcessInfo.processInfo.arguments[2])
        let mode = ProcessInfo.processInfo.arguments[3]
        try FileManager.default.createDirectory(
            at: libraryRoot,
            withIntermediateDirectories: true
        )

        let library = try AssetLibrary(rootURL: libraryRoot)
        var importedAssets: [CaptureAsset] = []
        if !["empty", "loading"].contains(mode) {
            for index in 0..<6 {
                let pixelWidth = 900 + index * 80
                let pixelHeight = 560 + (index % 3) * 100
                let image = makeFixtureImage(
                    width: pixelWidth,
                    height: pixelHeight,
                    index: index
                )
                guard let data = NSBitmapImageRep(cgImage: image)
                    .representation(using: .png, properties: [:]) else {
                    throw SnapshotError.imageEncodingFailed
                }
                let asset = try await library.importData(
                    data,
                    kind: .image,
                    fileExtension: "png",
                    pixelWidth: pixelWidth,
                    pixelHeight: pixelHeight,
                    sourceApplication: ["Safari", "Xcode", "Notes"][index % 3]
                )
                importedAssets.append(asset)
            }
        }
        if mode == "trash" {
            for asset in importedAssets.prefix(3) {
                try await library.moveToTrash(id: asset.id)
            }
        }

        let model = AppModel()
        if mode != "loading" {
            await model.refresh()
        }
        switch mode {
        case "trash":
            model.showingTrash = true
        case "search":
            model.searchQuery = "no matching fixture"
        case "error":
            model.errorMessage = "Kiri could not access this capture. Try again."
        case "populated", "compact", "dark", "empty", "loading":
            break
        default:
            throw SnapshotError.invalidMode(mode)
        }

        let size = CGSize(width: width, height: height)
        let hostingView = NSHostingView(
            rootView: LibraryView(model: model)
                .frame(width: size.width, height: size.height)
        )
        if mode == "dark" {
            hostingView.appearance = NSAppearance(named: .darkAqua)
        }
        hostingView.frame = CGRect(origin: .zero, size: size)
        hostingView.layoutSubtreeIfNeeded()
        try? await Task.sleep(for: .milliseconds(450))
        hostingView.layoutSubtreeIfNeeded()

        guard let bitmap = hostingView.bitmapImageRepForCachingDisplay(in: hostingView.bounds) else {
            throw SnapshotError.renderingFailed
        }
        hostingView.cacheDisplay(in: hostingView.bounds, to: bitmap)
        guard let png = bitmap.representation(using: .png, properties: [:]) else {
            throw SnapshotError.imageEncodingFailed
        }
        try png.write(to: outputURL, options: [.atomic])
    }

    private static func makeFixtureImage(width: Int, height: Int, index: Int) -> CGImage {
        let colorSpace = CGColorSpaceCreateDeviceRGB()
        let context = CGContext(
            data: nil,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: 0,
            space: colorSpace,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        )!
        let palettes: [(CGColor, CGColor)] = [
            (
                CGColor(red: 0.10, green: 0.18, blue: 0.32, alpha: 1),
                CGColor(red: 0.28, green: 0.55, blue: 0.96, alpha: 1)
            ),
            (
                CGColor(red: 0.95, green: 0.91, blue: 0.84, alpha: 1),
                CGColor(red: 0.83, green: 0.38, blue: 0.25, alpha: 1)
            ),
            (
                CGColor(red: 0.12, green: 0.26, blue: 0.22, alpha: 1),
                CGColor(red: 0.34, green: 0.77, blue: 0.60, alpha: 1)
            )
        ]
        let palette = palettes[index % palettes.count]
        context.setFillColor(palette.0)
        context.fill(CGRect(x: 0, y: 0, width: width, height: height))
        context.setFillColor(palette.1)
        context.fill(
            CGRect(
                x: width / 10,
                y: height / 6,
                width: width * 4 / 5,
                height: height * 2 / 3
            )
        )
        context.setFillColor(CGColor(gray: 1, alpha: 0.86))
        for row in 0..<4 {
            context.fill(
                CGRect(
                    x: width / 6,
                    y: height / 4 + row * height / 10,
                    width: width / (row + 2),
                    height: max(8, height / 45)
                )
            )
        }
        return context.makeImage()!
    }
}

private enum SnapshotError: Error {
    case invalidArguments
    case invalidMode(String)
    case imageEncodingFailed
    case renderingFailed
}
