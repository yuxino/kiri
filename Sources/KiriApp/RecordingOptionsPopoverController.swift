import AppKit
import KiriCore
import SwiftUI

@MainActor
final class RecordingOptionsPopoverController {
    private let popover = NSPopover()

    init(
        options: RecordingOptions,
        onChange: @escaping (RecordingOptions) -> Void,
        onStart: @escaping (RecordingOptions) -> Void
    ) {
        let supportsModernCapture: Bool
        if #available(macOS 15.0, *) {
            supportsModernCapture = true
        } else {
            supportsModernCapture = false
        }
        var compatibleOptions = options
        if !supportsModernCapture {
            compatibleOptions.capturesMicrophone = false
            compatibleOptions.highlightsClicks = false
        }
        let view = RecordingOptionsView(
            initialOptions: compatibleOptions,
            supportsModernCapture: supportsModernCapture,
            onChange: onChange,
            onStart: { [weak popover] selectedOptions in
                popover?.performClose(nil)
                onStart(selectedOptions)
            }
        )
        popover.behavior = .transient
        popover.animates = true
        popover.contentSize = NSSize(width: 336, height: 414)
        popover.contentViewController = NSHostingController(rootView: view)
    }

    func show(relativeTo view: NSView) {
        popover.show(
            relativeTo: view.bounds,
            of: view,
            preferredEdge: .maxY
        )
    }

    func close() {
        popover.performClose(nil)
    }
}

private struct RecordingOptionsView: View {
    @State private var options: RecordingOptions
    let supportsModernCapture: Bool
    let onChange: (RecordingOptions) -> Void
    let onStart: (RecordingOptions) -> Void

    init(
        initialOptions: RecordingOptions,
        supportsModernCapture: Bool,
        onChange: @escaping (RecordingOptions) -> Void,
        onStart: @escaping (RecordingOptions) -> Void
    ) {
        _options = State(initialValue: initialOptions.normalized)
        self.supportsModernCapture = supportsModernCapture
        self.onChange = onChange
        self.onStart = onStart
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 17) {
            HStack(spacing: 12) {
                KiriSymbolMark(symbol: "record.circle.fill", size: 44)
                VStack(alignment: .leading, spacing: 2) {
                    Text(L10n.text("Record Region"))
                        .font(.system(size: 16, weight: .bold, design: .rounded))
                    Text(L10n.text("MP4 · 30 fps · Saved locally"))
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }

            VStack(spacing: 0) {
                optionRow(
                    title: L10n.text("3-second countdown"),
                    symbol: "timer",
                    value: $options.usesCountdown
                )
                Divider().padding(.leading, 31)
                optionRow(
                    title: L10n.text("System audio"),
                    symbol: "speaker.wave.2.fill",
                    value: $options.capturesSystemAudio
                )
                Divider().padding(.leading, 31)
                optionRow(
                    title: L10n.text("Microphone"),
                    symbol: "mic.fill",
                    detail: supportsModernCapture ? nil : L10n.text("Requires macOS 15"),
                    value: $options.capturesMicrophone,
                    enabled: supportsModernCapture
                )
                Divider().padding(.leading, 31)
                optionRow(
                    title: L10n.text("Show pointer"),
                    symbol: "cursorarrow",
                    value: cursorBinding
                )
                Divider().padding(.leading, 31)
                optionRow(
                    title: L10n.text("Highlight clicks"),
                    symbol: "cursorarrow.click.2",
                    value: $options.highlightsClicks,
                    enabled: options.showsCursor
                )
            }
            .padding(.horizontal, 11)
            .kiriSurface(radius: 16)

            Button {
                onStart(options.normalized)
            } label: {
                Label(L10n.text("Start Recording"), systemImage: "record.circle")
                    .font(.system(size: 12, weight: .semibold))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 3)
            }
            .buttonStyle(KiriPrimaryButtonStyle())
            .keyboardShortcut(.return, modifiers: [])

            Label(L10n.text("Saved locally · Never uploaded"), systemImage: "lock.fill")
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .center)
        }
        .padding(20)
        .frame(width: 336)
        .background {
            ZStack {
                KiriUI.Palette.canvas
                LinearGradient(
                    colors: [KiriUI.Palette.coral.opacity(0.055), .clear, KiriUI.Palette.cyan.opacity(0.035)],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
            }
        }
        .onChange(of: options) { _, newValue in
            let normalized = newValue.normalized
            if normalized != newValue {
                options = normalized
            }
            onChange(normalized)
        }
    }

    private var cursorBinding: Binding<Bool> {
        Binding(
            get: { options.showsCursor },
            set: { newValue in
                options.showsCursor = newValue
                if !newValue {
                    options.highlightsClicks = false
                }
            }
        )
    }

    private func optionRow(
        title: String,
        symbol: String,
        detail: String? = nil,
        value: Binding<Bool>,
        enabled: Bool = true
    ) -> some View {
        HStack(spacing: 9) {
            Image(systemName: symbol)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(enabled ? Color.primary : Color.secondary)
                .frame(width: 22)
            VStack(alignment: .leading, spacing: 1) {
                Text(title)
                    .font(.system(size: 12, weight: .medium))
                if let detail {
                    Text(detail)
                        .font(.system(size: 9))
                        .foregroundStyle(.secondary)
                }
            }
            Spacer()
            Toggle("", isOn: value)
                .labelsHidden()
                .toggleStyle(.switch)
                .controlSize(.mini)
                .disabled(!enabled)
        }
        .frame(minHeight: detail == nil ? 35 : 40)
        .contentShape(Rectangle())
        .opacity(enabled ? 1 : 0.58)
        .accessibilityElement(children: .combine)
    }
}
