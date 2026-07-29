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
