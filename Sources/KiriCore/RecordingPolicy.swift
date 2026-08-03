import Foundation

public struct RecordingOptions: Codable, Equatable, Sendable {
    public var usesCountdown: Bool
    public var capturesSystemAudio: Bool
    public var capturesMicrophone: Bool
    public var showsCursor: Bool
    public var highlightsClicks: Bool

    public init(
        usesCountdown: Bool = true,
        capturesSystemAudio: Bool = false,
        capturesMicrophone: Bool = false,
        showsCursor: Bool = true,
        highlightsClicks: Bool = false
    ) {
        self.usesCountdown = usesCountdown
        self.capturesSystemAudio = capturesSystemAudio
        self.capturesMicrophone = capturesMicrophone
        self.showsCursor = showsCursor
        self.highlightsClicks = highlightsClicks
    }

    public var normalized: RecordingOptions {
        var result = self
        if !result.showsCursor {
            result.highlightsClicks = false
        }
        return result
    }
}

public enum RecordingPolicy {
    public static let framesPerSecond = 30
    public static let countdownSeconds = 3
    public static let maximumGIFDuration: TimeInterval = 15
    public static let gifFramesPerSecond = 12
    public static let maximumGIFLongEdge = 720

    public static func evenDimension(_ value: Int) -> Int {
        let positive = max(2, value)
        return positive.isMultiple(of: 2) ? positive : positive - 1
    }

    public static func isGIFEligible(duration: TimeInterval?) -> Bool {
        guard let duration else { return false }
        return duration > 0 && duration <= maximumGIFDuration
    }

    public static func gifFrameCount(duration: TimeInterval) -> Int {
        guard duration > 0 else { return 0 }
        return max(1, Int(ceil(duration * Double(gifFramesPerSecond))))
    }

    public static func elapsedLabel(_ duration: TimeInterval) -> String {
        let totalSeconds = max(0, Int(duration.rounded(.down)))
        let hours = totalSeconds / 3_600
        let minutes = (totalSeconds % 3_600) / 60
        let seconds = totalSeconds % 60
        if hours > 0 {
            return String(format: "%d:%02d:%02d", hours, minutes, seconds)
        }
        return String(format: "%02d:%02d", minutes, seconds)
    }
}
