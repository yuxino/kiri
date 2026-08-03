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
        root.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.18).cgColor
        root.layer?.borderWidth = 3
        root.layer?.borderColor = CaptureUIColors.blossom.cgColor

        let badge = NSVisualEffectView()
        badge.material = .hudWindow
        badge.blendingMode = .withinWindow
        badge.state = .active
        badge.wantsLayer = true
        badge.layer?.cornerRadius = 28
        badge.layer?.cornerCurve = .continuous
        badge.layer?.borderWidth = 1
        badge.layer?.borderColor = NSColor.white.withAlphaComponent(0.18).cgColor
        badge.translatesAutoresizingMaskIntoConstraints = false
        badge.identifier = NSUserInterfaceItemIdentifier("recording-countdown-badge")

        let label = NSTextField(labelWithString: "3")
        label.font = .monospacedDigitSystemFont(ofSize: 54, weight: .bold)
        label.textColor = .white
        label.alignment = .center
        label.translatesAutoresizingMaskIntoConstraints = false
        label.identifier = NSUserInterfaceItemIdentifier("recording-countdown-label")

        let hint = NSTextField(labelWithString: "Esc to cancel")
        hint.font = .systemFont(ofSize: 10, weight: .medium)
        hint.textColor = NSColor.white.withAlphaComponent(0.72)
        hint.alignment = .center
        hint.translatesAutoresizingMaskIntoConstraints = false

        root.addSubview(badge)
        badge.addSubview(label)
        badge.addSubview(hint)
        let badgeSize = min(132, max(88, min(frame.width, frame.height) - 20))
        NSLayoutConstraint.activate([
            badge.centerXAnchor.constraint(equalTo: root.centerXAnchor),
            badge.centerYAnchor.constraint(equalTo: root.centerYAnchor),
            badge.widthAnchor.constraint(equalToConstant: badgeSize),
            badge.heightAnchor.constraint(equalToConstant: min(128, badgeSize)),
            label.centerXAnchor.constraint(equalTo: badge.centerXAnchor),
            label.centerYAnchor.constraint(equalTo: badge.centerYAnchor, constant: -8),
            hint.centerXAnchor.constraint(equalTo: badge.centerXAnchor),
            hint.bottomAnchor.constraint(equalTo: badge.bottomAnchor, constant: -13)
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
        label.layer?.transform = CATransform3DMakeScale(1.22, 1.22, 1)
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.16
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
