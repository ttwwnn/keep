import AppKit
import SwiftUI

/// The app's view of the daemon.
@MainActor
final class Store: ObservableObject {
    @Published private(set) var workspaces: [Daemon.Workspace] = []
    @Published var selectedWorkspace: String?
    @Published var error: String?

    private var timer: Timer?

    var current: Daemon.Workspace? {
        workspaces.first { $0.name == selectedWorkspace }
    }

    func start() {
        do {
            try Daemon.ensureRunning()
            workspaces = try Daemon.list()
        } catch {
            self.error = error.localizedDescription
            presentError()
            return
        }

        // Something must be on screen; a fresh daemon gets a default
        // workspace named after the user.
        if workspaces.isEmpty {
            let name = NSUserName()
            _ = try? Daemon.newTab(in: name)
            workspaces = (try? Daemon.list()) ?? []
        }

        let first = workspaces.first?.name
        selectedWorkspace = first
        if let workspace = current {
            WindowManager.shared.show(workspace: workspace, store: self)
        }

        timer = Timer.scheduledTimer(withTimeInterval: 2, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.poll() }
        }
    }

    /// Refresh state and relabel windows. Opens nothing: windows come and go
    /// only through user actions.
    private func poll() {
        workspaces = (try? Daemon.list()) ?? workspaces
        WindowManager.shared.sync(with: workspaces)
    }

    // MARK: - actions

    func select(workspace name: String) {
        guard name != selectedWorkspace || WindowManager.shared.controllers.isEmpty else { return }
        selectedWorkspace = name
        guard let workspace = workspaces.first(where: { $0.name == name }) else { return }

        if workspace.liveTabs.isEmpty {
            // A workspace with no tabs has nothing to show; opening one is
            // what entering it means.
            newTab(in: name)
        } else {
            WindowManager.shared.show(workspace: workspace, store: self)
        }
    }

    func newTabInFront() {
        let name = WindowManager.shared.frontWorkspace ?? selectedWorkspace
        guard let name else { return }
        newTab(in: name)
    }

    func newTab(in workspace: String) {
        do {
            let id = try Daemon.newTab(in: workspace)
            selectedWorkspace = workspace
            workspaces = (try? Daemon.list()) ?? workspaces
            WindowManager.shared.openTab(workspace: workspace, tab: id, store: self)
        } catch {
            self.error = error.localizedDescription
            presentError()
        }
    }

    func createWorkspace(named name: String) {
        let trimmed = name.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return }
        newTab(in: trimmed)
    }

    /// Called when a tab window is closed by the user.
    func close(tab: UInt32, in workspace: String) {
        try? Daemon.closeTab(tab, in: workspace)
        workspaces = (try? Daemon.list()) ?? workspaces
    }

    func kill(workspace name: String) {
        do {
            try Daemon.kill(name)
        } catch {
            self.error = error.localizedDescription
            presentError()
        }
        workspaces = (try? Daemon.list()) ?? workspaces
        WindowManager.shared.sync(with: workspaces)
        if selectedWorkspace == name {
            selectedWorkspace = workspaces.first?.name
            if let workspace = current {
                WindowManager.shared.show(workspace: workspace, store: self)
            }
        }
    }

    private func presentError() {
        guard let error else { return }
        let alert = NSAlert()
        alert.messageText = "Keep"
        alert.informativeText = error
        alert.runModal()
    }
}
