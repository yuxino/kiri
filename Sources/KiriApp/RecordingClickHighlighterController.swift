import AppKit

@MainActor
final class RecordingClickHighlighterController {
    private let panel: NSPanel
    private let rippleView = RecordingClickRippleView()
    private var globalMonitor: Any?

    init(anchorPoint: CGPoint) {
        let size = CGSize(width: 58, height: 58)
        panel = NSPanel(
            contentRect: CGRect(
                x: anchorPoint.x - size.width / 2,
                y: anchorPoint.y - size.height / 2,
                width: size.width,
                height: size.height
            ),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.level = .statusBar
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.ignoresMouseEvents = true
        panel.hidesOnDeactivate = false
        panel.isReleasedWhenClosed = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
        panel.contentView = rippleView
        rippleView.primeForCapture()
        panel.orderFrontRegardless()
    }

    var exceptedWindowIDs: Set<CGWindowID> {
        guard panel.windowNumber > 0 else { return [] }
        return [CGWindowID(panel.windowNumber)]
    }

    func setActive(_ active: Bool) {
        if active {
            guard globalMonitor == nil else { return }
            rippleView.reset()
            globalMonitor = NSEvent.addGlobalMonitorForEvents(
                matching: [.leftMouseDown, .rightMouseDown]
            ) { [weak self] _ in
                let point = NSEvent.mouseLocation
                Task { @MainActor in
                    self?.showRipple(at: point)
                }
            }
        } else if let globalMonitor {
            NSEvent.removeMonitor(globalMonitor)
            self.globalMonitor = nil
            rippleView.reset()
        }
    }

    func close() {
        setActive(false)
        panel.orderOut(nil)
        panel.close()
    }

    private func showRipple(at point: CGPoint) {
        let size = panel.frame.size
        panel.setFrameOrigin(
            CGPoint(x: point.x - size.width / 2, y: point.y - size.height / 2)
        )
        panel.orderFrontRegardless()
        rippleView.play()
    }
}

private final class RecordingClickRippleView: NSView {
    private let haloLayer = CAShapeLayer()
    private let ringLayer = CAShapeLayer()
    private let centerLayer = CAShapeLayer()

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.backgroundColor = NSColor.clear.cgColor

        let center = CGPoint(x: 29, y: 29)
        haloLayer.path = CGPath(
            ellipseIn: CGRect(x: center.x - 21, y: center.y - 21, width: 42, height: 42),
            transform: nil
        )
        haloLayer.fillColor = NSColor.clear.cgColor
        haloLayer.strokeColor = CaptureUIColors.accent.withAlphaComponent(0.30).cgColor
        haloLayer.lineWidth = 6
        haloLayer.opacity = 0

        ringLayer.path = CGPath(
            ellipseIn: CGRect(x: center.x - 15, y: center.y - 15, width: 30, height: 30),
            transform: nil
        )
        ringLayer.fillColor = CaptureUIColors.accent.withAlphaComponent(0.12).cgColor
        ringLayer.strokeColor = CaptureUIColors.accent.withAlphaComponent(0.95).cgColor
        ringLayer.lineWidth = 2.5
        ringLayer.opacity = 0

        centerLayer.path = CGPath(
            ellipseIn: CGRect(x: center.x - 3.5, y: center.y - 3.5, width: 7, height: 7),
            transform: nil
        )
        centerLayer.fillColor = NSColor.white.withAlphaComponent(0.95).cgColor
        centerLayer.strokeColor = CaptureUIColors.accent.cgColor
        centerLayer.lineWidth = 1.5
        centerLayer.opacity = 0

        layer?.addSublayer(haloLayer)
        layer?.addSublayer(ringLayer)
        layer?.addSublayer(centerLayer)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    func play() {
        reset()
        animate(
            haloLayer,
            fromScale: 0.45,
            toScale: 1.12,
            peakOpacity: 0.72,
            duration: 0.46
        )
        animate(
            ringLayer,
            fromScale: 0.58,
            toScale: 1.0,
            peakOpacity: 1,
            duration: 0.34
        )
        animate(
            centerLayer,
            fromScale: 0.72,
            toScale: 1.0,
            peakOpacity: 1,
            duration: 0.24
        )
    }

    func primeForCapture() {
        ringLayer.opacity = 0.01
    }

    func reset() {
        for layer in [haloLayer, ringLayer, centerLayer] {
            layer.removeAllAnimations()
            layer.opacity = 0
        }
    }

    private func animate(
        _ layer: CALayer,
        fromScale: CGFloat,
        toScale: CGFloat,
        peakOpacity: Float,
        duration: CFTimeInterval
    ) {
        let scale = CAKeyframeAnimation(keyPath: "transform.scale")
        scale.values = [fromScale, toScale, toScale]
        scale.keyTimes = [0, 0.68, 1]

        let opacity = CAKeyframeAnimation(keyPath: "opacity")
        opacity.values = [0, peakOpacity, peakOpacity * 0.82, 0]
        opacity.keyTimes = [0, 0.12, 0.68, 1]

        let group = CAAnimationGroup()
        group.animations = [scale, opacity]
        group.duration = duration
        group.timingFunction = CAMediaTimingFunction(name: .easeOut)
        layer.add(group, forKey: "kiri-click-ripple")
    }
}
