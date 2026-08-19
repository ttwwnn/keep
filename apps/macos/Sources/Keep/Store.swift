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

        WindowManager.shared.store = self
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
    ///
    /// The assignment is guarded because `@Published` notifies on every write,
    /// changed or not, and every window carries its own sidebar rendered from
    /// this. An unguarded write rebuilt all of them twice a minute for nothing
    /// — and rebuilding a view tree is more accessibility churn for whatever
    /// is watching the app from outside.
    private func poll() {
        if let latest = try? Daemon.list(), latest != workspaces {
            workspaces = latest
        }
        WindowManager.shared.sync(with: workspaces)
    }

    // MARK: - actions

    func select(workspace name: String) {
        Trace.log("select", "want=\(name) current=\(selectedWorkspace ?? "nil") switching=\(switching)")
        // One click, one switch. Rearranging the windows replaces the control
        // that was clicked, and whatever takes its place under the pointer can
        // be handed the tail of the same click — which asks for another
        // switch, and another. Until the rearrangement has settled, there is
        // no new intent to act on.
        guard !switching else { return }
        guard name != selectedWorkspace || WindowManager.shared.controllers.isEmpty else { return }
        selectedWorkspace = name
        guard let workspace = workspaces.first(where: { $0.name == name }) else { return }

        if workspace.liveTabs.isEmpty {
            // A workspace with no tabs has nothing to show; opening one is
            // what entering it means.
            newTab(in: name)
        } else {
            scheduleShow(name)
        }
    }

    /// The workspace a deferred `show` will put up, if one is pending.
    private var pendingShow: String?

    /// True from the moment a switch is asked for until the windows it moves
    /// have settled and the click that asked can no longer reach them.
    private var switching = false

    /// Rearrange the windows after the click that asked for it has finished.
    ///
    /// `show` closes and opens windows, and the control that called it lives
    /// inside one of them. Doing that within the click's own event handling
    /// tears down the view hierarchy under the pointer while the event is
    /// still being routed, and the window that takes its place inherits the
    /// click — which becomes a second switch nobody asked for, and then a
    /// third. Waiting one turn of the run loop lets the click finish first.
    ///
    /// It also coalesces: several requests in one turn settle on the last.
    private func scheduleShow(_ name: String) {
        pendingShow = name
        switching = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            defer {
                // Released a turn after the windows moved: the click that
                // started this can still be in flight, and the windows it
                // would land on now are not the ones it was aimed at.
                DispatchQueue.main.async { self.switching = false }
            }
            guard let wanted = self.pendingShow else { return }
            self.pendingShow = nil
            guard let workspace = self.workspaces.first(where: { $0.name == wanted }),
                  !workspace.liveTabs.isEmpty
            else { return }
            WindowManager.shared.show(workspace: workspace, store: self)
        }
    }

    func newTabInFront() {
        let name = WindowManager.shared.frontWorkspace ?? selectedWorkspace
        guard let name else { return }
        newTab(in: name)
    }

    /// Open a tab and put its workspace on screen.
    func newTab(in workspace: String) {
        do {
            let id = try Daemon.newTab(in: workspace)
            selectedWorkspace = workspace
            workspaces = (try? Daemon.list()) ?? workspaces

            // Same call either way now: a tab added to the workspace on screen
            // and a workspace being entered are both just "put this workspace
            // up", which is what makes them cost the same.
            if let opened = workspaces.first(where: { $0.name == workspace }) {
                WindowManager.shared.show(workspace: opened, store: self)
            }
        } catch {
            self.error = error.localizedDescription
            presentError()
        }
    }

    /// Split the focused pane of the front window. Direction 1 = right,
    /// 2 = down (the protocol's values).
    func split(direction: UInt8) {
        guard let controller = WindowManager.shared.frontController else { return }
        do {
            _ = try Daemon.newTab(
                in: controller.workspace,
                splitOf: controller.focusedTab,
                splitDir: direction
            )
            workspaces = (try? Daemon.list()) ?? workspaces
            if let current = current {
                WindowManager.shared.show(workspace: current, store: self)
            }
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
            SurfacePool.shared.discardAll(workspace: name)
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
