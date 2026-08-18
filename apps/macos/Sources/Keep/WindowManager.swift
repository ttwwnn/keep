import AppKit

/// Owns the tab windows for the workspace currently on screen.
///
/// Only user actions open or close windows. The poller merely relabels the
/// ones that exist — an earlier version reconciled windows against the
/// daemon on a timer and reopened every window the user had just closed,
/// because the daemon still had the tab.
final class WindowManager {
    static let shared = WindowManager()

    /// How many windows may exist at once.
    ///
    /// A chrome experiment once drove window creation in a loop and opened
    /// dozens of tab windows in seconds, which took the machine down. Every
    /// window is created through `makeController`, so this is the only lever
    /// that has to move to widen it again.
    static let maxWindows = 1

    private(set) var controllers: [TerminalWindowController] = []

    /// Set while the app quits. Windows closed on quit are the app going
    /// away, not the user closing tabs: the daemon must keep everything.
    var isQuitting = false

    private init() {}

    /// The sole way a window comes into being. Returns nil at the cap, and
    /// callers show whatever they already have instead.
    private func makeController(
        workspace: String, rootTab: UInt32, store: Store
    ) -> TerminalWindowController? {
        guard controllers.count < Self.maxWindows else { return nil }
        let controller = TerminalWindowController(
            workspace: workspace, rootTab: rootTab, store: store)
        controllers.append(controller)
        return controller
    }

    /// Show a workspace: one native tab per root daemon tab (panes render
    /// inside their root's window), existing windows left untouched.
    func show(workspace: Daemon.Workspace, store: Store) {
        let mine = controllers.filter { $0.workspace == workspace.name }

        // Close windows of other workspaces: the window is a view of one
        // workspace at a time, which is what "switching" means here.
        for controller in controllers where controller.workspace != workspace.name {
            controller.isClosingBecauseTabEnded = true
            controller.close()
        }

        var previous: NSWindow? = nil
        for root in workspace.rootTabs {
            let controller: TerminalWindowController
            if let existing = mine.first(where: { $0.rootTab == root.id }) {
                controller = existing
            } else {
                // At the cap the remaining tabs keep running in the daemon,
                // just unshown; later roots may still have a window already.
                guard let made = makeController(
                    workspace: workspace.name, rootTab: root.id, store: store)
                else { continue }
                controller = made
                if let previous, let window = controller.window {
                    previous.addTabbedWindow(window, ordered: .above)
                } else if let window = controller.window {
                    window.setFrame(
                        NSRect(x: 0, y: 0, width: 1040, height: 660), display: false)
                    window.center()
                }
                controller.showWindow(nil)
            }
            controller.updateTitle(root.title, busy: root.busy)
            // Rebuild this window's panes from the daemon's layout.
            for pane in workspace.panes(of: root.id) {
                controller.addPane(tab: pane.id, direction: pane.splitDir)
            }
            previous = controller.window
        }

        controllers.first { $0.workspace == workspace.name }?
            .window?.makeKeyAndOrderFront(nil)
    }

    /// Open one new tab window without touching the others.
    func openTab(workspace: String, tab: UInt32, store: Store) {
        guard !controllers.contains(where: { $0.workspace == workspace && $0.rootTab == tab })
        else { return }
        guard let controller = makeController(workspace: workspace, rootTab: tab, store: store)
        else { return }

        if let sibling = controllers.first(where: { $0.workspace == workspace && $0 !== controller })?.window,
           let window = controller.window {
            sibling.addTabbedWindow(window, ordered: .above)
        } else if let window = controller.window {
            window.setFrame(NSRect(x: 0, y: 0, width: 1040, height: 660), display: false)
            window.center()
        }
        controller.showWindow(nil)
        controller.window?.makeKeyAndOrderFront(nil)
    }

    /// Refresh labels, drop panes and windows the daemon no longer has.
    /// Never opens anything.
    func sync(with workspaces: [Daemon.Workspace]) {
        for controller in controllers {
            guard
                let workspace = workspaces.first(where: { $0.name == controller.workspace }),
                let root = workspace.liveTabs.first(where: { $0.id == controller.rootTab })
            else {
                controller.isClosingBecauseTabEnded = true
                controller.close()
                continue
            }
            controller.updateTitle(root.title, busy: root.busy)
            let live = Set(workspace.liveTabs.map(\.id))
            for tab in controller.tabs where tab != controller.rootTab && !live.contains(tab) {
                controller.removePane(tab: tab)
            }
        }
    }

    var frontWorkspace: String? {
        frontController?.workspace ?? controllers.first?.workspace
    }

    var frontController: TerminalWindowController? {
        (NSApp.keyWindow?.windowController as? TerminalWindowController)
            ?? (NSApp.mainWindow?.windowController as? TerminalWindowController)
    }

    func forget(_ controller: TerminalWindowController) {
        controllers.removeAll { $0 === controller }
    }
}
