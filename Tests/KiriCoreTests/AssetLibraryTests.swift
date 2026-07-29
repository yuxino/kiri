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
