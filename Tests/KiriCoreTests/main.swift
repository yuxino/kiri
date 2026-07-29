import Darwin

let tests = [
    KiriTest(name: "capture kinds round-trip") {
        try allCaptureKindsRoundTrip()
    },
    KiriTest(name: "asset searchable text") {
        try searchableTextIncludesFilenameKindAndApplication()
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
    }
]

do {
    try await runTests(tests)
} catch {
    testLog("\n\(error)")
    exit(1)
}
