import AppKit

enum CaptureUIColors {
    static let accent = NSColor(
        calibratedRed: 0.49,
        green: 0.37,
        blue: 0.96,
        alpha: 1
    )
    static let accentStrong = NSColor(
        calibratedRed: 0.39,
        green: 0.28,
        blue: 0.90,
        alpha: 1
    )
    static let blossom = NSColor(
        calibratedRed: 0.98,
        green: 0.42,
        blue: 0.67,
        alpha: 1
    )
    static let label = NSColor.labelColor
    static let secondaryLabel = NSColor.secondaryLabelColor
    static let disabledLabel = NSColor.tertiaryLabelColor
    static let hoverFill = accent.withAlphaComponent(0.09)
    static let selectedFill = accent.withAlphaComponent(0.18)
    static let divider = NSColor.separatorColor.withAlphaComponent(0.7)
    static let surfaceBorder = accent.withAlphaComponent(0.24)
    static let groupFill = accent.withAlphaComponent(0.055)
}

final class CaptureDividerView: NSView {
    init(height: CGFloat = 24) {
        super.init(frame: CGRect(x: 0, y: 0, width: 1, height: height))
        wantsLayer = true
        layer?.backgroundColor = CaptureUIColors.divider.cgColor
        setFrameSize(CGSize(width: 1, height: height))
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }
}

final class CaptureToolGroupView: NSView {
    init(content: NSView) {
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = 10
        layer?.cornerCurve = .continuous
        refreshAppearance()

        content.translatesAutoresizingMaskIntoConstraints = false
        addSubview(content)
        NSLayoutConstraint.activate([
            content.topAnchor.constraint(equalTo: topAnchor, constant: 2),
            content.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 2),
            content.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -2),
            content.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -2)
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        refreshAppearance()
    }

    private func refreshAppearance() {
        layer?.backgroundColor = CaptureUIColors.groupFill.cgColor
        layer?.borderWidth = 1
        layer?.borderColor = CaptureUIColors.surfaceBorder.withAlphaComponent(0.55).cgColor
    }
}

final class CaptureSparkleView: NSImageView {
    init() {
        super.init(frame: CGRect(x: 0, y: 0, width: 24, height: 30))
        image = NSImage(
            systemSymbolName: "sparkles",
            accessibilityDescription: "Kiri tools"
        )?.withSymbolConfiguration(
            NSImage.SymbolConfiguration(pointSize: 13, weight: .semibold)
        )
        contentTintColor = CaptureUIColors.blossom
        imageScaling = .scaleProportionallyDown
        toolTip = "Kiri capture tools"
        setAccessibilityElement(false)
        setFrameSize(CGSize(width: 24, height: 30))
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }
}

final class CaptureActionButton: NSButton {
    enum Style {
        case tool
        case secondary
        case primary
    }

    private let visualStyle: Style
    private let label: String
    private let hoverHint: String
    private let preferredSize: CGSize
    private var hovering = false
    private var pressed = false
    private var selectedTool = false
    private var hoverTrackingArea: NSTrackingArea?
    var onHoverHintChange: ((String?) -> Void)?

    init(
        symbol: String,
        label: String,
        style: Style,
        showsTitle: Bool = false,
        hoverHint: String? = nil,
        target: AnyObject?,
        action: Selector
    ) {
        visualStyle = style
        self.label = label
        self.hoverHint = hoverHint ?? label
        preferredSize = CGSize(width: showsTitle ? 78 : 32, height: 32)
        super.init(frame: .zero)

        self.target = target
        self.action = action
        title = showsTitle ? label : ""
        image = NSImage(
            systemSymbolName: symbol,
            accessibilityDescription: label
        )?.withSymbolConfiguration(
            NSImage.SymbolConfiguration(pointSize: 13, weight: .semibold)
        )
        imagePosition = showsTitle ? .imageLeading : .imageOnly
        imageHugsTitle = true
        font = .systemFont(ofSize: 12, weight: .medium)
        isBordered = false
        focusRingType = .exterior
        toolTip = label
        setAccessibilityLabel(label)
        wantsLayer = true
        layer?.cornerRadius = 9
        layer?.cornerCurve = .continuous
        setFrameSize(preferredSize)
        refreshAppearance()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override var intrinsicContentSize: NSSize {
        preferredSize
    }

    func setToolSelected(_ selected: Bool) {
        selectedTool = selected
        refreshAppearance()
    }

    func setActionEnabled(_ enabled: Bool) {
        isEnabled = enabled
        refreshAppearance()
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let hoverTrackingArea {
            removeTrackingArea(hoverTrackingArea)
        }
        let area = NSTrackingArea(
            rect: bounds,
            options: [.activeAlways, .mouseEnteredAndExited, .inVisibleRect],
            owner: self
        )
        addTrackingArea(area)
        hoverTrackingArea = area
    }

    override func mouseEntered(with event: NSEvent) {
        hovering = true
        refreshAppearance()
        onHoverHintChange?(hoverHint)
    }

    override func mouseExited(with event: NSEvent) {
        hovering = false
        refreshAppearance()
        onHoverHintChange?(nil)
    }

    override func highlight(_ flag: Bool) {
        super.highlight(flag)
        pressed = flag
        refreshAppearance()
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        refreshAppearance()
    }

    private func refreshAppearance() {
        let tint: NSColor
        let background: NSColor
        let titleColor: NSColor

        if !isEnabled {
            tint = CaptureUIColors.disabledLabel
            background = .clear
            titleColor = CaptureUIColors.disabledLabel
        } else {
            switch visualStyle {
            case .tool:
                tint = selectedTool ? CaptureUIColors.accent : CaptureUIColors.label
                if pressed {
                    background = CaptureUIColors.label.withAlphaComponent(0.16)
                } else if selectedTool {
                    background = CaptureUIColors.selectedFill
                } else {
                    background = hovering ? CaptureUIColors.hoverFill : .clear
                }
                titleColor = tint
            case .secondary:
                tint = CaptureUIColors.secondaryLabel
                background = pressed
                    ? CaptureUIColors.label.withAlphaComponent(0.16)
                    : (hovering ? CaptureUIColors.hoverFill : .clear)
                titleColor = CaptureUIColors.label
            case .primary:
                let color = CaptureUIColors.accentStrong
                tint = .white
                if pressed {
                    background = color.shadow(withLevel: 0.14) ?? color
                } else {
                    background = hovering ? color.highlight(withLevel: 0.1) ?? color : color
                }
                titleColor = .white
            }
        }

        contentTintColor = tint
        layer?.backgroundColor = background.cgColor
        layer?.borderWidth = selectedTool || visualStyle == .primary ? 1 : 0
        layer?.borderColor = visualStyle == .primary
            ? NSColor.white.withAlphaComponent(0.22).cgColor
            : CaptureUIColors.accent.withAlphaComponent(0.32).cgColor
        layer?.shadowColor = CaptureUIColors.accentStrong.cgColor
        layer?.shadowOpacity = visualStyle == .primary && isEnabled ? 0.22 : 0
        layer?.shadowRadius = 5
        layer?.shadowOffset = CGSize(width: 0, height: 2)
        layer?.transform = pressed
            ? CATransform3DMakeScale(0.94, 0.94, 1)
            : CATransform3DIdentity
        if !title.isEmpty {
            attributedTitle = NSAttributedString(
                string: label,
                attributes: [
                    .font: NSFont.systemFont(ofSize: 12, weight: .semibold),
                    .foregroundColor: titleColor
                ]
            )
        }
    }
}
