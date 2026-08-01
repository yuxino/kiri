import KiriCore
import SwiftUI

@main
struct KiriApp: App {
    @StateObject private var model = AppModel()

    var body: some Scene {
        Window("Kiri", id: "library") {
            LibraryView(model: model)
                .frame(minWidth: 780, minHeight: 520)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .task {
                    model.start()
                    await model.refresh()
                }
        }
        .defaultSize(width: 880, height: 600)

        MenuBarExtra("kiri", systemImage: "viewfinder") {
            MenuBarView(model: model)
        }
    }
}

private struct MenuBarView: View {
    @ObservedObject var model: AppModel
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Button("Capture & Copy  \(model.captureShortcutLabel)") {
            model.startCapture(intent: .copy)
        }
        Button("Capture & Annotate") {
            model.startCapture(intent: .annotate)
        }
        Button("Open Library") {
            openWindow(id: "library")
            NSApplication.shared.activate(ignoringOtherApps: true)
        }
        Menu("Capture Shortcut") {
            ForEach(CaptureShortcutPreset.allCases) { preset in
                Button {
                    model.selectShortcut(preset)
                } label: {
                    HStack {
                        Text(preset.shortcut.displayLabel)
                        if preset == model.captureShortcutPreset {
                            Image(systemName: "checkmark")
                        }
                    }
                }
            }
        }
        Divider()
        if let error = model.errorMessage {
            Text(error)
                .foregroundStyle(.secondary)
                .lineLimit(3)
            if let label = model.capturePermissionRecoveryLabel {
                Button(label) {
                    model.performCapturePermissionRecovery()
                }
            }
            Divider()
        }
        Button("Quit kiri") {
            NSApplication.shared.terminate(nil)
        }
        .task {
            model.start()
        }
    }
}
