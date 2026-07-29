import AppKit

enum CaptureUIColors {
    static let pen = NSColor.systemPink
    static let rectangle = NSColor.systemTeal
    static let arrow = NSColor.systemPurple
    static let text = NSColor.systemBlue
    static let mosaic = NSColor.systemOrange

    static let undo = NSColor.white.withAlphaComponent(0.72)
    static let copy = NSColor.systemBlue
    static let save = NSColor.systemGreen
    static let pin = NSColor.systemOrange
    static let edit = NSColor.systemPurple
    static let dismiss = NSColor.white.withAlphaComponent(0.72)
}

final class CaptureDividerView: NSView {
    init(height: CGFloat = 24) {
        super.init(frame: CGRect(x: 0, y: 0, width: 1, height: height))
        wantsLayer = true
        layer?.backgroundColor = NSColor.white.withAlphaComponent(0.18).cgColor
        setFrameSize(CGSize(width: 1, height: height))
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }
}

final class CaptureActionButton: NSButton {
    enum Style {
        case tool(NSColor)
        case secondary(NSColor)
        case primary(NSColor)
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
            NSImage.SymbolConfiguration(pointSize: 13, weight: .semibold)
        )
        imagePosition = showsTitle ? .imageLeading : .imageOnly
        imageHugsTitle = true
        font = .systemFont(ofSize: 12, weight: .semibold)
        isBordered = false
        focusRingType = .none
        toolTip = label
        setAccessibilityLabel(label)
        wantsLayer = true
        layer?.cornerRadius = 8
        layer?.borderWidth = 1
        setFrameSize(CGSize(width: showsTitle ? 76 : 34, height: 32))
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

    private func refreshAppearance() {
        let tint: NSColor
        let background: NSColor
        let border: NSColor
        let titleColor: NSColor

        switch visualStyle {
        case let .tool(color):
            tint = color
            background = color.withAlphaComponent(
                selectedTool ? 0.25 : (hovering ? 0.14 : 0.07)
            )
            border = color.withAlphaComponent(selectedTool ? 0.72 : 0.18)
            titleColor = color
        case let .secondary(color):
            tint = color
            background = color.withAlphaComponent(hovering ? 0.24 : 0.12)
            border = color.withAlphaComponent(hovering ? 0.48 : 0.22)
            titleColor = color
        case let .primary(color):
            tint = .white
            background = hovering ? color.highlight(withLevel: 0.12) ?? color : color
            border = color.withAlphaComponent(0.9)
            titleColor = .white
        }

        contentTintColor = tint
        layer?.backgroundColor = background.cgColor
        layer?.borderColor = border.cgColor
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
