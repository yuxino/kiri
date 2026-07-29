import CoreGraphics

public enum SelectionHandle: String, CaseIterable, Sendable {
    case topLeft
    case top
    case topRight
    case right
    case bottomRight
    case bottom
    case bottomLeft
    case left
}

public enum SelectionGeometry {
    public static func normalized(from start: CGPoint, to end: CGPoint) -> CGRect {
        CGRect(
            x: min(start.x, end.x),
            y: min(start.y, end.y),
            width: abs(end.x - start.x),
            height: abs(end.y - start.y)
        )
    }

    public static func clamped(_ rect: CGRect, to bounds: CGRect) -> CGRect {
        rect.standardized.intersection(bounds)
    }

    public static func isValid(_ rect: CGRect, minimumSide: CGFloat = 3) -> Bool {
        !rect.isNull && rect.width >= minimumSide && rect.height >= minimumSide
    }

    public static func pixelRect(
        forTopLeftRect rect: CGRect,
        canvasSize: CGSize,
        imageSize: CGSize
    ) -> CGRect {
        guard canvasSize.width > 0, canvasSize.height > 0 else { return .null }
        let scaleX = imageSize.width / canvasSize.width
        let scaleY = imageSize.height / canvasSize.height
        return CGRect(
            x: rect.minX * scaleX,
            y: rect.minY * scaleY,
            width: rect.width * scaleX,
            height: rect.height * scaleY
        ).integral
    }

    public static func pixelRect(
        forScreenRect rect: CGRect,
        displayFrame: CGRect,
        scale: CGFloat
    ) -> CGRect {
        CGRect(
            x: (rect.minX - displayFrame.minX) * scale,
            y: (displayFrame.maxY - rect.maxY) * scale,
            width: rect.width * scale,
            height: rect.height * scale
        ).integral
    }

    public static func handlePoint(
        for handle: SelectionHandle,
        in selection: CGRect
    ) -> CGPoint {
        let rect = selection.standardized
        return switch handle {
        case .topLeft:
            CGPoint(x: rect.minX, y: rect.minY)
        case .top:
            CGPoint(x: rect.midX, y: rect.minY)
        case .topRight:
            CGPoint(x: rect.maxX, y: rect.minY)
        case .right:
            CGPoint(x: rect.maxX, y: rect.midY)
        case .bottomRight:
            CGPoint(x: rect.maxX, y: rect.maxY)
        case .bottom:
            CGPoint(x: rect.midX, y: rect.maxY)
        case .bottomLeft:
            CGPoint(x: rect.minX, y: rect.maxY)
        case .left:
            CGPoint(x: rect.minX, y: rect.midY)
        }
    }

    public static func hitTest(
        _ point: CGPoint,
        selection: CGRect,
        radius: CGFloat = 8
    ) -> SelectionHandle? {
        guard isValid(selection), radius >= 0 else { return nil }
        return SelectionHandle.allCases.first { handle in
            let center = handlePoint(for: handle, in: selection)
            return hypot(point.x - center.x, point.y - center.y) <= radius
        }
    }

    public static func resized(
        _ selection: CGRect,
        using handle: SelectionHandle,
        to point: CGPoint,
        within bounds: CGRect,
        minimumSide: CGFloat = 8
    ) -> CGRect {
        let rect = selection.standardized
        let limits = bounds.standardized
        let minimum = max(1, minimumSide)
        let clampedPoint = CGPoint(
            x: min(max(point.x, limits.minX), limits.maxX),
            y: min(max(point.y, limits.minY), limits.maxY)
        )

        var minX = rect.minX
        var maxX = rect.maxX
        var minY = rect.minY
        var maxY = rect.maxY

        switch handle {
        case .topLeft, .left, .bottomLeft:
            minX = min(clampedPoint.x, maxX - minimum)
        case .topRight, .right, .bottomRight:
            maxX = max(clampedPoint.x, minX + minimum)
        case .top, .bottom:
            break
        }

        switch handle {
        case .topLeft, .top, .topRight:
            minY = min(clampedPoint.y, maxY - minimum)
        case .bottomLeft, .bottom, .bottomRight:
            maxY = max(clampedPoint.y, minY + minimum)
        case .left, .right:
            break
        }

        minX = max(minX, limits.minX)
        maxX = min(maxX, limits.maxX)
        minY = max(minY, limits.minY)
        maxY = min(maxY, limits.maxY)

        return CGRect(
            x: minX,
            y: minY,
            width: maxX - minX,
            height: maxY - minY
        )
    }

    public static func moved(
        _ selection: CGRect,
        by translation: CGSize,
        within bounds: CGRect
    ) -> CGRect {
        let rect = selection.standardized
        let limits = bounds.standardized
        guard rect.width <= limits.width, rect.height <= limits.height else {
            return clamped(rect, to: limits)
        }

        let x = min(
            max(rect.minX + translation.width, limits.minX),
            limits.maxX - rect.width
        )
        let y = min(
            max(rect.minY + translation.height, limits.minY),
            limits.maxY - rect.height
        )
        return CGRect(origin: CGPoint(x: x, y: y), size: rect.size)
    }
}

public enum WindowSnapGeometry {
    public static func candidate(
        at point: CGPoint,
        windowsFrontToBack: [CGRect],
        within bounds: CGRect,
        minimumSide: CGFloat = 8
    ) -> CGRect? {
        let displayBounds = bounds.standardized
        let minimum = max(1, minimumSide)

        for window in windowsFrontToBack {
            let visible = window.standardized.intersection(displayBounds)
            guard !visible.isNull,
                  visible.width >= minimum,
                  visible.height >= minimum,
                  visible.contains(point) else {
                continue
            }
            return visible
        }
        return nil
    }
}
