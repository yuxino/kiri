import AppKit
import KiriCore
import SwiftUI

@main
struct KiriApp: App {
    @NSApplicationDelegateAdaptor(KiriAppDelegate.self) private var appDelegate
    @StateObject private var model = AppModel()

    var body: some Scene {
        Window("Kiri", id: "library") {
            LibraryView(model: model)
                .frame(minWidth: 820, minHeight: 540)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .task {
                    model.start()
                    await model.refresh()
                }
        }
        .defaultSize(width: 960, height: 640)
        .commands {
            KiriCommands(model: model)
        }

        MenuBarExtra("kiri", systemImage: "viewfinder") {
            MenuBarView(model: model)
        }
    }
}

@MainActor
private final class KiriAppDelegate: NSObject, NSApplicationDelegate {
    private var launchObserver: NSObjectProtocol?
    private var duplicateScanTimer: Timer?

    func applicationDidFinishLaunching(_ notification: Notification) {
        _ = NSApplication.shared.setActivationPolicy(.regular)
        closeOtherKiriInstances()
        launchObserver = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didLaunchApplicationNotification,
            object: nil,
            queue: .main
        ) { notification in
            guard let application = notification.userInfo?[
                NSWorkspace.applicationUserInfoKey
            ] as? NSRunningApplication else {
                return
            }
            Task { @MainActor in
                Self.closeIfDuplicate(application)
            }
        }
        let timer = Timer(timeInterval: 1, repeats: true) { _ in
            Task { @MainActor [weak self] in
                self?.closeOtherKiriInstances()
            }
        }
        RunLoop.main.add(timer, forMode: .common)
        duplicateScanTimer = timer
    }

    func applicationWillTerminate(_ notification: Notification) {
        if let launchObserver {
            NSWorkspace.shared.notificationCenter.removeObserver(launchObserver)
        }
        duplicateScanTimer?.invalidate()
    }

    private func closeOtherKiriInstances() {
        guard let bundleIdentifier = Bundle.main.bundleIdentifier else { return }
        for application in NSRunningApplication.runningApplications(
            withBundleIdentifier: bundleIdentifier
        ) {
            Self.closeIfDuplicate(application)
        }
    }

    private static func closeIfDuplicate(_ application: NSRunningApplication) {
        guard application.processIdentifier != ProcessInfo.processInfo.processIdentifier,
              application.bundleIdentifier == Bundle.main.bundleIdentifier else {
            return
        }
        if !application.terminate() {
            application.forceTerminate()
            return
        }
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(350))
            if !application.isTerminated {
                application.forceTerminate()
            }
        }
    }
}

private struct KiriCommands: Commands {
    @ObservedObject var model: AppModel
    @FocusedValue(\.focusLibrarySearch) private var focusLibrarySearch

    var body: some Commands {
        CommandMenu(L10n.text("Capture")) {
            if model.hasRecordingSession {
                Button(L10n.text(model.isRecordingPaused ? "Resume Recording" : "Pause Recording")) {
                    model.toggleRecordingPause()
                }
                .disabled(model.isRecordingTransitioning)

                Button(L10n.format("Stop and Save  %@", model.recordingElapsedLabel)) {
                    model.stopRecording()
                }
                .disabled(model.isRecordingTransitioning)
            } else {
                Button(L10n.text("Capture")) {
                    model.startCapture()
                }
                .disabled(model.captureIsUnavailable)
            }
        }

        CommandGroup(after: .textEditing) {
            Button(L10n.text("Find in Library")) {
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
        if model.hasRecordingSession {
            Button {
                model.toggleRecordingPause()
            } label: {
                Label(
                    L10n.text(model.isRecordingPaused ? "Resume Recording" : "Pause Recording"),
                    systemImage: model.isRecordingPaused ? "play.circle.fill" : "pause.circle.fill"
                )
            }
            .disabled(model.isRecordingTransitioning)

            Button {
                model.stopRecording()
            } label: {
                Label(
                    L10n.format("Stop and Save  %@", model.recordingElapsedLabel),
                    systemImage: "stop.circle.fill"
                )
            }
            .disabled(model.isRecordingTransitioning)
        } else {
            Button {
                model.startCapture()
            } label: {
                if model.isRecordingFinalizing {
                    Label(L10n.text("Finalizing Recording…"), systemImage: "hourglass")
                } else if model.isRecordingStarting || model.isCaptureStarting {
                    Label(L10n.text("Preparing Capture…"), systemImage: "hourglass")
                } else {
                    Text(L10n.format("Capture  %@", model.captureShortcutLabel))
                }
            }
            .disabled(model.captureIsUnavailable)
        }
        Divider()
        Button(L10n.text("Open Library")) {
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
        Button(L10n.text("Quit Kiri")) {
            NSApplication.shared.terminate(nil)
        }
        .task {
            model.start()
        }
    }
}
