import Foundation

/// Layer 4: one entity per daemon root tab.
///
/// Owns what is *this tab's own business* — its label, whether it is busy,
/// its pane arrangement, which pane holds the keyboard, and its sidebar
/// state — and nothing about how any of it is drawn. Deactivation freezes
/// the state by construction: nothing sends a deactivated entity messages,
/// so there is no "freeze" call to forget.
@MainActor
final class TabEntity {
    let id: TabID

    private(set) var title: String = ""
    private(set) var busy: Bool = false
    private(set) var panes: [PaneState] = []
    private(set) var focusedPane: UInt32

    /// Lazy: nil until the tab's first activation hydrates it from disk.
    private var sidebar: SidebarState?

    init(id: TabID) {
        self.id = id
        self.focusedPane = id.root
    }

    /// Apply one daemon poll. Returns true when anything observable changed,
    /// so a quiet poll publishes nothing.
    @discardableResult
    func apply(root: Daemon.Tab, panes daemonPanes: [Daemon.Tab]) -> Bool {
        let newTitle = root.label
        let newBusy = root.busy || daemonPanes.contains { $0.busy }
        let newPanes = daemonPanes.map { PaneState(tab: $0.id, splitDir: $0.splitDir) }
        let changed = newTitle != title || newBusy != busy || newPanes != panes
        title = newTitle
        busy = newBusy
        panes = newPanes
        // A focused pane that died falls back to the root.
        if focusedPane != id.root && !newPanes.contains(where: { $0.tab == focusedPane }) {
            focusedPane = id.root
        }
        return changed
    }

    /// The sidebar state, hydrating from persistence exactly once. `seed` is
    /// used when nothing was persisted — a new tab inherits the state on
    /// screen, so opening a tab never jumps the sidebar.
    func sidebarState(loading store: SidebarStateStore, seed: SidebarState?) -> SidebarState {
        if let sidebar { return sidebar }
        let state = store.state(for: id) ?? seed ?? .initial
        sidebar = state
        return state
    }

    /// Peek without hydrating: the state only if this tab has been activated.
    var sidebarIfHydrated: SidebarState? { sidebar }

    /// Single writer: Session.
    func setSidebar(_ state: SidebarState) {
        sidebar = state
    }

    func noteFocus(pane: UInt32) {
        focusedPane = pane
    }
}
