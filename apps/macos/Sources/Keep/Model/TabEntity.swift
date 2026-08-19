import Foundation

/// Layer 4: one entity per daemon root tab.
///
/// Owns what is *this tab's own business* — its label, whether it is busy,
/// its pane arrangement, and which pane holds the keyboard — and nothing
/// about how any of it is drawn. The sidebar is deliberately NOT here: it is
/// one state for the whole app, because furniture that rearranges itself as
/// you move between tabs reads as a glitch rather than as memory.
@MainActor
final class TabEntity {
    let id: TabID

    private(set) var title: String = ""
    private(set) var busy: Bool = false
    private(set) var panes: [PaneState] = []
    private(set) var focusedPane: UInt32

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

    func noteFocus(pane: UInt32) {
        focusedPane = pane
    }
}
