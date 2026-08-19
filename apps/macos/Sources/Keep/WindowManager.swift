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

    /// The workspace the pool is currently bound to.
    private(set) var currentWorkspace: String?

    /// Kept so the poller can rebuild the arrangement without being handed one.
    weak var store: Store?

    /// Windows built once and set aside when the pool shrinks.
    ///
    /// A window is expensive to make — a toolbar, a split view and a whole
    /// SwiftUI sidebar — and workspaces differ in how many tabs they hold, so
    /// switching between a one-tab workspace and a three-tab one would build
    /// two windows from nothing every time. They are closed but not released,
    /// so bringing one back is just showing it again.
    private var reserve: [TerminalWindowController] = []

    /// Set while the app quits. Windows closed on quit are the app going
    /// away, not the user closing tabs: the daemon must keep everything.
    var isQuitting = false

    private init() {}

    /// Keep AppKit's tab bar on screen, always — even for a lone tab.
    ///
    /// Letting it come and go is what makes a switch flash. The bar is part of
    /// the window's height: when it appears, the terminal below gives up a row
    /// and every surface is remeasured and repainted, which reads as the whole
    /// content area blinking. That happens on the way into a workspace with
    /// several tabs from one with a single tab, and not between two that both
    /// have tabs — which is the shape of the report that led here.
    ///
    /// Holding it visible also settles the placement: the bar stops changing
    /// hands between windows, so the one that owns it stays the one that put
    /// it in the toolbar row.
    ///
    /// On Tahoe, `addTabbedWindow` can keep the group as a hidden window set
    /// even with `.preferred` tabbing, so this waits a run-loop turn for the
    /// group to finish forming.
    private func revealTabBar(for window: NSWindow?) {
        guard let window else { return }
        DispatchQueue.main.async {
            guard let group = window.tabGroup else { return }
            Trace.log("tabbar", "reveal: windows=\(group.windows.count) visible=\(group.isTabBarVisible)")
            guard !group.isTabBarVisible else { return }
            window.toggleTabBar(nil)
        }
    }

    /// Put a workspace on screen.
    ///
    /// The windows are a pool, not a per-workspace set. Showing a workspace
    /// resizes that pool to its root tabs and rebinds each window; it never
    /// hides one set and reveals another. That distinction is the whole point:
    /// a window that is ordered out and back in is animated by the system,
    /// hands its key status to whatever the system picks next, has its frame
    /// restored by its tab group, and is re-tiled by any window manager
    /// watching. A window that simply changes what it shows does none of that.
    ///
    /// Tabs still belong to workspaces — the pool only ever holds the current
    /// one's — and they are still native tabs, one window each, in one group.
    func show(workspace: Daemon.Workspace, store: Store) {
        let roots = workspace.rootTabs
        guard !roots.isEmpty else { return }
        Trace.log("show", "→ \(workspace.name) roots=\(roots.map(\.id)) pool=\(controllers.count)")
        currentWorkspace = workspace.name

        grow(to: roots.count, store: store)
        shrink(to: roots.count)

        for (index, root) in roots.enumerated() {
            var wanted: [(tab: UInt32, direction: UInt8?)] = [(root.id, nil)]
            wanted += workspace.panes(of: root.id).map { ($0.id, Optional($0.splitDir)) }
            controllers[index].bind(
                workspace: workspace.name, rootTab: root.id, panes: wanted)
            controllers[index].updateTitle(root.title, busy: root.busy)
        }

        controllers.first?.window?.makeKeyAndOrderFront(nil)
        revealTabBar(for: controllers.last?.window)

        // The group's membership just changed, so whichever window now holds
        // the one real tab bar has to place it. A turn later: AppKit hands the
        // bar over as part of settling the group, not before.
        DispatchQueue.main.async { [weak self] in
            guard let window = self?.controllers.first?.window as? KeepWindow else { return }
            window.refreshTabBar()
        }
    }

    /// Add windows to the group until it can hold the workspace.
    private func grow(to count: Int, store: Store) {
        while controllers.count < count {
            let controller = reserve.popLast() ?? TerminalWindowController(store: store)
            controller.isRecycling = false
            if let anchor = controllers.first?.window, let window = controller.window {
                anchor.addTabbedWindow(window, ordered: .above)
            } else if let window = controller.window {
                window.setFrame(NSRect(x: 0, y: 0, width: 1040, height: 660), display: false)
                window.center()
            }
            controllers.append(controller)
            controller.showWindow(nil)
            Trace.log("pool", "grew to \(controllers.count) reserve=\(reserve.count)")
        }
    }

    /// Drop surplus windows. Their tabs keep running — the pool is smaller,
    /// the work is not gone — so they close without touching the daemon.
    private func shrink(to count: Int) {
        while controllers.count > count {
            let controller = controllers.removeLast()
            controller.isRecycling = true
            controller.close()
            reserve.append(controller)
            Trace.log("pool", "shrank to \(controllers.count) reserve=\(reserve.count)")
        }
    }

    /// Open one more tab in the workspace on screen.
    func openTab(workspace: Daemon.Workspace, store: Store) {
        show(workspace: workspace, store: store)
    }

    /// Refresh labels, and rebuild the arrangement if the daemon's changed.
    /// Opens nothing on its own: it only follows what is already on screen.
    func sync(with workspaces: [Daemon.Workspace]) {
        guard let name = currentWorkspace,
              let workspace = workspaces.first(where: { $0.name == name })
        else { return }

        // A tab the daemon no longer has takes its surface with it, otherwise
        // the client would linger attached to nothing.
        let live = Set(workspace.liveTabs.map(\.id))
        for tab in SurfacePool.shared.tabs(in: name) where !live.contains(tab) {
            SurfacePool.shared.discard(workspace: name, tab: tab)
        }

        // Rebuild only when the arrangement actually moved: `show` is cheap
        // now, but not free, and this runs on a timer.
        let roots = workspace.rootTabs.map(\.id)
        let shown = controllers.map(\.rootTab)
        if roots != shown, let store = store {
            show(workspace: workspace, store: store)
            return
        }

        for (index, root) in workspace.rootTabs.enumerated() where index < controllers.count {
            controllers[index].updateTitle(root.title, busy: root.busy)
        }
    }

    var frontWorkspace: String? {
        currentWorkspace ?? frontController?.workspace
    }

    var frontController: TerminalWindowController? {
        (NSApp.keyWindow?.windowController as? TerminalWindowController)
            ?? (NSApp.mainWindow?.windowController as? TerminalWindowController)
    }

    func forget(_ controller: TerminalWindowController) {
        controllers.removeAll { $0 === controller }
    }
}
