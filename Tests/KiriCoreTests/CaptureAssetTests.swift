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
        kind: .longImage,
        filename: "Article.PNG",
        pixelWidth: 1200,
        pixelHeight: 8000,
        sourceApplication: "Safari"
    )
    try expect(asset.searchableText.contains("article.png"), "Search should include filename")
    try expect(asset.searchableText.contains("longimage"), "Search should include kind")
    try expect(asset.searchableText.contains("safari"), "Search should include source app")
}
