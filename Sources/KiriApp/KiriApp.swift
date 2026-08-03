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
        .commands {
            KiriCommands(model: model)
        }

        MenuBarExtra("kiri", systemImage: "viewfinder") {
            MenuBarView(model: model)
        }
    }
}

private struct KiriCommands: Commands {
    @ObservedObject var model: AppModel
    @FocusedValue(\.focusLibrarySearch) private var focusLibrarySearch

    var body: some Commands {
        CommandMenu("Capture") {
            Button("Capture") {
                model.startCapture()
            }
            .disabled(model.isCaptureStarting)
        }

        CommandGroup(after: .textEditing) {
            Button("Find in Library") {
                focusLibrarySearch?()
            }
            .keyboardShortcut("f", modifiers: .command)
            .disabled(focusLibrarySearch == nil)
        }
    }
}

private struct MenuBarView: View {
    @ObservedObject var model: AppModel
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Button {
            model.startCapture()
        } label: {
            if model.isCaptureStarting {
                Label("Preparing Capture…", systemImage: "hourglass")
            } else {
                Text("Capture  \(model.captureShortcutLabel)")
            }
        }
        .disabled(model.isCaptureStarting)
        Divider()
        Button("Open Library") {
            openWindow(id: "library")
            NSApplication.shared.activate(ignoringOtherApps: true)
        }
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
