@testable import KiriCore

func annotationHistoryStartsEmpty() throws {
    let history = AnnotationHistory<Int>()

    try expect(history.elements.isEmpty, "A new history should contain no elements")
    try expect(!history.canUndo, "A new history should not allow undo")
    try expect(!history.canRedo, "A new history should not allow redo")
}

func annotationHistorySupportsUndoAndRedo() throws {
    var history = AnnotationHistory<Int>()
    history.append(1)
    history.append(2)

    try expect(history.elements == [1, 2], "Appended elements should preserve order")
    try expect(history.undo() == 2, "Undo should remove the newest element")
    try expect(history.elements == [1], "Undo should update visible elements")
    try expect(history.canRedo, "Undo should make redo available")
    try expect(history.redo() == 2, "Redo should restore the newest undone element")
    try expect(history.elements == [1, 2], "Redo should restore element order")
}

func annotationHistoryInvalidatesRedoBranch() throws {
    var history = AnnotationHistory<Int>()
    history.append(1)
    history.append(2)
    _ = history.undo()
    history.append(3)

    try expect(history.elements == [1, 3], "A new edit should replace the undone branch")
    try expect(!history.canRedo, "A new edit should invalidate redo")
    try expect(history.redo() == nil, "Redo should do nothing after branch invalidation")
}

func annotationHistoryClearsAllState() throws {
    var history = AnnotationHistory<Int>()
    history.append(1)
    history.append(2)
    _ = history.undo()
    history.clear()

    try expect(history.elements.isEmpty, "Clear should remove visible elements")
    try expect(!history.canUndo, "Clear should remove undo availability")
    try expect(!history.canRedo, "Clear should remove redo availability")
}

func annotationHistoryReplacesElementsWithUndoAndRedo() throws {
    var history = AnnotationHistory<String>()
    history.append("original")

    try expect(
        history.replace(at: 0, with: "edited") == "original",
        "Replace should return the previous element"
    )
    try expect(history.elements == ["edited"], "Replace should update the indexed element")
    _ = history.undo()
    try expect(history.elements == ["original"], "Undo should restore the previous element")
    _ = history.redo()
    try expect(history.elements == ["edited"], "Redo should restore the replacement")
}
