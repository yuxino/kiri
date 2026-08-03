@testable import KiriCore

func recordingDimensionsArePositiveAndEven() throws {
    try expect(RecordingPolicy.evenDimension(1) == 2, "Tiny dimensions should become valid")
    try expect(RecordingPolicy.evenDimension(100) == 100, "Even dimensions should remain stable")
    try expect(RecordingPolicy.evenDimension(101) == 100, "Odd dimensions should round down")
}

func gifEligibilityIsBounded() throws {
    try expect(!RecordingPolicy.isGIFEligible(duration: nil), "Unknown duration should be rejected")
    try expect(!RecordingPolicy.isGIFEligible(duration: 0), "Empty video should be rejected")
    try expect(RecordingPolicy.isGIFEligible(duration: 15), "The duration limit should be included")
    try expect(!RecordingPolicy.isGIFEligible(duration: 15.01), "Long video should be rejected")
}

func gifFrameCountUsesTheConfiguredRate() throws {
    try expect(RecordingPolicy.gifFrameCount(duration: 0) == 0, "Empty video should have no frames")
    try expect(RecordingPolicy.gifFrameCount(duration: 1) == 12, "One second should use twelve frames")
    try expect(RecordingPolicy.gifFrameCount(duration: 1.01) == 13, "Partial frames should round up")
}

func recordingElapsedLabelsRemainCompact() throws {
    try expect(RecordingPolicy.elapsedLabel(0) == "00:00", "Zero should be formatted")
    try expect(RecordingPolicy.elapsedLabel(65.9) == "01:05", "Minutes should be formatted")
    try expect(RecordingPolicy.elapsedLabel(3_661) == "1:01:01", "Hours should be formatted")
}

func recordingOptionsUsePrivacyFriendlyDefaults() throws {
    let options = RecordingOptions()
    try expect(options.usesCountdown, "Countdown should be enabled by default")
    try expect(!options.capturesSystemAudio, "System audio should be opt-in")
    try expect(!options.capturesMicrophone, "Microphone should be opt-in")
    try expect(options.showsCursor, "Pointer should be visible by default")
    try expect(!options.highlightsClicks, "Click highlights should be opt-in")
}

func recordingOptionsKeepClickFeedbackConsistent() throws {
    let options = RecordingOptions(showsCursor: false, highlightsClicks: true).normalized
    try expect(!options.showsCursor, "Pointer preference should remain off")
    try expect(!options.highlightsClicks, "Click highlights require a visible pointer")
}
