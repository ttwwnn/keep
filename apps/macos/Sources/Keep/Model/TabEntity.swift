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
    /// The title of every pane here that is busy, root included. Plural
    /// because a tab can have three things running in it, and knowing about
    /// one of them is knowing the wrong amount.
    private(set) var busyTitles: [String] = []
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
        // Whose titles to show while something runs: the ones it is running
        // in. Reading the root's would name a shell sitting at a prompt
        // whenever the work is happening in a pane beside it.
        var newBusyTitles: [String] = root.busy ? [root.label] : []
        newBusyTitles += daemonPanes.filter(\.busy).map(\.label)
        let newPanes = daemonPanes.map {
            PaneState(tab: $0.id, splitOf: $0.splitOf, splitDir: $0.splitDir)
        }
        let changed = newTitle != title || newBusy != busy || newPanes != panes
            || newBusyTitles != busyTitles
        title = newTitle
        busy = newBusy
        busyTitles = newBusyTitles
        panes = newPanes
        // A focused pane that died falls back to the root.
        if focusedPane != id.root && !newPanes.contains(where: { $0.tab == focusedPane }) {
            focusedPane = id.root
        }
        return changed
    }

    /// Whether a pane id belongs to this tab at all.
    func owns(pane: UInt32) -> Bool {
        pane == id.root || panes.contains { $0.tab == pane }
    }

    /// Remember which pane has the keyboard — but only a pane of this tab.
    /// Focus arrives from views, and a view can be handed the responder by
    /// AppKit while it belongs to a tab nobody is looking at; remembering
    /// that would aim the next split or close at another tab's work.
    func noteFocus(pane: UInt32) {
        guard owns(pane: pane) else { return }
        focusedPane = pane
    }
}
