@preconcurrency import AVFoundation
import CoreGraphics
import Foundation
import ImageIO
import KiriCore
import UniformTypeIdentifiers

struct ExportedGIF: Sendable {
    let fileURL: URL
    let pixelWidth: Int
    let pixelHeight: Int
    let duration: TimeInterval
}

enum GIFExporterError: LocalizedError, Sendable {
    case videoTrackUnavailable
    case durationUnavailable
    case durationTooLong
    case destinationUnavailable
    case frameGenerationFailed
    case finalizeFailed

    var errorDescription: String? {
        switch self {
        case .videoTrackUnavailable:
            L10n.text("The video track could not be read.")
        case .durationUnavailable:
            L10n.text("The video duration is unavailable.")
        case .durationTooLong:
            L10n.text("GIF conversion currently supports videos up to 15 seconds.")
        case .destinationUnavailable:
            L10n.text("Kiri could not create the GIF file.")
        case .frameGenerationFailed:
            L10n.text("Kiri could not extract a video frame.")
        case .finalizeFailed:
            L10n.text("The GIF could not be finalized.")
        }
    }
}

enum GIFExporter {
    static func export(videoAt sourceURL: URL) async throws -> ExportedGIF {
        let asset = AVURLAsset(url: sourceURL)
        let durationTime = try await asset.load(.duration)
        let duration = CMTimeGetSeconds(durationTime)
        guard duration.isFinite, duration > 0 else {
            throw GIFExporterError.durationUnavailable
        }
        guard RecordingPolicy.isGIFEligible(duration: duration) else {
            throw GIFExporterError.durationTooLong
        }
        guard let track = try await asset.loadTracks(withMediaType: .video).first else {
            throw GIFExporterError.videoTrackUnavailable
        }
        let naturalSize = try await track.load(.naturalSize)
        let transform = try await track.load(.preferredTransform)
        let transformedSize = naturalSize.applying(transform)
        let sourceWidth = abs(transformedSize.width)
        let sourceHeight = abs(transformedSize.height)
        guard sourceWidth > 0, sourceHeight > 0 else {
            throw GIFExporterError.videoTrackUnavailable
        }
        let scale = min(
            1,
            CGFloat(RecordingPolicy.maximumGIFLongEdge) / max(sourceWidth, sourceHeight)
        )
        let targetSize = CGSize(
            width: max(1, (sourceWidth * scale).rounded()),
            height: max(1, (sourceHeight * scale).rounded())
        )
        let frameCount = RecordingPolicy.gifFrameCount(duration: duration)
        let outputURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("kiri-gif-\(UUID().uuidString.lowercased()).gif")
        guard let destination = CGImageDestinationCreateWithURL(
            outputURL as CFURL,
            UTType.gif.identifier as CFString,
            frameCount,
            nil
        ) else {
            throw GIFExporterError.destinationUnavailable
        }
        CGImageDestinationSetProperties(
            destination,
            [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFLoopCount: 0]] as CFDictionary
        )

        let generator = AVAssetImageGenerator(asset: asset)
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = targetSize
        generator.requestedTimeToleranceBefore = .zero
        generator.requestedTimeToleranceAfter = CMTime(
            value: 1,
            timescale: CMTimeScale(RecordingPolicy.gifFramesPerSecond)
        )
        let frameDelay = 1 / Double(RecordingPolicy.gifFramesPerSecond)
        let frameProperties = [
            kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFDelayTime: frameDelay]
        ] as CFDictionary
        var outputSize: CGSize?
        do {
            for index in 0..<frameCount {
                try Task.checkCancellation()
                let seconds = min(
                    duration - 0.001,
                    Double(index) / Double(RecordingPolicy.gifFramesPerSecond)
                )
                let result = try await generator.image(
                    at: CMTime(seconds: max(0, seconds), preferredTimescale: 600)
                )
                outputSize = CGSize(width: result.image.width, height: result.image.height)
                CGImageDestinationAddImage(destination, result.image, frameProperties)
            }
        } catch {
            try? FileManager.default.removeItem(at: outputURL)
            throw error
        }
        guard CGImageDestinationFinalize(destination), let outputSize else {
            try? FileManager.default.removeItem(at: outputURL)
            throw GIFExporterError.finalizeFailed
        }
        return ExportedGIF(
            fileURL: outputURL,
            pixelWidth: Int(outputSize.width),
            pixelHeight: Int(outputSize.height),
            duration: duration
        )
    }
}
