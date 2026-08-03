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

private protocol RegionRecordingBackend: AnyObject, Sendable {
    func start(
        displayID: CGDirectDisplayID,
        sourceRect: CGRect,
        backingScale: CGFloat,
        options: RecordingOptions
    ) async throws

    func stop() async throws -> RecordedMedia
}

final class RegionRecorder: @unchecked Sendable {
    private var backend: (any RegionRecordingBackend)?

    func start(
        displayID: CGDirectDisplayID,
        sourceRect: CGRect,
        backingScale: CGFloat,
        options: RecordingOptions
    ) async throws {
        let backend: any RegionRecordingBackend
        if #available(macOS 15.0, *) {
            backend = ModernRegionRecordingBackend()
        } else {
            backend = LegacyRegionRecordingBackend()
        }
        self.backend = backend
        do {
            try await backend.start(
                displayID: displayID,
                sourceRect: sourceRect,
                backingScale: backingScale,
                options: options.normalized
            )
        } catch {
            self.backend = nil
            throw error
        }
    }

    func stop() async throws -> RecordedMedia {
        guard let backend else { throw RegionRecorderError.noFrames }
        defer { self.backend = nil }
        return try await backend.stop()
    }
}

@available(macOS 15.0, *)
private final class ModernRegionRecordingBackend: NSObject,
    RegionRecordingBackend,
    SCRecordingOutputDelegate,
    SCStreamDelegate,
    @unchecked Sendable {
    private let stateQueue = DispatchQueue(label: "io.yuxino.kiri.modern-region-recorder")
    private var stream: SCStream?
    private var recordingOutput: SCRecordingOutput?
    private var outputURL: URL?
    private var pixelWidth = 0
    private var pixelHeight = 0
    private var completionResult: Result<Void, Error>?
    private var completionContinuation: CheckedContinuation<Void, Error>?

    func start(
        displayID: CGDirectDisplayID,
        sourceRect: CGRect,
        backingScale: CGFloat,
        options: RecordingOptions
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
        configuration.pixelFormat = kCVPixelFormatType_32BGRA
        configuration.showsCursor = options.showsCursor
        configuration.showMouseClicks = options.highlightsClicks
        configuration.capturesAudio = options.capturesSystemAudio
        configuration.excludesCurrentProcessAudio = true
        configuration.sampleRate = 48_000
        configuration.channelCount = 2
        configuration.captureMicrophone = options.capturesMicrophone

        let temporaryURL = Self.makeTemporaryURL()
        let outputConfiguration = SCRecordingOutputConfiguration()
        outputConfiguration.outputURL = temporaryURL
        outputConfiguration.outputFileType = .mp4
        outputConfiguration.videoCodecType = .h264
        let recordingOutput = SCRecordingOutput(
            configuration: outputConfiguration,
            delegate: self
        )
        let stream = SCStream(filter: filter, configuration: configuration, delegate: self)
        try stream.addRecordingOutput(recordingOutput)
        stateQueue.sync {
            self.stream = stream
            self.recordingOutput = recordingOutput
            outputURL = temporaryURL
            pixelWidth = width
            pixelHeight = height
            completionResult = nil
            completionContinuation = nil
        }
        do {
            try await stream.startCapture()
        } catch {
            reset(removeTemporaryFile: true)
            throw error
        }
    }

    func stop() async throws -> RecordedMedia {
        guard let stream = stateQueue.sync(execute: { self.stream }) else {
            throw RegionRecorderError.noFrames
        }
        do {
            try await stream.stopCapture()
            try await waitForRecordingCompletion()
            guard let outputURL = stateQueue.sync(execute: { self.outputURL }) else {
                throw RegionRecorderError.noFrames
            }
            let asset = AVURLAsset(url: outputURL)
            let duration = try await asset.load(.duration)
            let media = RecordedMedia(
                fileURL: outputURL,
                pixelWidth: stateQueue.sync(execute: { pixelWidth }),
                pixelHeight: stateQueue.sync(execute: { pixelHeight }),
                duration: max(0, CMTimeGetSeconds(duration))
            )
            reset(removeTemporaryFile: false)
            return media
        } catch {
            reset(removeTemporaryFile: true)
            throw error
        }
    }

    func recordingOutputDidStartRecording(_ recordingOutput: SCRecordingOutput) {}

    func recordingOutputDidFinishRecording(_ recordingOutput: SCRecordingOutput) {
        stateQueue.async { [weak self] in
            self?.finish(with: .success(()))
        }
    }

    func recordingOutput(_ recordingOutput: SCRecordingOutput, didFailWithError error: Error) {
        stateQueue.async { [weak self] in
            self?.finish(with: .failure(error))
        }
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        stateQueue.async { [weak self] in
            self?.finish(with: .failure(error))
        }
    }

    private func waitForRecordingCompletion() async throws {
        try await withCheckedThrowingContinuation { continuation in
            stateQueue.async { [self] in
                if let completionResult {
                    self.completionResult = nil
                    continuation.resume(with: completionResult)
                } else {
                    completionContinuation = continuation
                }
            }
        }
    }

    private func finish(with result: Result<Void, Error>) {
        if let continuation = completionContinuation {
            completionContinuation = nil
            continuation.resume(with: result)
        } else if completionResult == nil {
            completionResult = result
        }
    }

    private func reset(removeTemporaryFile: Bool) {
        stateQueue.sync {
            if removeTemporaryFile, let outputURL {
                try? FileManager.default.removeItem(at: outputURL)
            }
            stream = nil
            recordingOutput = nil
            outputURL = nil
            pixelWidth = 0
            pixelHeight = 0
            completionResult = nil
            completionContinuation = nil
        }
    }

    private static func makeTemporaryURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("kiri-recording-\(UUID().uuidString.lowercased()).mp4")
    }
}

private final class LegacyRegionRecordingBackend: NSObject,
    RegionRecordingBackend,
    SCStreamOutput,
    SCStreamDelegate,
    @unchecked Sendable {
    private let sampleQueue = DispatchQueue(label: "io.yuxino.kiri.legacy-region-recorder")
    private var stream: SCStream?
    private var writer: AVAssetWriter?
    private var videoInput: AVAssetWriterInput?
    private var audioInput: AVAssetWriterInput?
    private var outputURL: URL?
    private var firstTimestamp: CMTime?
    private var lastTimestamp: CMTime?
    private var pixelWidth = 0
    private var pixelHeight = 0
    private var streamFailure: Error?

    func start(
        displayID: CGDirectDisplayID,
        sourceRect: CGRect,
        backingScale: CGFloat,
        options: RecordingOptions
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
        configuration.showsCursor = options.showsCursor
        configuration.capturesAudio = options.capturesSystemAudio
        configuration.excludesCurrentProcessAudio = true
        configuration.sampleRate = 48_000
        configuration.channelCount = 2

        let temporaryURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("kiri-recording-\(UUID().uuidString.lowercased()).mp4")
        let writer = try AVAssetWriter(outputURL: temporaryURL, fileType: .mp4)
        let videoInput = AVAssetWriterInput(
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
        videoInput.expectsMediaDataInRealTime = true
        guard writer.canAdd(videoInput) else {
            throw RegionRecorderError.writerSetupFailed
        }
        writer.add(videoInput)

        var audioInput: AVAssetWriterInput?
        if options.capturesSystemAudio {
            let input = AVAssetWriterInput(
                mediaType: .audio,
                outputSettings: [
                    AVFormatIDKey: kAudioFormatMPEG4AAC,
                    AVSampleRateKey: 48_000,
                    AVNumberOfChannelsKey: 2,
                    AVEncoderBitRateKey: 192_000
                ]
            )
            input.expectsMediaDataInRealTime = true
            guard writer.canAdd(input) else {
                throw RegionRecorderError.writerSetupFailed
            }
            writer.add(input)
            audioInput = input
        }

        let stream = SCStream(filter: filter, configuration: configuration, delegate: self)
        try stream.addStreamOutput(self, type: .screen, sampleHandlerQueue: sampleQueue)
        if options.capturesSystemAudio {
            try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: sampleQueue)
        }
        sampleQueue.sync {
            self.stream = stream
            self.writer = writer
            self.videoInput = videoInput
            self.audioInput = audioInput
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
                audioInput?.markAsFinished()
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
        guard sampleBuffer.isValid, CMSampleBufferDataIsReady(sampleBuffer), let writer else {
            return
        }
        let timestamp = CMSampleBufferGetPresentationTimeStamp(sampleBuffer)
        switch type {
        case .screen:
            guard let videoInput else { return }
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
                streamFailure = writer.error
                    ?? RegionRecorderError.writerFailed("Frame append failed")
            }
        case .audio:
            guard firstTimestamp != nil,
                  let audioInput,
                  audioInput.isReadyForMoreMediaData else { return }
            if !audioInput.append(sampleBuffer) {
                streamFailure = writer.error
                    ?? RegionRecorderError.writerFailed("Audio append failed")
            }
        case .microphone:
            break
        @unknown default:
            break
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
        audioInput = nil
        outputURL = nil
        firstTimestamp = nil
        lastTimestamp = nil
        pixelWidth = 0
        pixelHeight = 0
        streamFailure = nil
    }
}
