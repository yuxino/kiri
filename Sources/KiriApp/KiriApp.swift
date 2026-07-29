import KiriCore
import SwiftUI

@main
struct KiriApp: App {
    @StateObject private var model = AppModel()

    var body: some Scene {
        Window("kiri library", id: "library") {
            LibraryView(model: model)
                .frame(minWidth: 760, minHeight: 500)
                .task {
                    model.start()
                    await model.refresh()
                }
        }
        .defaultSize(width: 920, height: 640)

        MenuBarExtra("kiri", systemImage: "viewfinder") {
            MenuBarView(model: model)
        }
    }
}

private struct MenuBarView: View {
    @ObservedObject var model: AppModel
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Button("Capture Region  \(model.captureShortcutLabel)") {
            model.startCapture()
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
