@preconcurrency import AVFoundation
import CoreMedia
import CoreVideo
import Foundation
import KiriCore
@preconcurrency import ScreenCaptureKit

struct RecordedMedia: Sendable {
    let fileURL: URL
    let pixelWidth: Int
    let pixelHeight: Int
    let duration: TimeInterval
}

enum RegionRecorderError: LocalizedError, Sendable {
    case displayUnavailable
    case invalidRegion
    case writerSetupFailed
    case noFrames
    case writerFailed(String)

    var errorDescription: String? {
        switch self {
        case .displayUnavailable:
            "The selected display is no longer available."
        case .invalidRegion:
            "The recording region is too small."
        case .writerSetupFailed:
            "Kiri could not prepare the MP4 encoder."
        case .noFrames:
            "The recording ended before a complete frame arrived."
        case let .writerFailed(message):
            "The MP4 could not be finalized: \(message)"
        }
    }
}

final class RegionRecorder: NSObject, SCStreamOutput, SCStreamDelegate, @unchecked Sendable {
    private let sampleQueue = DispatchQueue(label: "io.yuxino.kiri.region-recorder")
    private var stream: SCStream?
    private var writer: AVAssetWriter?
    private var videoInput: AVAssetWriterInput?
    private var outputURL: URL?
    private var firstTimestamp: CMTime?
    private var lastTimestamp: CMTime?
    private var pixelWidth = 0
    private var pixelHeight = 0
    private var streamFailure: Error?

    func start(
        displayID: CGDirectDisplayID,
        sourceRect: CGRect,
        backingScale: CGFloat
    ) async throws {
        guard sourceRect.width >= 2, sourceRect.height >= 2 else {
            throw RegionRecorderError.invalidRegion
        }
        let content = try await SCShareableContent.excludingDesktopWindows(
            false,
            onScreenWindowsOnly: true
        )
        guard let display = content.displays.first(where: { $0.displayID == displayID }) else {
            throw RegionRecorderError.displayUnavailable
        }

        let currentProcessID = ProcessInfo.processInfo.processIdentifier
        let excludedWindows = content.windows.filter {
            $0.owningApplication?.processID == currentProcessID
        }
        let filter = SCContentFilter(display: display, excludingWindows: excludedWindows)
        let width = RecordingPolicy.evenDimension(
            Int((sourceRect.width * max(1, backingScale)).rounded())
        )
        let height = RecordingPolicy.evenDimension(
            Int((sourceRect.height * max(1, backingScale)).rounded())
        )
        let configuration = SCStreamConfiguration()
        configuration.sourceRect = sourceRect.standardized
        configuration.width = width
        configuration.height = height
        configuration.minimumFrameInterval = CMTime(
            value: 1,
            timescale: CMTimeScale(RecordingPolicy.framesPerSecond)
        )
        configuration.queueDepth = 6
        configuration.showsCursor = true
        configuration.capturesAudio = false

        let temporaryURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("kiri-recording-\(UUID().uuidString.lowercased()).mp4")
        let writer = try AVAssetWriter(outputURL: temporaryURL, fileType: .mp4)
        let input = AVAssetWriterInput(
            mediaType: .video,
            outputSettings: [
                AVVideoCodecKey: AVVideoCodecType.h264,
                AVVideoWidthKey: width,
                AVVideoHeightKey: height,
                AVVideoCompressionPropertiesKey: [
                    AVVideoAverageBitRateKey: max(1_000_000, width * height * 4),
                    AVVideoExpectedSourceFrameRateKey: RecordingPolicy.framesPerSecond,
                    AVVideoMaxKeyFrameIntervalKey: RecordingPolicy.framesPerSecond * 2
                ]
            ]
        )
        input.expectsMediaDataInRealTime = true
        guard writer.canAdd(input) else {
            throw RegionRecorderError.writerSetupFailed
        }
        writer.add(input)

        let stream = SCStream(filter: filter, configuration: configuration, delegate: self)
        try stream.addStreamOutput(self, type: .screen, sampleHandlerQueue: sampleQueue)
        sampleQueue.sync {
            self.stream = stream
            self.writer = writer
            videoInput = input
            outputURL = temporaryURL
            pixelWidth = width
            pixelHeight = height
            firstTimestamp = nil
            lastTimestamp = nil
            streamFailure = nil
        }
        do {
            try await stream.startCapture()
        } catch {
            resetAndRemoveTemporaryFile()
            throw error
        }
    }

    func stop() async throws -> RecordedMedia {
        guard let stream = sampleQueue.sync(execute: { self.stream }) else {
            throw RegionRecorderError.noFrames
        }
        try await stream.stopCapture()
        return try await withCheckedThrowingContinuation { continuation in
            sampleQueue.async { [self] in
                if let streamFailure {
                    resetAndRemoveTemporaryFile()
                    continuation.resume(throwing: streamFailure)
                    return
                }
                guard let writer,
                      let videoInput,
                      let outputURL,
                      let firstTimestamp,
                      let lastTimestamp else {
                    resetAndRemoveTemporaryFile()
                    continuation.resume(throwing: RegionRecorderError.noFrames)
                    return
                }
                videoInput.markAsFinished()
                writer.finishWriting { [self] in
                    sampleQueue.async { [self] in
                        guard let completedWriter = self.writer,
                              completedWriter.status == .completed else {
                            let message = self.writer?.error?.localizedDescription
                                ?? "Unknown encoder error"
                            resetAndRemoveTemporaryFile()
                            continuation.resume(
                                throwing: RegionRecorderError.writerFailed(message)
                            )
                            return
                        }
                        let duration = max(
                            0,
                            CMTimeGetSeconds(CMTimeSubtract(lastTimestamp, firstTimestamp))
                        )
                        let result = RecordedMedia(
                            fileURL: outputURL,
                            pixelWidth: pixelWidth,
                            pixelHeight: pixelHeight,
                            duration: duration
                        )
                        reset(keepTemporaryFile: true)
                        continuation.resume(returning: result)
                    }
                }
            }
        }
    }

    func stream(
        _ stream: SCStream,
        didOutputSampleBuffer sampleBuffer: CMSampleBuffer,
        of type: SCStreamOutputType
    ) {
        guard type == .screen,
              sampleBuffer.isValid,
              CMSampleBufferDataIsReady(sampleBuffer),
              let writer,
              let videoInput else { return }
        let timestamp = CMSampleBufferGetPresentationTimeStamp(sampleBuffer)
        if firstTimestamp == nil {
            guard writer.startWriting() else {
                streamFailure = writer.error ?? RegionRecorderError.writerSetupFailed
                return
            }
            writer.startSession(atSourceTime: timestamp)
            firstTimestamp = timestamp
        }
        guard videoInput.isReadyForMoreMediaData else { return }
        if videoInput.append(sampleBuffer) {
            lastTimestamp = timestamp
        } else {
            streamFailure = writer.error ?? RegionRecorderError.writerFailed("Frame append failed")
        }
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        sampleQueue.async { [weak self] in
            self?.streamFailure = error
        }
    }

    private func resetAndRemoveTemporaryFile() {
        reset(keepTemporaryFile: false)
    }

    private func reset(keepTemporaryFile: Bool) {
        if !keepTemporaryFile, let outputURL {
            try? FileManager.default.removeItem(at: outputURL)
        }
        stream = nil
        writer = nil
        videoInput = nil
        outputURL = nil
        firstTimestamp = nil
        lastTimestamp = nil
        pixelWidth = 0
        pixelHeight = 0
        streamFailure = nil
    }
}
