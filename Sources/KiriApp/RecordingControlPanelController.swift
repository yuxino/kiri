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
            contentRect: CGRect(x: 0, y: 0, width: 270, height: 52),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.level = .statusBar
        panel.title = "Recording Controls"
        panel.setAccessibilityLabel("Recording Controls")
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
        HStack(spacing: 9) {
            ZStack {
                Circle()
                    .fill(state.isPaused ? Color.orange : Color.red)
                    .frame(width: 10, height: 10)
                if !state.isPaused && !state.isBusy {
                    Circle()
                        .stroke(Color.red.opacity(0.35), lineWidth: 4)
                        .frame(width: 17, height: 17)
                }
            }

            Text(state.isPaused ? "Paused" : state.elapsed)
                .font(.system(size: 12, weight: .semibold, design: .monospaced))
                .frame(minWidth: 54, alignment: .leading)

            Divider().frame(height: 22)

            if state.isBusy {
                ProgressView()
                    .controlSize(.small)
                    .frame(width: 32, height: 30)
                    .help("Preparing recording")
            } else {
                Button {
                    state.onPauseResume?()
                } label: {
                    Image(systemName: state.isPaused ? "play.fill" : "pause.fill")
                        .frame(width: 28, height: 28)
                }
                .buttonStyle(.plain)
                .background(.primary.opacity(0.075), in: RoundedRectangle(cornerRadius: 8))
                .help(state.isPaused ? "Resume Recording" : "Pause Recording")
                .accessibilityLabel(state.isPaused ? "Resume Recording" : "Pause Recording")
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
            .background(Color.red, in: RoundedRectangle(cornerRadius: 8))
            .disabled(state.isBusy)
            .help("Stop and Save Recording")
            .accessibilityLabel("Stop Recording")
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 9)
        .background(.ultraThickMaterial, in: RoundedRectangle(cornerRadius: 14))
        .overlay {
            RoundedRectangle(cornerRadius: 14)
                .strokeBorder(Color.white.opacity(0.14), lineWidth: 1)
        }
        .padding(4)
    }
}
