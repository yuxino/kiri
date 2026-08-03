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
    KiriTest(name: "selection confirms with inside click") {
        try confirmsAnExistingSelectionWithAClick()
    },
    KiriTest(name: "selection stays editable after adjustments") {
        try keepsNewOrAdjustedSelectionsEditable()
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
    KiriTest(name: "window snap topmost hit") {
        try windowSnapChoosesTopmostCandidate()
    },
    KiriTest(name: "window snap display clipping") {
        try windowSnapClipsToDisplay()
    },
    KiriTest(name: "window snap candidate filtering") {
        try windowSnapFiltersInvalidCandidates()
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
