@preconcurrency import AVFoundation
import CoreMedia
import Foundation

enum RecordingSegmentMergerError: LocalizedError {
    case noSegments
    case cannotCreateTrack
    case cannotCreateExporter
    case exportFailed(String)

    var errorDescription: String? {
        switch self {
        case .noSegments:
            "No recording segments are available."
        case .cannotCreateTrack:
            "Kiri could not prepare the paused recording for export."
        case .cannotCreateExporter:
            "Kiri could not prepare the final MP4 export."
        case let .exportFailed(message):
            "The paused recording could not be merged: \(message)"
        }
    }
}

enum RecordingSegmentMerger {
    static func merge(_ segments: [RecordedMedia]) async throws -> RecordedMedia {
        guard let first = segments.first else {
            throw RecordingSegmentMergerError.noSegments
        }
        guard segments.count > 1 else { return first }

        let composition = AVMutableComposition()
        var videoDestinations: [AVMutableCompositionTrack] = []
        var audioDestinations: [AVMutableCompositionTrack] = []
        var insertionTime = CMTime.zero

        for segment in segments {
            let asset = AVURLAsset(url: segment.fileURL)
            let duration = try await asset.load(.duration)
            let timeRange = CMTimeRange(start: .zero, duration: duration)
            let videoTracks = try await asset.loadTracks(withMediaType: .video)
            let audioTracks = try await asset.loadTracks(withMediaType: .audio)

            try append(
                videoTracks,
                to: &videoDestinations,
                in: composition,
                mediaType: .video,
                timeRange: timeRange,
                at: insertionTime
            )
            try append(
                audioTracks,
                to: &audioDestinations,
                in: composition,
                mediaType: .audio,
                timeRange: timeRange,
                at: insertionTime
            )
            insertionTime = CMTimeAdd(insertionTime, duration)
        }

        let outputURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("kiri-recording-merged-\(UUID().uuidString.lowercased()).mp4")
        guard let exporter = AVAssetExportSession(
            asset: composition,
            presetName: AVAssetExportPresetHighestQuality
        ) else {
            throw RecordingSegmentMergerError.cannotCreateExporter
        }

        do {
            if #available(macOS 15.0, *) {
                try await exporter.export(to: outputURL, as: .mp4)
            } else {
                try await exportLegacy(exporter, to: outputURL)
            }
        } catch {
            try? FileManager.default.removeItem(at: outputURL)
            throw error
        }

        return RecordedMedia(
            fileURL: outputURL,
            pixelWidth: first.pixelWidth,
            pixelHeight: first.pixelHeight,
            duration: max(0, CMTimeGetSeconds(insertionTime))
        )
    }

    private static func append(
        _ sourceTracks: [AVAssetTrack],
        to destinations: inout [AVMutableCompositionTrack],
        in composition: AVMutableComposition,
        mediaType: AVMediaType,
        timeRange: CMTimeRange,
        at insertionTime: CMTime
    ) throws {
        while destinations.count < sourceTracks.count {
            guard let track = composition.addMutableTrack(
                withMediaType: mediaType,
                preferredTrackID: kCMPersistentTrackID_Invalid
            ) else {
                throw RecordingSegmentMergerError.cannotCreateTrack
            }
            destinations.append(track)
        }
        for (index, sourceTrack) in sourceTracks.enumerated() {
            try destinations[index].insertTimeRange(timeRange, of: sourceTrack, at: insertionTime)
        }
    }

    @available(macOS, introduced: 14.0, obsoleted: 15.0)
    private static func exportLegacy(
        _ exporter: AVAssetExportSession,
        to outputURL: URL
    ) async throws {
        exporter.outputURL = outputURL
        exporter.outputFileType = .mp4
        await exporter.export()
        guard exporter.status == .completed else {
            throw RecordingSegmentMergerError.exportFailed(
                exporter.error?.localizedDescription ?? "Unknown export error"
            )
        }
    }
}
