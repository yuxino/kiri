import CoreGraphics
@testable import KiriCore

func normalizesReverseDrag() throws {
    let rect = SelectionGeometry.normalized(
        from: CGPoint(x: 120, y: 90),
        to: CGPoint(x: 20, y: 10)
    )
    try expect(rect == CGRect(x: 20, y: 10, width: 100, height: 80), "Reverse drag should normalize")
}

func clampsSelectionToDisplay() throws {
    let result = SelectionGeometry.clamped(
        CGRect(x: -20, y: 30, width: 80, height: 90),
        to: CGRect(x: 0, y: 0, width: 100, height: 100)
    )
    try expect(result == CGRect(x: 0, y: 30, width: 60, height: 70), "Selection should clamp")
}

func convertsTopLeftViewRectToPixels() throws {
    let result = SelectionGeometry.pixelRect(
        forTopLeftRect: CGRect(x: 50, y: 25, width: 100, height: 50),
        canvasSize: CGSize(width: 200, height: 100),
        imageSize: CGSize(width: 400, height: 200)
    )
    try expect(result == CGRect(x: 100, y: 50, width: 200, height: 100), "Top-left crop should scale")
}

func convertsBottomLeftScreenCoordinatesToTopLeftPixels() throws {
    let result = SelectionGeometry.pixelRect(
        forScreenRect: CGRect(x: 110, y: 220, width: 50, height: 40),
        displayFrame: CGRect(x: 100, y: 200, width: 300, height: 200),
        scale: 2
    )
    try expect(
        result == CGRect(x: 20, y: 280, width: 100, height: 80),
        "Screen coordinates should flip and scale"
    )
}

func rejectsTinySelection() throws {
    try expect(
        !SelectionGeometry.isValid(CGRect(x: 0, y: 0, width: 2, height: 20)),
        "Tiny selection should be rejected"
    )
    try expect(
        SelectionGeometry.isValid(CGRect(x: 0, y: 0, width: 3, height: 3)),
        "Minimum selection should be accepted"
    )
}

func hitTestsSelectionHandles() throws {
    let selection = CGRect(x: 20, y: 30, width: 100, height: 80)
    try expect(
        SelectionGeometry.hitTest(
            CGPoint(x: 20, y: 30),
            selection: selection,
            radius: 6
        ) == .topLeft,
        "Top-left handle should win at the corner"
    )
    try expect(
        SelectionGeometry.hitTest(
            CGPoint(x: 70, y: 30),
            selection: selection,
            radius: 6
        ) == .top,
        "Top midpoint should hit the top handle"
    )
    try expect(
        SelectionGeometry.hitTest(
            CGPoint(x: 120, y: 70),
            selection: selection,
            radius: 6
        ) == .right,
        "Right midpoint should hit the right handle"
    )
    try expect(
        SelectionGeometry.hitTest(
            CGPoint(x: 70, y: 70),
            selection: selection,
            radius: 6
        ) == nil,
        "Selection center should not hit a resize handle"
    )
}

func resizesSelectionFromHandles() throws {
    let selection = CGRect(x: 10, y: 10, width: 100, height: 80)
    let bounds = CGRect(x: 0, y: 0, width: 200, height: 200)

    let expanded = SelectionGeometry.resized(
        selection,
        using: .topLeft,
        to: CGPoint(x: 0, y: 5),
        within: bounds,
        minimumSide: 8
    )
    try expect(
        expanded == CGRect(x: 0, y: 5, width: 110, height: 85),
        "Top-left resize should preserve the opposite corner"
    )

    let minimum = SelectionGeometry.resized(
        selection,
        using: .left,
        to: CGPoint(x: 180, y: 50),
        within: bounds,
        minimumSide: 8
    )
    try expect(
        minimum == CGRect(x: 102, y: 10, width: 8, height: 80),
        "Resize should preserve the minimum side"
    )
}

func movesSelectionWithinBounds() throws {
    let selection = CGRect(x: 10, y: 20, width: 50, height: 40)
    let bounds = CGRect(x: 0, y: 0, width: 200, height: 200)

    let moved = SelectionGeometry.moved(
        selection,
        by: CGSize(width: -50, height: 100),
        within: bounds
    )
    try expect(
        moved == CGRect(x: 0, y: 120, width: 50, height: 40),
        "Move should clamp without changing selection size"
    )
}

func convertsSelectionsOnOffsetDisplays() throws {
    let leftDisplay = CGRect(x: -1440, y: 0, width: 1440, height: 900)
    let leftResult = SelectionGeometry.pixelRect(
        forScreenRect: CGRect(x: -1400, y: 100, width: 200, height: 150),
        displayFrame: leftDisplay,
        scale: 2
    )
    try expect(
        leftResult == CGRect(x: 80, y: 1300, width: 400, height: 300),
        "Negative display origins should convert relative to that display"
    )

    let upperDisplay = CGRect(x: 0, y: 900, width: 1920, height: 1080)
    let upperResult = SelectionGeometry.pixelRect(
        forScreenRect: CGRect(x: 100, y: 1000, width: 300, height: 200),
        displayFrame: upperDisplay,
        scale: 1
    )
    try expect(
        upperResult == CGRect(x: 100, y: 780, width: 300, height: 200),
        "Vertically offset displays should use their own maximum Y"
    )
}
