import AppKit

enum CaptureUIColors {
    static let accent = NSColor.controlAccentColor
    static let label = NSColor.labelColor
    static let secondaryLabel = NSColor.secondaryLabelColor
    static let disabledLabel = NSColor.tertiaryLabelColor
    static let hoverFill = NSColor.labelColor.withAlphaComponent(0.09)
    static let selectedFill = NSColor.controlAccentColor.withAlphaComponent(0.16)
    static let divider = NSColor.separatorColor.withAlphaComponent(0.7)
    static let surfaceBorder = NSColor.separatorColor.withAlphaComponent(0.55)
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

final class CaptureActionButton: NSButton {
    enum Style {
        case tool
        case secondary
        case primary
    }

    private let visualStyle: Style
    private let label: String
    private var hovering = false
    private var selectedTool = false
    private var hoverTrackingArea: NSTrackingArea?

    init(
        symbol: String,
        label: String,
        style: Style,
        showsTitle: Bool = false,
        target: AnyObject?,
        action: Selector
    ) {
        visualStyle = style
        self.label = label
        super.init(frame: .zero)

        self.target = target
        self.action = action
        title = showsTitle ? label : ""
        image = NSImage(
            systemSymbolName: symbol,
            accessibilityDescription: label
        )?.withSymbolConfiguration(
            NSImage.SymbolConfiguration(pointSize: 13, weight: .medium)
        )
        imagePosition = showsTitle ? .imageLeading : .imageOnly
        imageHugsTitle = true
        font = .systemFont(ofSize: 12, weight: .medium)
        isBordered = false
        focusRingType = .none
        toolTip = label
        setAccessibilityLabel(label)
        wantsLayer = true
        layer?.cornerRadius = 7
        setFrameSize(CGSize(width: showsTitle ? 72 : 30, height: 30))
        refreshAppearance()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
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
    }

    override func mouseExited(with event: NSEvent) {
        hovering = false
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
                background = selectedTool
                    ? CaptureUIColors.selectedFill
                    : (hovering ? CaptureUIColors.hoverFill : .clear)
                titleColor = tint
            case .secondary:
                tint = CaptureUIColors.secondaryLabel
                background = hovering ? CaptureUIColors.hoverFill : .clear
                titleColor = CaptureUIColors.label
            case .primary:
                let color = CaptureUIColors.accent
                tint = .white
                background = hovering ? color.highlight(withLevel: 0.1) ?? color : color
                titleColor = .white
            }
        }

        contentTintColor = tint
        layer?.backgroundColor = background.cgColor
        layer?.borderWidth = 0
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
