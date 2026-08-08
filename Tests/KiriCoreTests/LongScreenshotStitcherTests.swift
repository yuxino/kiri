import CoreGraphics
@testable import KiriCore

func longScreenshotRejectsEmptyInput() throws {
    do {
        _ = try LongScreenshotStitcher().stitch([])
        throw TestFailure(description: "Empty input should throw")
    } catch LongScreenshotStitcherError.emptyInput {
        return
    }
}

func longScreenshotKeepsHeightWhenThereIsNoOverlap() throws {
    let first = makeTestImage(width: 4, rows: [20, 60, 100, 140])
    let second = makeTestImage(width: 4, rows: [180, 220, 250])
    let stitcher = LongScreenshotStitcher(configuration: .init(
        maxOverlapFraction: 0.75,
        minimumOverlapPixels: 1
    ))

    let result = try stitcher.stitch([first, second])

    try expect(result.detectedOverlaps == [0], "Unmatched sections should report no overlap")
    try expect(result.image.width == 4, "Equal-width sections should keep their width")
    try expect(result.image.height == 7, "No overlap should sum section heights")
}

func longScreenshotDetectsOverlapAndPreservesPixelOrder() throws {
    let first = makeTestImage(width: 4, rows: [25, 75, 125, 175, 225])
    let second = makeTestImage(width: 4, rows: [175, 225, 35, 95])
    let stitcher = LongScreenshotStitcher(configuration: .init(
        maxOverlapFraction: 0.8,
        minimumOverlapPixels: 1
    ))

    let result = try stitcher.stitch([first, second])

    try expect(result.detectedOverlaps == [2], "The repeated bottom and top rows should overlap by two pixels")
    try expect(result.image.height == 7, "Detected overlap should be removed from the second section")
    try expect(
        testImageRows(result.image) == [25, 75, 125, 175, 225, 35, 95],
        "The output should keep rows in top-to-bottom order"
    )
}

func longScreenshotHandlesDifferentSizesAndHeightCap() throws {
    let first = makeTestImage(width: 5, rows: [15, 55, 95])
    let second = makeTestImage(width: 3, rows: [135, 175, 215, 245])
    let configuration = LongScreenshotStitcher.Configuration(
        maxOverlapFraction: 0.25,
        minimumOverlapPixels: 1,
        maximumOutputHeight: 20
    )
    let result = try LongScreenshotStitcher(configuration: configuration).stitch([first, second])

    try expect(result.image.width == 3, "Stitching should use the minimum section width")
    try expect(result.image.height == 7, "Different section sizes should remain vertically intact")

    do {
        _ = try LongScreenshotStitcher(configuration: .init(
            maxOverlapFraction: 0.25,
            minimumOverlapPixels: 1,
            maximumOutputHeight: 6
        )).stitch([first, second])
        throw TestFailure(description: "The output-height safety limit should throw")
    } catch LongScreenshotStitcherError.outputTooTall {
        // Continue to verify the single-section safety check below.
    }

    do {
        _ = try LongScreenshotStitcher(configuration: .init(
            maximumOutputHeight: 2
        )).stitch([first])
        throw TestFailure(description: "A single section over the output-height limit should throw")
    } catch LongScreenshotStitcherError.outputTooTall {
        return
    }
}

private func makeTestImage(width: Int, rows: [UInt8]) -> CGImage {
    let colorSpace = CGColorSpaceCreateDeviceRGB()
    let context = CGContext(
        data: nil,
        width: width,
        height: rows.count,
        bitsPerComponent: 8,
        bytesPerRow: width * 4,
        space: colorSpace,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    )!

    for (topRow, value) in rows.enumerated() {
        context.setFillColor(red: CGFloat(value) / 255, green: 0, blue: 0, alpha: 1)
        context.fill(CGRect(
            x: 0,
            y: CGFloat(topRow),
            width: CGFloat(width),
            height: 1
        ))
    }
    return context.makeImage()!
}

private func testImageRows(_ image: CGImage) -> [UInt8] {
    let width = image.width
    let height = image.height
    let context = CGContext(
        data: nil,
        width: width,
        height: height,
        bitsPerComponent: 8,
        bytesPerRow: width * 4,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    )!
    context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
    let bytes = context.data!.assumingMemoryBound(to: UInt8.self)
    return (0..<height).map { topRow in
        let rowStart = (height - topRow - 1) * width * 4
        return bytes[rowStart]
    }
}
