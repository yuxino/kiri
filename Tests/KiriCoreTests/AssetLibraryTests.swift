import Foundation
@testable import KiriCore

private func temporaryLibraryURL() -> URL {
    FileManager.default.temporaryDirectory
        .appendingPathComponent("kiri-tests-\(UUID().uuidString)", isDirectory: true)
}

func importPersistsAcrossLibraryInstances() async throws {
    let temporaryURL = temporaryLibraryURL()
    defer { try? FileManager.default.removeItem(at: temporaryURL) }
    let library = try AssetLibrary(rootURL: temporaryURL)
    let asset = try await library.importData(
        Data("png".utf8),
        kind: .image,
        fileExtension: "png",
        pixelWidth: 320,
        pixelHeight: 180,
        sourceApplication: "Notes"
    )

    let url = await library.assetURL(for: asset)
    try expect(FileManager.default.fileExists(atPath: url.path), "Imported file should exist")
    let reopened = try AssetLibrary(rootURL: temporaryURL)
    let assets = await reopened.allAssets()
    try expect(assets == [asset], "Index should persist across instances")
}

func favoriteTrashRestoreAndDelete() async throws {
    let temporaryURL = temporaryLibraryURL()
    defer { try? FileManager.default.removeItem(at: temporaryURL) }
    let library = try AssetLibrary(rootURL: temporaryURL)
    let asset = try await library.importData(
        Data("png".utf8),
        kind: .image,
        fileExtension: "png",
        pixelWidth: 10,
        pixelHeight: 10
    )

    try await library.setFavorite(true, id: asset.id)
    let favorites = await library.allAssets()
    try expect(favorites.first?.isFavorite == true, "Favorite should persist")

    try await library.moveToTrash(id: asset.id)
    let visibleAfterTrash = await library.allAssets()
    let trashResults = await library.search("", includeTrashed: true)
    try expect(visibleAfterTrash.isEmpty, "Trashed asset should leave the library view")
    try expect(trashResults.map(\.id) == [asset.id], "Trashed asset should remain recoverable")

    try await library.restore(id: asset.id)
    let visibleAfterRestore = await library.allAssets()
    try expect(visibleAfterRestore.map(\.id) == [asset.id], "Restored asset should be visible")

    try await library.moveToTrash(id: asset.id)
    try await library.permanentlyDelete(id: asset.id)
    let allAfterDelete = await library.allAssets(includeTrashed: true)
    try expect(allAfterDelete.isEmpty, "Permanent deletion should remove metadata")
}

func searchFiltersBySourceApplication() async throws {
    let temporaryURL = temporaryLibraryURL()
    defer { try? FileManager.default.removeItem(at: temporaryURL) }
    let library = try AssetLibrary(rootURL: temporaryURL)
    _ = try await library.importData(
        Data("png".utf8),
        kind: .image,
        fileExtension: "png",
        pixelWidth: 10,
        pixelHeight: 10,
        sourceApplication: "Preview"
    )
    let preview = await library.search("preview")
    let safari = await library.search("safari")
    try expect(preview.count == 1, "Search should find source application")
    try expect(safari.isEmpty, "Search should exclude non-matches")
}

func replacementKeepsStableAssetURL() async throws {
    let temporaryURL = temporaryLibraryURL()
    defer { try? FileManager.default.removeItem(at: temporaryURL) }
    let library = try AssetLibrary(rootURL: temporaryURL)
    let asset = try await library.importData(
        Data("before".utf8),
        kind: .image,
        fileExtension: "png",
        pixelWidth: 10,
        pixelHeight: 10
    )
    let originalURL = await library.assetURL(for: asset)

    let replaced = try await library.replaceData(
        Data("after".utf8),
        for: asset.id
    )
    let replacementURL = await library.assetURL(for: replaced)

    try expect(replaced == asset, "Replacing image bytes should preserve asset metadata")
    try expect(replacementURL == originalURL, "Replacement should keep the stable asset URL")
    try expect(
        try Data(contentsOf: replacementURL) == Data("after".utf8),
        "Replacement should atomically update the stored bytes"
    )
}

func fileImportCopiesMediaWithoutRemovingTheSource() async throws {
    let temporaryURL = temporaryLibraryURL()
    defer { try? FileManager.default.removeItem(at: temporaryURL) }
    try FileManager.default.createDirectory(at: temporaryURL, withIntermediateDirectories: true)
    let sourceURL = temporaryURL.appendingPathComponent("recording.mp4")
    let bytes = Data(repeating: 7, count: 1_024)
    try bytes.write(to: sourceURL)
    let libraryURL = temporaryURL.appendingPathComponent("Library", isDirectory: true)
    let library = try AssetLibrary(rootURL: libraryURL)

    let asset = try await library.importFile(
        at: sourceURL,
        kind: .video,
        fileExtension: "mp4",
        pixelWidth: 640,
        pixelHeight: 360,
        duration: 2.5
    )
    let importedURL = await library.assetURL(for: asset)

    try expect(asset.kind == .video, "File import should preserve the media kind")
    try expect(asset.duration == 2.5, "File import should preserve duration")
    try expect(try Data(contentsOf: importedURL) == bytes, "File import should copy the bytes")
    try expect(FileManager.default.fileExists(atPath: sourceURL.path), "Source should remain recoverable")
}
