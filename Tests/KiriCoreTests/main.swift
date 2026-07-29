import Darwin

let tests = [
    KiriTest(name: "capture kinds round-trip") {
        try allCaptureKindsRoundTrip()
    },
    KiriTest(name: "asset searchable text") {
        try searchableTextIncludesFilenameKindAndApplication()
    },
    KiriTest(name: "capture shortcut labels") {
        try captureShortcutPresetsHaveStableLabels()
    },
    KiriTest(name: "capture shortcut round-trip") {
        try captureShortcutRoundTrips()
    },
    KiriTest(name: "capture shortcut modifiers") {
        try captureShortcutExposesNormalizedModifiers()
    },
    KiriTest(name: "capture shortcut legacy migration") {
        try captureShortcutMigratesLegacyPreset()
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
    }
]

do {
    try await runTests(tests)
} catch {
    testLog("\n\(error)")
    exit(1)
}
