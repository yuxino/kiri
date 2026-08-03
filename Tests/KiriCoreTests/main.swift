import Darwin

let tests = [
    KiriTest(name: "annotation history empty state") {
        try annotationHistoryStartsEmpty()
    },
    KiriTest(name: "annotation history undo and redo") {
        try annotationHistorySupportsUndoAndRedo()
    },
    KiriTest(name: "annotation history branch invalidation") {
        try annotationHistoryInvalidatesRedoBranch()
    },
    KiriTest(name: "annotation history clear") {
        try annotationHistoryClearsAllState()
    },
    KiriTest(name: "annotation history indexed replacement") {
        try annotationHistoryReplacesElementsWithUndoAndRedo()
    },
    KiriTest(name: "annotation history indexed removal") {
        try annotationHistoryRemovesElementsWithUndoAndRedo()
    },
    KiriTest(name: "capture kinds round-trip") {
        try allCaptureKindsRoundTrip()
    },
    KiriTest(name: "asset searchable text") {
        try searchableTextIncludesFilenameKindAndApplication()
    },
    KiriTest(name: "capture shortcut labels") {
        try captureShortcutHasStableLabel()
    },
    KiriTest(name: "capture shortcut round-trip") {
        try captureShortcutRoundTrips()
    },
    KiriTest(name: "capture shortcut modifiers") {
        try captureShortcutExposesNormalizedModifiers()
    },
    KiriTest(name: "library import persistence") {
        try await importPersistsAcrossLibraryInstances()
    },
    KiriTest(name: "library favorite, trash, restore, delete") {
        try await favoriteTrashRestoreAndDelete()
    },
    KiriTest(name: "library source application search") {
        try await searchFiltersBySourceApplication()
    },
    KiriTest(name: "library replacement keeps stable URL") {
        try await replacementKeepsStableAssetURL()
    },
    KiriTest(name: "library imports finalized media files") {
        try await fileImportCopiesMediaWithoutRemovingTheSource()
    },
    KiriTest(name: "recording dimensions are positive and even") {
        try recordingDimensionsArePositiveAndEven()
    },
    KiriTest(name: "recording dimensions honor Retina scale") {
        try recordingDimensionsHonorRetinaScale()
    },
    KiriTest(name: "recording bitrate uses high-quality bounds") {
        try recordingBitRateUsesHighQualityBounds()
    },
    KiriTest(name: "GIF eligibility is bounded") {
        try gifEligibilityIsBounded()
    },
    KiriTest(name: "GIF frame count uses configured rate") {
        try gifFrameCountUsesTheConfiguredRate()
    },
    KiriTest(name: "recording elapsed labels remain compact") {
        try recordingElapsedLabelsRemainCompact()
    },
    KiriTest(name: "recording options use privacy-friendly defaults") {
        try recordingOptionsUsePrivacyFriendlyDefaults()
    },
    KiriTest(name: "recording click feedback stays consistent") {
        try recordingOptionsKeepClickFeedbackConsistent()
    },
    KiriTest(name: "selection reverse drag") {
        try normalizesReverseDrag()
    },
    KiriTest(name: "selection display clamping") {
        try clampsSelectionToDisplay()
    },
    KiriTest(name: "selection top-left pixel conversion") {
        try convertsTopLeftViewRectToPixels()
    },
    KiriTest(name: "selection screen coordinate conversion") {
        try convertsBottomLeftScreenCoordinatesToTopLeftPixels()
    },
    KiriTest(name: "selection minimum size") {
        try rejectsTinySelection()
    },
    KiriTest(name: "selection handle hit testing") {
        try hitTestsSelectionHandles()
    },
    KiriTest(name: "selection handle resizing") {
        try resizesSelectionFromHandles()
    },
    KiriTest(name: "selection movement clamping") {
        try movesSelectionWithinBounds()
    },
    KiriTest(name: "selection offset display conversion") {
        try convertsSelectionsOnOffsetDisplays()
    },
    KiriTest(name: "screen permission authorized") {
        try screenCapturePermissionSkipsRequestWhenAuthorized()
    },
    KiriTest(name: "screen permission granted cache") {
        try screenCapturePermissionCachesGrantedRequest()
    },
    KiriTest(name: "screen permission declined cache") {
        try screenCapturePermissionCachesDeclinedRequest()
    },
    KiriTest(name: "screen permission preflight override") {
        try screenCapturePermissionPreflightOverridesCache()
    }
]

do {
    try await runTests(tests)
} catch {
    testLog("\n\(error)")
    exit(1)
}
