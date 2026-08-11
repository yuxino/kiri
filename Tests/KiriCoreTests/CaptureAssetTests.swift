import Foundation
@testable import KiriCore

func allCaptureKindsRoundTrip() throws {
    for kind in CaptureKind.allCases {
        let asset = CaptureAsset(
            kind: kind,
            filename: "capture.dat",
            pixelWidth: 1920,
            pixelHeight: 1080,
            duration: kind == .video ? 3.2 : nil,
            sourceApplication: "Safari"
        )
        let data = try JSONEncoder().encode(asset)
        try expect(
            try JSONDecoder().decode(CaptureAsset.self, from: data) == asset,
            "\(kind.rawValue) should survive JSON round-trip"
        )
    }
}

func searchableTextIncludesFilenameKindAndApplication() throws {
    let asset = CaptureAsset(
        kind: .image,
        filename: "Article.PNG",
        pixelWidth: 1200,
        pixelHeight: 8000,
        sourceApplication: "Safari"
    )
    try expect(asset.searchableText.contains("article.png"), "Search should include filename")
    try expect(asset.searchableText.contains("image"), "Search should include kind")
    try expect(asset.searchableText.contains("safari"), "Search should include source app")
}

func legacyLongImageKindDecodesAsImage() throws {
    let json = """
    {
      "id": "00000000-0000-0000-0000-000000000001",
      "kind": "longImage",
      "createdAt": 0,
      "filename": "legacy.png",
      "pixelWidth": 1200,
      "pixelHeight": 8000,
      "isFavorite": false
    }
    """.data(using: .utf8)!
    let asset = try JSONDecoder().decode(CaptureAsset.self, from: json)
    try expect(asset.kind == .image, "Legacy long-image metadata should open as a normal image")
}
