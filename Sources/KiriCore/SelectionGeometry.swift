import CoreGraphics

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
}

