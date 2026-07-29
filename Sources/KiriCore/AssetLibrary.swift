import Foundation

public enum AssetLibraryError: LocalizedError, Sendable {
    case assetNotFound
    case invalidFilename

    public var errorDescription: String? {
        switch self {
        case .assetNotFound:
            "The capture could not be found."
        case .invalidFilename:
            "The capture filename is invalid."
        }
    }
}

public actor AssetLibrary {
    public let rootURL: URL
    public let assetsURL: URL
    public let thumbnailsURL: URL
    private let indexURL: URL
    private var index: [CaptureAsset]

    public init(rootURL: URL) throws {
        self.rootURL = rootURL
        assetsURL = rootURL.appendingPathComponent("Assets", isDirectory: true)
        thumbnailsURL = rootURL.appendingPathComponent("Thumbnails", isDirectory: true)
        indexURL = rootURL.appendingPathComponent("library.json")

        let manager = FileManager.default
        try manager.createDirectory(at: assetsURL, withIntermediateDirectories: true)
        try manager.createDirectory(at: thumbnailsURL, withIntermediateDirectories: true)

        if manager.fileExists(atPath: indexURL.path) {
            let data = try Data(contentsOf: indexURL)
            index = try JSONDecoder.kiri.decode([CaptureAsset].self, from: data)
        } else {
            index = []
        }
    }

    public static func defaultRootURL() throws -> URL {
        let support = try FileManager.default.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        return support.appendingPathComponent("kiri", isDirectory: true)
    }

    public func allAssets(includeTrashed: Bool = false) -> [CaptureAsset] {
        index
            .filter { includeTrashed || $0.trashedAt == nil }
            .sorted { $0.createdAt > $1.createdAt }
    }

    public func search(_ query: String, includeTrashed: Bool = false) -> [CaptureAsset] {
        let normalized = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return allAssets(includeTrashed: includeTrashed).filter { asset in
            let trashMatches = includeTrashed ? asset.trashedAt != nil : asset.trashedAt == nil
            return trashMatches && (normalized.isEmpty || asset.searchableText.contains(normalized))
        }
    }

    @discardableResult
    public func importData(
        _ data: Data,
        kind: CaptureKind,
        fileExtension: String,
        pixelWidth: Int,
        pixelHeight: Int,
        duration: TimeInterval? = nil,
        sourceApplication: String? = nil,
        createdAt: Date = Date()
    ) throws -> CaptureAsset {
        let safeExtension = fileExtension
            .trimmingCharacters(in: CharacterSet(charactersIn: "."))
            .lowercased()
        guard !safeExtension.isEmpty,
              safeExtension.allSatisfy({ $0.isLetter || $0.isNumber }) else {
            throw AssetLibraryError.invalidFilename
        }

        let persistedCreatedAt = Date(
            timeIntervalSince1970: (createdAt.timeIntervalSince1970 * 1_000).rounded() / 1_000
        )
        let id = UUID()
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        let filename = "\(formatter.string(from: persistedCreatedAt))-\(id.uuidString.lowercased()).\(safeExtension)"
        let fileURL = assetsURL.appendingPathComponent(filename)
        try data.write(to: fileURL, options: [.atomic])

        let asset = CaptureAsset(
            id: id,
            kind: kind,
            createdAt: persistedCreatedAt,
            filename: filename,
            pixelWidth: pixelWidth,
            pixelHeight: pixelHeight,
            duration: duration,
            sourceApplication: sourceApplication
        )
        index.append(asset)

        do {
            try persist()
        } catch {
            try? FileManager.default.removeItem(at: fileURL)
            index.removeAll { $0.id == id }
            throw error
        }
        return asset
    }

    public func assetURL(for asset: CaptureAsset) -> URL {
        assetsURL.appendingPathComponent(asset.filename)
    }

    @discardableResult
    public func replaceData(_ data: Data, for id: UUID) throws -> CaptureAsset {
        guard let asset = index.first(where: { $0.id == id }) else {
            throw AssetLibraryError.assetNotFound
        }
        try data.write(to: assetURL(for: asset), options: [.atomic])
        return asset
    }

    public func setFavorite(_ favorite: Bool, id: UUID) throws {
        try update(id: id) { $0.isFavorite = favorite }
    }

    public func moveToTrash(id: UUID, at date: Date = Date()) throws {
        try update(id: id) { $0.trashedAt = date }
    }

    public func restore(id: UUID) throws {
        try update(id: id) { $0.trashedAt = nil }
    }

    public func permanentlyDelete(id: UUID) throws {
        guard let position = index.firstIndex(where: { $0.id == id }) else {
            throw AssetLibraryError.assetNotFound
        }
        let asset = index.remove(at: position)
        try persist()
        try? FileManager.default.removeItem(at: assetURL(for: asset))
        try? FileManager.default.removeItem(
            at: thumbnailsURL.appendingPathComponent("\(asset.id.uuidString.lowercased()).jpg")
        )
    }

    private func update(id: UUID, mutation: (inout CaptureAsset) -> Void) throws {
        guard let position = index.firstIndex(where: { $0.id == id }) else {
            throw AssetLibraryError.assetNotFound
        }
        let previous = index[position]
        mutation(&index[position])
        do {
            try persist()
        } catch {
            index[position] = previous
            throw error
        }
    }

    private func persist() throws {
        let data = try JSONEncoder.kiri.encode(index)
        try data.write(to: indexURL, options: [.atomic])
    }
}

private extension JSONEncoder {
    static var kiri: JSONEncoder {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        encoder.dateEncodingStrategy = .millisecondsSince1970
        return encoder
    }
}

private extension JSONDecoder {
    static var kiri: JSONDecoder {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .millisecondsSince1970
        return decoder
    }
}
