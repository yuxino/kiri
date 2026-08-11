import AppKit
import Combine
import SwiftUI

@MainActor
final class RecordingControlPanelController {
    private let state = RecordingControlState()
    private let panel: NSPanel

    init(onPauseResume: @escaping () -> Void, onStop: @escaping () -> Void) {
        state.onPauseResume = onPauseResume
        state.onStop = onStop

        panel = NSPanel(
            contentRect: CGRect(x: 0, y: 0, width: 296, height: 64),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.level = .statusBar
        panel.title = L10n.text("Recording Controls")
        panel.setAccessibilityLabel(L10n.text("Recording Controls"))
        panel.isFloatingPanel = true
        panel.hidesOnDeactivate = false
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.isReleasedWhenClosed = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.contentViewController = NSHostingController(
            rootView: RecordingControlBar(state: state)
        )
    }

    func show(screenFrame: CGRect) {
        let panelSize = panel.frame.size
        panel.setFrameOrigin(
            CGPoint(
                x: screenFrame.midX - panelSize.width / 2,
                y: screenFrame.maxY - panelSize.height - 18
            )
        )
        panel.orderFrontRegardless()
    }

    func update(elapsed: String, isPaused: Bool, isBusy: Bool) {
        state.elapsed = elapsed
        state.isPaused = isPaused
        state.isBusy = isBusy
    }

    func close() {
        panel.orderOut(nil)
        panel.close()
    }
}

@MainActor
private final class RecordingControlState: ObservableObject {
    @Published var elapsed = "00:00"
    @Published var isPaused = false
    @Published var isBusy = true
    var onPauseResume: (() -> Void)?
    var onStop: (() -> Void)?
}

private struct RecordingControlBar: View {
    @ObservedObject var state: RecordingControlState

    var body: some View {
        HStack(spacing: 10) {
            ZStack {
                Circle()
                    .fill(state.isPaused ? KiriUI.Palette.coral : Color.red)
                    .frame(width: 10, height: 10)
                if !state.isPaused && !state.isBusy {
                    Circle()
                        .stroke(Color.red.opacity(0.35), lineWidth: 4)
                        .frame(width: 17, height: 17)
                }
            }

            Text(state.isPaused ? L10n.text("Paused") : state.elapsed)
                .font(.system(size: 12, weight: .semibold, design: .monospaced))
                .foregroundStyle(state.isPaused ? KiriUI.Palette.coral : .primary)
                .frame(minWidth: 58, alignment: .leading)

            Divider().frame(height: 22)

            if state.isBusy {
                ProgressView()
                    .controlSize(.small)
                    .frame(width: 32, height: 30)
                    .help(L10n.text("Preparing recording"))
            } else {
                Button {
                    state.onPauseResume?()
                } label: {
                    Image(systemName: state.isPaused ? "play.fill" : "pause.fill")
                        .frame(width: 28, height: 28)
                }
                .buttonStyle(.plain)
                .background(
                    KiriUI.Palette.accent.opacity(0.14),
                    in: RoundedRectangle(cornerRadius: 9)
                )
                .foregroundStyle(KiriUI.Palette.accent)
                .help(L10n.text(state.isPaused ? "Resume Recording" : "Pause Recording"))
                .accessibilityLabel(L10n.text(state.isPaused ? "Resume Recording" : "Pause Recording"))
            }

            Button {
                state.onStop?()
            } label: {
                Image(systemName: "stop.fill")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(.white)
                    .frame(width: 28, height: 28)
            }
            .buttonStyle(.plain)
            .background(Color.red, in: RoundedRectangle(cornerRadius: 9))
            .disabled(state.isBusy)
            .help(L10n.text("Stop and Save Recording"))
            .accessibilityLabel(L10n.text("Stop Recording"))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18))
        .overlay {
            RoundedRectangle(cornerRadius: 18)
                .strokeBorder(KiriUI.Palette.border.opacity(0.9), lineWidth: 1)
        }
        .shadow(color: .black.opacity(0.18), radius: 14, y: 6)
        .padding(4)
    }
}
