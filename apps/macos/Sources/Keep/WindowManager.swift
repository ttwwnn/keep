import AppKit

/// Owns the tab windows for the workspace currently on screen.
///
/// Only user actions open or close windows. The poller merely relabels the
/// ones that exist — an earlier version reconciled windows against the
/// daemon on a timer and reopened every window the user had just closed,
/// because the daemon still had the tab.
final class WindowManager {
    static let shared = WindowManager()

    private(set) var controllers: [TerminalWindowController] = []

    /// Set while the app quits. Windows closed on quit are the app going
    /// away, not the user closing tabs: the daemon must keep everything.
    var isQuitting = false

    private init() {}

    /// Show a workspace: one native tab per live daemon tab, existing windows
    /// left untouched.
    func show(workspace: Daemon.Workspace, store: Store) {
        let mine = controllers.filter { $0.workspace == workspace.name }

        // Close windows of other workspaces: the window is a view of one
        // workspace at a time, which is what "switching" means here.
        for controller in controllers where controller.workspace != workspace.name {
            controller.isClosingBecauseTabEnded = true
            controller.close()
        }

        var previous: NSWindow? = nil
        for tab in workspace.liveTabs {
            if let existing = mine.first(where: { $0.tab == tab.id }) {
                existing.updateTitle(tab.title, busy: tab.busy)
                previous = existing.window
                continue
            }
            let controller = TerminalWindowController(
                workspace: workspace.name, tab: tab.id, store: store)
            controller.updateTitle(tab.title, busy: tab.busy)
            controllers.append(controller)

            if let previous, let window = controller.window {
                previous.addTabbedWindow(window, ordered: .above)
            } else if let window = controller.window {
                window.setFrame(
                    NSRect(x: 0, y: 0, width: 1040, height: 660), display: false)
                window.center()
            }
            controller.showWindow(nil)
            previous = controller.window
        }

        controllers.first { $0.workspace == workspace.name }?
            .window?.makeKeyAndOrderFront(nil)
    }

    /// Open one new tab window without touching the others.
    func openTab(workspace: String, tab: UInt32, store: Store) {
        guard !controllers.contains(where: { $0.workspace == workspace && $0.tab == tab }) else {
            return
        }
        let controller = TerminalWindowController(workspace: workspace, tab: tab, store: store)
        controllers.append(controller)

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

    /// Refresh labels and drop windows whose tab the daemon no longer has.
    /// Never opens anything.
    func sync(with workspaces: [Daemon.Workspace]) {
        for controller in controllers {
            let tab = workspaces
                .first { $0.name == controller.workspace }?
                .liveTabs.first { $0.id == controller.tab }
            if let tab {
                controller.updateTitle(tab.title, busy: tab.busy)
            } else {
                controller.isClosingBecauseTabEnded = true
                controller.close()
            }
        }
    }

    var frontWorkspace: String? {
        (NSApp.keyWindow?.windowController as? TerminalWindowController)?.workspace
            ?? controllers.first?.workspace
    }

    func forget(_ controller: TerminalWindowController) {
        controllers.removeAll { $0 === controller }
    }
}
