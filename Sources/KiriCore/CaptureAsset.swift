import Foundation

public enum CaptureKind: String, Codable, CaseIterable, Sendable {
    case image
    case video
    case gif
    case longImage
}

public struct CaptureAsset: Codable, Identifiable, Equatable, Sendable {
    public let id: UUID
    public let kind: CaptureKind
    public let createdAt: Date
    public let filename: String
    public let pixelWidth: Int
    public let pixelHeight: Int
    public let duration: TimeInterval?
    public let sourceApplication: String?
    public var isFavorite: Bool
    public var trashedAt: Date?

    public init(
        id: UUID = UUID(),
        kind: CaptureKind,
        createdAt: Date = Date(),
        filename: String,
        pixelWidth: Int,
        pixelHeight: Int,
        duration: TimeInterval? = nil,
        sourceApplication: String? = nil,
        isFavorite: Bool = false,
        trashedAt: Date? = nil
    ) {
        self.id = id
        self.kind = kind
        self.createdAt = createdAt
        self.filename = filename
        self.pixelWidth = pixelWidth
        self.pixelHeight = pixelHeight
        self.duration = duration
        self.sourceApplication = sourceApplication
        self.isFavorite = isFavorite
        self.trashedAt = trashedAt
    }

    public var searchableText: String {
        [filename, sourceApplication, kind.rawValue]
            .compactMap { $0 }
            .joined(separator: " ")
            .lowercased()
    }
}

