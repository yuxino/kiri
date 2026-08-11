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

    for (index, value) in rows.enumerated() {
        context.setFillColor(red: CGFloat(value) / 255, green: 0, blue: 0, alpha: 1)
        context.fill(CGRect(
            x: 0,
            y: CGFloat(rows.count - 1 - index),
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
    return (0..<height).map { rowIndex in
        let rowStart = rowIndex * width * 4
        return bytes[rowStart]
    }
}

func longScreenshotOverlapDetectorFindsSharedRows() throws {
    let first = makeTestImage(width: 4, rows: [25, 75, 125, 175, 225])
    let second = makeTestImage(width: 4, rows: [175, 225, 35, 95])
    let configuration = LongScreenshotStitcher.Configuration(
        maxOverlapFraction: 0.8,
        minimumOverlapPixels: 1
    )

    let overlap = LongScreenshotOverlapDetector.detectOverlap(
        between: first,
        and: second,
        configuration: configuration
    )

    try expect(overlap == 2, "The repeated bottom and top rows should be detected as overlap")
}

func longScreenshotOverlapDetectorReturnsZeroForDistinctContent() throws {
    let first = makeTestImage(width: 4, rows: [10, 20, 30, 40])
    let second = makeTestImage(width: 4, rows: [110, 120, 130, 140])

    let overlap = LongScreenshotOverlapDetector.detectOverlap(
        between: first,
        and: second
    )

    try expect(overlap == 0, "Distinct content should report no overlap")
}

func longScreenshotOverlapDetectorHandlesLargeOverlap() throws {
    // 10-row first section; second shares its bottom five rows and adds three new ones.
    let first = makeTestImage(width: 4, rows: [10, 20, 30, 40, 50, 60, 70, 80, 90, 100])
    let second = makeTestImage(width: 4, rows: [60, 70, 80, 90, 100, 110, 120, 130])
    let configuration = LongScreenshotStitcher.Configuration(
        maxOverlapFraction: 0.95,
        minimumOverlapPixels: 1
    )

    let overlap = LongScreenshotOverlapDetector.detectOverlap(
        between: first,
        and: second,
        configuration: configuration
    )

    try expect(overlap == 5, "A recording-style capture should find a large overlap when configured")
}

func longScreenshotOverlapDetectorHandlesScrolledRealisticContent() throws {
    // A tall page with text-like variation; two viewport frames scrolled by 60px.
    let page = makeScrollingPage(width: 120, height: 1200)
    let first = page.cropping(to: CGRect(x: 0, y: 0, width: 120, height: 240))!
    let second = page.cropping(to: CGRect(x: 0, y: 60, width: 120, height: 240))!
    let configuration = LongScreenshotStitcher.Configuration(
        maxOverlapFraction: 0.95,
        minimumOverlapPixels: 8
    )

    let overlap = LongScreenshotOverlapDetector.detectOverlap(
        between: first,
        and: second,
        configuration: configuration
    )

    try expect(
        overlap == 180,
        "A 60px scroll in a 240px viewport should overlap by 180 pixels"
    )
}

func longScreenshotStitcherOutputStaysTopToBottom() throws {
    // Regression: stitched output must keep sections in top-to-bottom order
    // and remove only the overlapping rows.
    let first = makeTestImage(width: 4, rows: [25, 75, 125, 175, 225])
    let second = makeTestImage(width: 4, rows: [175, 225, 35, 95])
    let stitcher = LongScreenshotStitcher(configuration: .init(
        maxOverlapFraction: 0.8,
        minimumOverlapPixels: 1
    ))

    let result = try stitcher.stitch([first, second])

    try expect(result.detectedOverlaps == [2], "The two overlapping rows should be detected")
    try expect(
        testImageRows(result.image) == [25, 75, 125, 175, 225, 35, 95],
        "The stitched image should stay in top-to-bottom order with overlap removed"
    )
}

func longScreenshotUsesCaptureTimeOverlaps() throws {
    let first = makeTestImage(width: 4, rows: [10, 20, 30, 40, 50, 60, 70, 80, 90, 100])
    let second = makeTestImage(width: 4, rows: [30, 40, 50, 60, 70, 80, 90, 100, 110, 120])

    // The default detector intentionally searches only 40% of a viewport, but
    // live scrolling commonly leaves a much larger overlap. Export must use
    // the seam measured by the capture session instead of detecting it again.
    let result = try LongScreenshotStitcher.stitch(
        [first, second],
        overlaps: [8]
    )

    try expect(result.detectedOverlaps == [8], "Capture-time overlap should be preserved")
    try expect(result.image.height == 12, "A large live overlap should not duplicate a viewport")
    try expect(
        testImageRows(result.image) == [10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120],
        "Explicit seams should preserve the captured page order"
    )
}

func longScreenshotRejectsInvalidCaptureTimeOverlaps() throws {
    let first = makeTestImage(width: 4, rows: [10, 20, 30])
    let second = makeTestImage(width: 4, rows: [20, 30, 40])

    do {
        _ = try LongScreenshotStitcher.stitch([first, second], overlaps: [])
        throw TestFailure(description: "A missing capture-time overlap should throw")
    } catch LongScreenshotStitcherError.invalidOverlapCount {
        // Expected.
    }

    do {
        _ = try LongScreenshotStitcher.stitch([first, second], overlaps: [3])
        throw TestFailure(description: "An overlap that removes a whole section should throw")
    } catch LongScreenshotStitcherError.invalidOverlap {
        return
    }
}

private func makeScrollingPage(width: Int, height: Int) -> CGImage {
    let colorSpace = CGColorSpaceCreateDeviceRGB()
    let context = CGContext(
        data: nil,
        width: width,
        height: height,
        bitsPerComponent: 8,
        bytesPerRow: width * 4,
        space: colorSpace,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    )!
    var seed: UInt64 = 0x9E3779B97F4A7C15
    func rand() -> Double {
        seed = seed &* 6364136223846793005 &+ 1442695040888963407
        return Double((seed >> 33) & 0xFFFF) / Double(0xFFFF)
    }
    for y in 0..<height {
        let base: CGFloat = 0.25 + 0.5 * CGFloat((y / 60) % 3) / 3
        for x in 0..<width {
            var value = min(1, max(0, base + CGFloat((rand() - 0.5) * 0.3)))
            if y % 23 == 0 { value = 0.9 }
            if x % 9 == 0, (y / 5) % 2 == 0 { value = 0.1 }
            context.setFillColor(
                red: value,
                green: value * 0.85,
                blue: value * 0.7,
                alpha: 1
            )
            context.fill(CGRect(
                x: CGFloat(x),
                y: CGFloat(height - 1 - y),
                width: 1,
                height: 1
            ))
        }
    }
    return context.makeImage()!
}
