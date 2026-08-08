import CoreGraphics
import Foundation

public struct LongScreenshotStitcherConfiguration: Equatable, Sendable {
    public var maxOverlapFraction: Double
    public var minimumOverlapPixels: Int
    public var maximumOutputHeight: Int

    public init(
        maxOverlapFraction: Double = 0.4,
        minimumOverlapPixels: Int = 8,
        maximumOutputHeight: Int = 100_000
    ) {
        if maxOverlapFraction.isFinite {
            self.maxOverlapFraction = min(max(0, maxOverlapFraction), 1)
        } else {
            self.maxOverlapFraction = 0
        }
        self.minimumOverlapPixels = max(0, minimumOverlapPixels)
        self.maximumOutputHeight = max(1, maximumOutputHeight)
    }
}

public enum LongScreenshotStitcherError: LocalizedError, Equatable, Sendable {
    case emptyInput
    case invalidImage(index: Int)
    case outputTooTall(maximumHeight: Int, requiredHeight: Int)
    case outputCreationFailed

    public var errorDescription: String? {
        switch self {
        case .emptyInput:
            KiriCoreL10n.text("At least one image is required to create a long screenshot.")
        case let .invalidImage(index):
            KiriCoreL10n.text("The image at position \(index) is invalid.")
        case let .outputTooTall(maximumHeight, requiredHeight):
            KiriCoreL10n.text(
                "The stitched screenshot would be \(requiredHeight) pixels tall, exceeding the \(maximumHeight)-pixel limit."
            )
        case .outputCreationFailed:
            KiriCoreL10n.text("The stitched screenshot could not be created.")
        }
    }
}

public struct LongScreenshotStitchingResult {
    public let image: CGImage
    public let detectedOverlaps: [Int]

    public var overlaps: [Int] {
        detectedOverlaps
    }

    public init(image: CGImage, detectedOverlaps: [Int]) {
        self.image = image
        self.detectedOverlaps = detectedOverlaps
    }
}

public struct LongScreenshotStitcher {
    public typealias Configuration = LongScreenshotStitcherConfiguration
    public typealias Result = LongScreenshotStitchingResult
    public typealias Error = LongScreenshotStitcherError

    public let configuration: Configuration

    public init(configuration: Configuration = Configuration()) {
        self.configuration = configuration
    }

    public func stitch(_ sections: [CGImage]) throws -> Result {
        guard !sections.isEmpty else {
            throw Error.emptyInput
        }

        let stitchWidth = sections.map(\.width).min() ?? 0
        guard stitchWidth > 0 else {
            throw Error.invalidImage(index: 0)
        }

        let preparedSections = try sections.enumerated().map { index, image in
            try prepare(image, index: index, stitchWidth: stitchWidth)
        }

        var detectedOverlaps: [Int] = []
        detectedOverlaps.reserveCapacity(max(0, preparedSections.count - 1))
        for index in 1..<preparedSections.count {
            detectedOverlaps.append(
                detectOverlap(
                    between: preparedSections[index - 1],
                    and: preparedSections[index]
                )
            )
        }

        let outputHeight = try outputHeight(
            for: preparedSections,
            overlaps: detectedOverlaps
        )
        let bytesPerRow = stitchWidth.multipliedReportingOverflow(by: 4)
        guard !bytesPerRow.overflow else {
            throw Error.outputCreationFailed
        }

        guard let context = CGContext(
            data: nil,
            width: stitchWidth,
            height: outputHeight,
            bitsPerComponent: 8,
            bytesPerRow: bytesPerRow.partialValue,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else {
            throw Error.outputCreationFailed
        }

        context.interpolationQuality = .none
        var topOffset = 0
        for index in preparedSections.indices {
            let section = preparedSections[index]
            let overlap = index == 0 ? 0 : detectedOverlaps[index - 1]
            let visibleHeight = section.height - overlap
            let destinationY = topOffset

            context.saveGState()
            context.clip(to: CGRect(
                x: 0,
                y: destinationY,
                width: stitchWidth,
                height: visibleHeight
            ))
            context.draw(
                section.image,
                in: CGRect(
                    x: 0,
                    y: destinationY - overlap,
                    width: stitchWidth,
                    height: section.height
                )
            )
            context.restoreGState()

            topOffset += visibleHeight
        }

        guard let image = context.makeImage() else {
            throw Error.outputCreationFailed
        }
        return Result(image: image, detectedOverlaps: detectedOverlaps)
    }

    public static func stitch(
        _ sections: [CGImage],
        configuration: Configuration = Configuration()
    ) throws -> Result {
        try Self(configuration: configuration).stitch(sections)
    }

    private struct PreparedSection {
        let image: CGImage
        let width: Int
        let height: Int
        let grayscale: GrayscaleSamples
    }

    private struct GrayscaleSamples {
        let width: Int
        let height: Int
        let bytesPerRow: Int
        let pixels: [UInt8]

        func value(x: Int, yFromTop: Int) -> UInt8 {
            pixels[(height - 1 - yFromTop) * bytesPerRow + x]
        }
    }

    private func prepare(
        _ image: CGImage,
        index: Int,
        stitchWidth: Int
    ) throws -> PreparedSection {
        guard image.width > 0, image.height > 0, image.dataProvider != nil else {
            throw Error.invalidImage(index: index)
        }
        guard let grayscale = makeGrayscaleSamples(for: image, stitchWidth: stitchWidth) else {
            throw Error.invalidImage(index: index)
        }
        return PreparedSection(
            image: image,
            width: stitchWidth,
            height: image.height,
            grayscale: grayscale
        )
    }

    private func outputHeight(
        for sections: [PreparedSection],
        overlaps: [Int]
    ) throws -> Int {
        var height = sections[0].height
        guard height <= configuration.maximumOutputHeight else {
            throw Error.outputTooTall(
                maximumHeight: configuration.maximumOutputHeight,
                requiredHeight: height
            )
        }
        for index in 1..<sections.count {
            let addedHeight = sections[index].height - overlaps[index - 1]
            let (nextHeight, overflow) = height.addingReportingOverflow(addedHeight)
            guard !overflow else {
                throw Error.outputTooTall(
                    maximumHeight: configuration.maximumOutputHeight,
                    requiredHeight: Int.max
                )
            }
            guard nextHeight <= configuration.maximumOutputHeight else {
                throw Error.outputTooTall(
                    maximumHeight: configuration.maximumOutputHeight,
                    requiredHeight: nextHeight
                )
            }
            height = nextHeight
        }
        return height
    }

    private func detectOverlap(
        between first: PreparedSection,
        and second: PreparedSection
    ) -> Int {
        let smallestHeight = min(first.height, second.height)
        let maximumByFraction = Int(
            (Double(smallestHeight) * configuration.maxOverlapFraction).rounded(.down)
        )
        let maximumOverlap = min(smallestHeight, maximumByFraction)
        let minimumOverlap = max(1, configuration.minimumOverlapPixels)
        guard maximumOverlap >= minimumOverlap else {
            return 0
        }

        var bestOverlap = 0
        var bestScore = Double.infinity
        var secondBestScore = Double.infinity

        for overlap in minimumOverlap...maximumOverlap {
            let score = matchScore(
                between: first.grayscale,
                and: second.grayscale,
                overlap: overlap
            )
            if score < bestScore {
                secondBestScore = bestScore
                bestScore = score
                bestOverlap = overlap
            } else if score < secondBestScore {
                secondBestScore = score
            }
        }

        guard bestOverlap > 0, bestScore <= 0.12 else {
            return 0
        }

        let detail = detailScore(
            in: first.grayscale,
            startingAt: first.height - bestOverlap,
            length: bestOverlap
        )
        let hasMeaningfulDetail = detail >= 0.01
        let hasDistinctMatch = secondBestScore.isFinite && secondBestScore - bestScore >= 0.02
        guard hasMeaningfulDetail && (hasDistinctMatch || bestScore <= 0.01) else {
            return 0
        }
        return bestOverlap
    }

    private func matchScore(
        between first: GrayscaleSamples,
        and second: GrayscaleSamples,
        overlap: Int
    ) -> Double {
        let sampleCount = min(64, overlap)
        let width = min(first.width, second.width)
        guard sampleCount > 0, width > 0 else { return Double.infinity }

        var difference = 0.0
        for sampleIndex in 0..<sampleCount {
            let offset = sampleCount == 1
                ? 0
                : Int(
                    (Double(sampleIndex) * Double(overlap - 1)
                        / Double(sampleCount - 1)).rounded()
                )
            let firstY = first.height - overlap + offset
            let secondY = offset
            for x in 0..<width {
                difference += Double(abs(
                    Int(first.value(x: x, yFromTop: firstY))
                        - Int(second.value(x: x, yFromTop: secondY))
                ))
            }
        }
        return difference / Double(sampleCount * width * 255)
    }

    private func detailScore(
        in samples: GrayscaleSamples,
        startingAt startY: Int,
        length: Int
    ) -> Double {
        let endY = startY + length
        var variation = 0.0
        var comparisons = 0

        if length > 1 {
            for y in startY..<(endY - 1) {
                for x in 0..<samples.width {
                    variation += Double(abs(
                        Int(samples.value(x: x, yFromTop: y))
                            - Int(samples.value(x: x, yFromTop: y + 1))
                    ))
                    comparisons += 1
                }
            }
        }
        if samples.width > 1 {
            for y in startY..<endY {
                for x in 0..<(samples.width - 1) {
                    variation += Double(abs(
                        Int(samples.value(x: x, yFromTop: y))
                            - Int(samples.value(x: x + 1, yFromTop: y))
                    ))
                    comparisons += 1
                }
            }
        }
        guard comparisons > 0 else { return 0 }
        return variation / Double(comparisons * 255)
    }

    private func makeGrayscaleSamples(
        for image: CGImage,
        stitchWidth: Int
    ) -> GrayscaleSamples? {
        let sampleWidth = min(64, stitchWidth)
        guard sampleWidth > 0 else { return nil }
        let bytesPerRow = sampleWidth
        let pixelCount = bytesPerRow.multipliedReportingOverflow(by: image.height)
        guard !pixelCount.overflow else { return nil }

        var pixels = [UInt8](repeating: 0, count: pixelCount.partialValue)
        let rendered = pixels.withUnsafeMutableBytes { buffer -> Bool in
            guard let context = CGContext(
                data: buffer.baseAddress,
                width: sampleWidth,
                height: image.height,
                bitsPerComponent: 8,
                bytesPerRow: bytesPerRow,
                space: CGColorSpaceCreateDeviceGray(),
                bitmapInfo: CGImageAlphaInfo.none.rawValue
            ) else {
                return false
            }
            context.interpolationQuality = .high
            context.draw(
                image,
                in: CGRect(
                    x: 0,
                    y: 0,
                    width: sampleWidth,
                    height: image.height
                )
            )
            return true
        }
        guard rendered else { return nil }
        return GrayscaleSamples(
            width: sampleWidth,
            height: image.height,
            bytesPerRow: bytesPerRow,
            pixels: pixels
        )
    }
}
