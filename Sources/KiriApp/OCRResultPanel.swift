import AppKit

/// A result panel shown after OCR while the selection box stays live. It shows
/// the recognized text in an editable field so the user can adjust the region
/// (which re-runs recognition) or tweak the text before copying. Styled to
/// match the shared capture design system.
final class OCRResultPanel: NSVisualEffectView, NSTextViewDelegate {
    enum State {
        case recognizing
        case text(String)
        case empty
        case failed
    }

    static let panelWidth: CGFloat = 372
    static let panelHeight: CGFloat = 214

    var onCopy: ((String) -> Void)?
    var onCancel: (() -> Void)?

    private let titleLabel = NSTextField(labelWithString: "")
    private let statusLabel = NSTextField(labelWithString: "")
    private let statusDetailLabel = NSTextField(labelWithString: "")
    private let statusSymbol = NSImageView()
    private let spinner = NSProgressIndicator()
    private let statusStack = NSStackView()
    private let contentWell = NSView()
    private let textView = NSTextView()
    private let scrollView = NSScrollView()
    private let summaryLabel = NSTextField(labelWithString: "")
    private let copyButton: CaptureActionButton
    private let cancelButton: CaptureActionButton
    private var currentState: State = .recognizing

    init() {
        copyButton = CaptureActionButton(
            symbol: "doc.on.clipboard.fill",
            label: L10n.text("Copy"),
            style: .primary,
            showsTitle: true,
            target: nil,
            action: #selector(NSView.hash)
        )
        cancelButton = CaptureActionButton(
            symbol: "xmark",
            label: L10n.text("Cancel"),
            style: .secondary,
            showsTitle: true,
            target: nil,
            action: #selector(NSView.hash)
        )
        super.init(frame: CGRect(x: 0, y: 0, width: Self.panelWidth, height: Self.panelHeight))
        buildLayout()
        setState(.recognizing)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    private func buildLayout() {
        material = .popover
        blendingMode = .withinWindow
        state = .active
        wantsLayer = true
        layer?.cornerRadius = 13
        layer?.cornerCurve = .continuous
        layer?.borderWidth = 1
        layer?.borderColor = CaptureUIColors.surfaceBorder.cgColor
        layer?.shadowColor = NSColor.black.cgColor
        layer?.shadowOpacity = 0.2
        layer?.shadowRadius = 8
        layer?.shadowOffset = CGSize(width: 0, height: 3)

        titleLabel.stringValue = L10n.text("Recognized Text").uppercased()
        titleLabel.font = .systemFont(ofSize: 11, weight: .semibold)
        titleLabel.textColor = CaptureUIColors.secondaryLabel

        spinner.style = .spinning
        spinner.controlSize = .small
        spinner.isDisplayedWhenStopped = false
        spinner.translatesAutoresizingMaskIntoConstraints = false

        let titleIcon = NSImageView()
        titleIcon.image = NSImage(
            systemSymbolName: "text.viewfinder",
            accessibilityDescription: nil
        )?.withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 11, weight: .semibold))
        titleIcon.contentTintColor = CaptureUIColors.accent

        let header = NSStackView(views: [titleIcon, titleLabel])
        header.orientation = .horizontal
        header.alignment = .centerY
        header.spacing = 6

        // The content well keeps a fixed footprint across every state, so the
        // panel never collapses or leaves a gap when text is absent.
        contentWell.wantsLayer = true
        contentWell.layer?.cornerRadius = 9
        contentWell.layer?.cornerCurve = .continuous
        contentWell.layer?.backgroundColor = NSColor.labelColor
            .withAlphaComponent(0.05).cgColor
        contentWell.layer?.borderWidth = 1
        contentWell.layer?.borderColor = CaptureUIColors.surfaceBorder
            .withAlphaComponent(0.55).cgColor
        contentWell.translatesAutoresizingMaskIntoConstraints = false

        textView.isRichText = false
        textView.font = .systemFont(ofSize: 13)
        textView.textColor = CaptureUIColors.label
        textView.drawsBackground = false
        textView.delegate = self
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.textContainerInset = NSSize(width: 10, height: 10)
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]

        scrollView.documentView = textView
        scrollView.hasVerticalScroller = true
        scrollView.drawsBackground = false
        scrollView.translatesAutoresizingMaskIntoConstraints = false

        spinner.controlSize = .small
        statusSymbol.symbolConfiguration = NSImage.SymbolConfiguration(pointSize: 22, weight: .regular)
        statusSymbol.contentTintColor = CaptureUIColors.secondaryLabel
        statusLabel.font = .systemFont(ofSize: 12, weight: .medium)
        statusLabel.textColor = CaptureUIColors.secondaryLabel
        statusLabel.alignment = .center
        statusDetailLabel.font = .systemFont(ofSize: 11)
        statusDetailLabel.textColor = CaptureUIColors.disabledLabel
        statusDetailLabel.alignment = .center

        statusStack.orientation = .vertical
        statusStack.alignment = .centerX
        statusStack.spacing = 8
        statusStack.translatesAutoresizingMaskIntoConstraints = false
        statusStack.addArrangedSubview(spinner)
        statusStack.addArrangedSubview(statusSymbol)
        statusStack.addArrangedSubview(statusLabel)
        statusStack.addArrangedSubview(statusDetailLabel)

        contentWell.addSubview(scrollView)
        contentWell.addSubview(statusStack)
        NSLayoutConstraint.activate([
            scrollView.topAnchor.constraint(equalTo: contentWell.topAnchor, constant: 2),
            scrollView.leadingAnchor.constraint(equalTo: contentWell.leadingAnchor, constant: 2),
            scrollView.trailingAnchor.constraint(equalTo: contentWell.trailingAnchor, constant: -2),
            scrollView.bottomAnchor.constraint(equalTo: contentWell.bottomAnchor, constant: -2),
            statusStack.centerXAnchor.constraint(equalTo: contentWell.centerXAnchor),
            statusStack.centerYAnchor.constraint(equalTo: contentWell.centerYAnchor),
            statusStack.leadingAnchor.constraint(greaterThanOrEqualTo: contentWell.leadingAnchor, constant: 12),
            statusStack.trailingAnchor.constraint(lessThanOrEqualTo: contentWell.trailingAnchor, constant: -12)
        ])

        summaryLabel.font = .systemFont(ofSize: 11, weight: .medium)
        summaryLabel.textColor = CaptureUIColors.disabledLabel
        summaryLabel.isHidden = true
        summaryLabel.setContentHuggingPriority(.defaultHigh, for: .horizontal)
        summaryLabel.setContentCompressionResistancePriority(.defaultHigh, for: .horizontal)

        copyButton.target = self
        copyButton.action = #selector(handleCopy)
        cancelButton.target = self
        cancelButton.action = #selector(handleCancel)

        let buttonSpacer = NSView()
        buttonSpacer.translatesAutoresizingMaskIntoConstraints = false
        buttonSpacer.setContentHuggingPriority(.defaultLow, for: .horizontal)
        buttonSpacer.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        let buttonRow = NSStackView(views: [summaryLabel, buttonSpacer, cancelButton, copyButton])
        buttonRow.orientation = .horizontal
        buttonRow.alignment = .centerY
        buttonRow.spacing = 8
        buttonRow.distribution = .fill

        let stack = NSStackView(views: [header, contentWell, buttonRow])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 9
        stack.edgeInsets = NSEdgeInsets(top: 13, left: 14, bottom: 13, right: 14)
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)

        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: topAnchor),
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor),
            header.leadingAnchor.constraint(equalTo: stack.leadingAnchor),
            contentWell.leadingAnchor.constraint(equalTo: stack.leadingAnchor),
            contentWell.trailingAnchor.constraint(equalTo: stack.trailingAnchor),
            buttonRow.leadingAnchor.constraint(equalTo: stack.leadingAnchor),
            buttonRow.trailingAnchor.constraint(equalTo: stack.trailingAnchor)
        ])
    }

    func setState(_ state: State) {
        currentState = state
        switch state {
        case .recognizing:
            spinner.startAnimation(nil)
            spinner.isHidden = false
            statusSymbol.isHidden = true
            statusLabel.stringValue = L10n.text("Recognizing Text…")
            statusDetailLabel.stringValue = ""
            statusDetailLabel.isHidden = true
            statusStack.isHidden = false
            scrollView.isHidden = true
            summaryLabel.isHidden = true
            copyButton.setActionEnabled(false)
        case let .text(value):
            spinner.stopAnimation(nil)
            statusStack.isHidden = true
            scrollView.isHidden = false
            textView.string = value
            textView.setSelectedRange(NSRange(location: (value as NSString).length, length: 0))
            textView.scrollToBeginningOfDocument(nil)
            summaryLabel.isHidden = false
            updateSummary()
            copyButton.setActionEnabled(!value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        case .empty:
            showStatus(
                symbol: "text.badge.xmark",
                message: L10n.text("No Text Found"),
                detail: L10n.text("Try a larger region or clearer text"),
                tint: CaptureUIColors.accent
            )
        case .failed:
            showStatus(
                symbol: "exclamationmark.triangle",
                message: L10n.text("Text Recognition Failed"),
                detail: L10n.text("Adjust the region and try again"),
                tint: .systemOrange
            )
        }
    }

    private func showStatus(symbol: String, message: String, detail: String, tint: NSColor) {
        spinner.stopAnimation(nil)
        spinner.isHidden = true
        statusSymbol.isHidden = false
        statusSymbol.image = NSImage(systemSymbolName: symbol, accessibilityDescription: message)
        statusSymbol.contentTintColor = tint
        statusLabel.stringValue = message
        statusDetailLabel.stringValue = detail
        statusDetailLabel.isHidden = detail.isEmpty
        statusStack.isHidden = false
        scrollView.isHidden = true
        summaryLabel.isHidden = true
        copyButton.setActionEnabled(false)
    }

    private func updateSummary() {
        let value = textView.string
        let characterCount = value.filter { !$0.isWhitespace }.count
        let lineCount = max(1, value.split(separator: "\n", omittingEmptySubsequences: false).count)
        summaryLabel.stringValue = L10n.format("%d lines · %d chars", lineCount, characterCount)
    }

    func textDidChange(_ notification: Notification) {
        updateSummary()
        let hasText = !textView.string.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        copyButton.setActionEnabled(hasText)
    }

    var editedText: String {
        textView.string
    }

    @objc private func handleCopy() {
        let text = textView.string.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        onCopy?(textView.string)
    }

    @objc private func handleCancel() {
        onCancel?()
    }
}
