import AppKit
import KiriCore

private final class RecordingCountdownWindow: NSWindow {
    var onEscape: (() -> Void)?

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown, event.keyCode == 53 {
            onEscape?()
            return
        }
        super.sendEvent(event)
    }

    override func cancelOperation(_ sender: Any?) {
        onEscape?()
    }
}

@MainActor
final class RecordingCountdownController {
    private var window: RecordingCountdownWindow?
    private var countdownTask: Task<Void, Never>?
    private var continuation: CheckedContinuation<Bool, Never>?

    func run(screenFrame: CGRect, region: CGRect) async -> Bool {
        guard continuation == nil else { return false }
        let selectedFrame = CGRect(
            x: screenFrame.minX + region.minX,
            y: screenFrame.maxY - region.maxY,
            width: region.width,
            height: region.height
        ).standardized
        showWindow(frame: selectedFrame)

        return await withCheckedContinuation { continuation in
            self.continuation = continuation
            countdownTask = Task { [weak self] in
                guard let self else { return }
                for value in stride(
                    from: RecordingPolicy.countdownSeconds,
                    through: 1,
                    by: -1
                ) {
                    updateLabel(value)
                    try? await Task.sleep(for: .seconds(1))
                    guard !Task.isCancelled else { return }
                }
                finish(startRecording: true)
            }
        }
    }

    func cancel() {
        finish(startRecording: false)
    }

    private func showWindow(frame: CGRect) {
        let window = RecordingCountdownWindow(
            contentRect: frame,
            styleMask: .borderless,
            backing: .buffered,
            defer: false
        )
        window.level = .screenSaver
        window.backgroundColor = .clear
        window.isOpaque = false
        window.hasShadow = false
        window.isReleasedWhenClosed = false
        window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]

        let root = NSView(frame: CGRect(origin: .zero, size: frame.size))
        root.wantsLayer = true
        root.layer?.backgroundColor = NSColor.clear.cgColor

        let badge = NSView()
        badge.wantsLayer = true
        let badgeSize = min(96, max(68, min(frame.width, frame.height) - 16))
        badge.layer?.cornerRadius = badgeSize / 2
        badge.layer?.cornerCurve = .continuous
        badge.layer?.backgroundColor = NSColor(
            calibratedRed: 0.10,
            green: 0.08,
            blue: 0.16,
            alpha: 0.92
        ).cgColor
        badge.layer?.borderWidth = 1.5
        badge.layer?.borderColor = CaptureUIColors.accentSoft.withAlphaComponent(0.92).cgColor
        badge.layer?.shadowColor = NSColor.black.cgColor
        badge.layer?.shadowOpacity = 0.32
        badge.layer?.shadowRadius = 20
        badge.layer?.shadowOffset = CGSize(width: 0, height: -5)
        badge.translatesAutoresizingMaskIntoConstraints = false
        badge.identifier = NSUserInterfaceItemIdentifier("recording-countdown-badge")

        let label = NSTextField(labelWithString: "3")
        label.font = .monospacedDigitSystemFont(
            ofSize: min(46, badgeSize * 0.48),
            weight: .semibold
        )
        label.textColor = .white
        label.alignment = .center
        label.translatesAutoresizingMaskIntoConstraints = false
        label.identifier = NSUserInterfaceItemIdentifier("recording-countdown-label")

        let hint = NSTextField(labelWithString: L10n.text("Esc to cancel"))
        hint.font = .systemFont(ofSize: 9, weight: .medium)
        hint.textColor = NSColor.white.withAlphaComponent(0.68)
        hint.alignment = .center
        hint.translatesAutoresizingMaskIntoConstraints = false
        hint.isHidden = badgeSize < 80

        root.addSubview(badge)
        badge.addSubview(label)
        badge.addSubview(hint)
        NSLayoutConstraint.activate([
            badge.centerXAnchor.constraint(equalTo: root.centerXAnchor),
            badge.centerYAnchor.constraint(equalTo: root.centerYAnchor),
            badge.widthAnchor.constraint(equalToConstant: badgeSize),
            badge.heightAnchor.constraint(equalToConstant: badgeSize),
            label.centerXAnchor.constraint(equalTo: badge.centerXAnchor),
            label.centerYAnchor.constraint(equalTo: badge.centerYAnchor, constant: 6),
            hint.centerXAnchor.constraint(equalTo: badge.centerXAnchor),
            hint.bottomAnchor.constraint(equalTo: badge.bottomAnchor, constant: -12)
        ])

        window.contentView = root
        window.onEscape = { [weak self] in self?.cancel() }
        self.window = window
        NSApplication.shared.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
    }

    private func updateLabel(_ value: Int) {
        guard let label = window?.contentView?.subviews
            .first(where: { $0.identifier?.rawValue == "recording-countdown-badge" })?
            .subviews
            .compactMap({ $0 as? NSTextField })
            .first(where: { $0.identifier?.rawValue == "recording-countdown-label" }) else {
            return
        }
        label.stringValue = String(value)
        label.alphaValue = 0
        label.layer?.transform = CATransform3DMakeScale(0.76, 0.76, 1)
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.22
            context.timingFunction = CAMediaTimingFunction(name: .easeOut)
            label.animator().alphaValue = 1
            label.layer?.transform = CATransform3DIdentity
        }
    }

    private func finish(startRecording: Bool) {
        countdownTask?.cancel()
        countdownTask = nil
        window?.onEscape = nil
        window?.orderOut(nil)
        window?.close()
        window = nil
        let continuation = self.continuation
        self.continuation = nil
        continuation?.resume(returning: startRecording)
    }
}
