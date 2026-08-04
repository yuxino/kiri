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

final class CaptureTrackingSlider: NSSlider {
    var onTrackingBegan: (() -> Void)?
    var onTrackingEnded: (() -> Void)?

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        onTrackingBegan?()
        super.mouseDown(with: event)
        onTrackingEnded?()
    }
}

final class AnnotationColorSwatchButton: NSButton {
    let preset: AnnotationColorPreset
    private var selectedColor = false
    private var hovering = false
    private var hoverTrackingArea: NSTrackingArea?

    init(
        preset: AnnotationColorPreset,
        target: AnyObject?,
        action: Selector
    ) {
        self.preset = preset
        super.init(frame: CGRect(x: 0, y: 0, width: 22, height: 28))
        self.target = target
        self.action = action
        title = ""
        isBordered = false
        focusRingType = .exterior
        toolTip = preset.name
        setAccessibilityLabel(L10n.format("Annotation color: %@", preset.name))
        wantsLayer = true
        layer?.cornerRadius = 8
        layer?.cornerCurve = .continuous
        setFrameSize(CGSize(width: 22, height: 28))
        refreshAppearance()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override var intrinsicContentSize: NSSize {
        CGSize(width: 22, height: 28)
    }

    func setColorSelected(_ selected: Bool) {
        selectedColor = selected
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
        let swatch = preset.color
        layer?.backgroundColor = selectedColor
            ? swatch.withAlphaComponent(0.2).cgColor
            : (hovering ? CaptureUIColors.hoverFill.cgColor : NSColor.clear.cgColor)
        layer?.borderWidth = selectedColor ? 1.5 : 0
        layer?.borderColor = swatch.cgColor

        let circleSize: CGFloat = selectedColor ? 12 : 10
        let image = NSImage(size: CGSize(width: 16, height: 16), flipped: false) { rect in
            let circle = NSBezierPath(
                ovalIn: CGRect(
                    x: rect.midX - circleSize / 2,
                    y: rect.midY - circleSize / 2,
                    width: circleSize,
                    height: circleSize
                )
            )
            swatch.setFill()
            circle.fill()
            NSColor.black.withAlphaComponent(0.18).setStroke()
            circle.lineWidth = 0.75
            circle.stroke()
            return true
        }
        self.image = image
        imagePosition = .imageOnly
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

/// A capture-mode selector styled to match the shared design system, so the
/// mode picker reads as part of the same toolbar family instead of a stock
/// `NSSegmentedControl`. Each segment is an icon + label pill; the selected
/// segment fills with the accent tint used everywhere else.
final class CaptureModeSegmentedControl: NSView {
    struct Segment {
        let symbol: String
        let title: String
        let accessibilityLabel: String
        let toolTip: String
    }

    var onSelect: ((Int) -> Void)?

    private(set) var selectedIndex: Int
    private var buttons: [CaptureModeSegmentButton] = []
    private let stack = NSStackView()

    init(segments: [Segment], selectedIndex: Int) {
        self.selectedIndex = selectedIndex
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false

        stack.orientation = .horizontal
        stack.alignment = .centerY
        stack.distribution = .fill
        stack.spacing = 2
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: topAnchor),
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor)
        ])

        for (index, segment) in segments.enumerated() {
            let button = CaptureModeSegmentButton(
                segment: segment,
                target: self,
                action: #selector(handleSelection(_:))
            )
            button.tag = index
            button.setSelected(index == selectedIndex)
            buttons.append(button)
            stack.addArrangedSubview(button)
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    func setSelectedIndex(_ index: Int) {
        guard index != selectedIndex, buttons.indices.contains(index) else { return }
        selectedIndex = index
        for (buttonIndex, button) in buttons.enumerated() {
            button.setSelected(buttonIndex == index)
        }
    }

    @objc private func handleSelection(_ sender: CaptureModeSegmentButton) {
        let index = sender.tag
        guard index != selectedIndex else { return }
        setSelectedIndex(index)
        onSelect?(index)
    }
}

private final class CaptureModeSegmentButton: NSButton {
    private let segment: CaptureModeSegmentedControl.Segment
    private var hovering = false
    private var isSelectedSegment = false
    private var hoverTrackingArea: NSTrackingArea?

    init(
        segment: CaptureModeSegmentedControl.Segment,
        target: AnyObject?,
        action: Selector
    ) {
        self.segment = segment
        super.init(frame: .zero)
        self.target = target
        self.action = action
        title = segment.title
        image = NSImage(
            systemSymbolName: segment.symbol,
            accessibilityDescription: segment.accessibilityLabel
        )?.withSymbolConfiguration(
            NSImage.SymbolConfiguration(pointSize: 12, weight: .semibold)
        )
        imagePosition = .imageLeading
        imageHugsTitle = true
        isBordered = false
        focusRingType = .exterior
        toolTip = segment.toolTip
        setAccessibilityLabel(segment.accessibilityLabel)
        wantsLayer = true
        layer?.cornerRadius = 9
        layer?.cornerCurve = .continuous
        translatesAutoresizingMaskIntoConstraints = false
        heightAnchor.constraint(equalToConstant: 32).isActive = true
        widthAnchor.constraint(greaterThanOrEqualToConstant: 92).isActive = true
        refreshAppearance()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override var intrinsicContentSize: NSSize {
        var size = super.intrinsicContentSize
        size.width += 20
        size.height = 32
        return size
    }

    func setSelected(_ selected: Bool) {
        isSelectedSegment = selected
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
        if isSelectedSegment {
            tint = CaptureUIColors.accent
            background = CaptureUIColors.selectedFill
        } else {
            tint = CaptureUIColors.secondaryLabel
            background = hovering ? CaptureUIColors.hoverFill : .clear
        }
        contentTintColor = tint
        layer?.backgroundColor = background.cgColor
        layer?.borderWidth = isSelectedSegment ? 1 : 0
        layer?.borderColor = CaptureUIColors.accent.withAlphaComponent(0.32).cgColor
        attributedTitle = NSAttributedString(
            string: segment.title,
            attributes: [
                .font: NSFont.systemFont(ofSize: 12, weight: .semibold),
                .foregroundColor: tint
            ]
        )
    }
}
