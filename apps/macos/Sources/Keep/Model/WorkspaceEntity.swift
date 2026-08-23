import Foundation

/// Layer 5: one workspace's arrangement.
///
/// Owns only which tab is active, the tab order, and the workspace's
/// daemon-derived status. Sidebar state is the active tab's — this type asks
/// the entity and never stores a copy.
@MainActor
final class WorkspaceEntity {
    let name: String
    private(set) var tabs: [TabEntity] = []
    /// How the tabs should be ordered, asked of whoever remembers it. Nil
    /// while nobody does, which is the daemon's order.
    var arrangement: (([UInt32]) -> [UInt32])?
    /// The tab a window lands on when it enters this workspace.
    ///
    /// Not "the tab that is showing" — that is a fact about a window, and
    /// lives there. This is the workspace's own memory of where you were in
    /// it, which is what a click on it in any window should resolve to.
    private(set) var lastTabID: TabID?

    /// Where this workspace last was, for the sidebar row.
    ///
    /// Remembered rather than read each time. A tab titles itself after
    /// whatever it is running, so the moment work starts the place stops
    /// being on offer — and that is exactly when it is worth knowing, since a
    /// row that has said the same path for an hour is a row you can leave
    /// alone. It is only ever replaced by another place, never by a command.
    private(set) var place: String = ""

    // Cached from the daemon listing for the sidebar row.
    private(set) var subtitle: String = "empty"
    private(set) var dot: SessionSnapshot.SidebarRow.Dot = .empty

    init(name: String) {
        self.name = name
    }

    var lastTab: TabEntity? {
        tabs.first { $0.id == lastTabID }
    }

    /// Put the tabs in this order. Ids not named keep their places at the end.
    func reorder(_ ids: [UInt32]) {
        tabs.sort {
            (ids.firstIndex(of: $0.id.root) ?? Int.max)
                < (ids.firstIndex(of: $1.id.root) ?? Int.max)
        }
    }

    /// The path out of a tab's title, when the title has one.
    ///
    /// Shells differ: some title the tab `user@host:/some/path`, others just
    /// the path. The user is always the same user and the host is this
    /// machine, so where a prefix like that is present it is dropped.
    ///
    /// What is left has to look like a path to count. A title that does not
    /// is a program that renamed the tab — it is saying something, and the
    /// row shows that too, but it is not saying where.
    static func place(in title: String) -> String? {
        var text = title
        if let colon = text.firstIndex(of: ":"),
           text[text.startIndex..<colon].contains("@") {
            text = String(text[text.index(after: colon)...])
        }
        guard text.hasPrefix("/") || text.hasPrefix("~") else { return nil }
        return text
    }

    func remember(_ id: TabID) {
        guard tabs.contains(where: { $0.id == id }) else { return }
        lastTabID = id
    }

    /// Merge one daemon listing. Creates entities for new root tabs (not
    /// hydrated — surfaces and sidebar state wait for first activation),
    /// drops dead ones, keeps the active id valid by falling to a neighbor.
    /// Returns whether anything observable changed, plus the dead ids so the
    /// caller can discard surfaces and prune persistence.
    func reconcile(with daemon: Daemon.Workspace) -> (changed: Bool, dead: [TabID]) {
        var changed = false

        let roots = daemon.rootTabs
        let rootIDs = roots.map { TabID(workspace: name, root: $0.id) }

        let dead = tabs.map(\.id).filter { !rootIDs.contains($0) }
        if !dead.isEmpty {
            tabs.removeAll { dead.contains($0.id) }
            changed = true
        }

        for root in roots {
            let id = TabID(workspace: name, root: root.id)
            let entity: TabEntity
            if let existing = tabs.first(where: { $0.id == id }) {
                entity = existing
            } else {
                entity = TabEntity(id: id)
                changed = true
            }
            if entity.apply(root: root, panes: daemon.panes(of: root.id)) {
                changed = true
            }
            if !tabs.contains(where: { $0.id == id }) {
                tabs.append(entity)
            }
        }
        // Daemon order unless somebody arranged one.
        let order = arrangement?(rootIDs.map(\.root)).map { TabID(workspace: name, root: $0) }
            ?? rootIDs
        tabs.sort {
            (order.firstIndex(of: $0.id) ?? 0) < (order.firstIndex(of: $1.id) ?? 0)
        }

        // The active tab died: its index neighbor takes over.
        if let remembered = lastTabID, dead.contains(remembered) {
            lastTabID = tabs.first?.id
            changed = true
        }
        if lastTabID == nil, let first = tabs.first {
            lastTabID = first.id
            changed = true
        }

        if let found = Self.place(in: lastTab?.title ?? ""), found != place {
            place = found
            changed = true
        }

        let newSubtitle = {
            let n = daemon.liveTabs.count
            return "\(n == 1 ? "1 tab" : "\(n) tabs") · \(daemon.stateLabel)"
        }()
        let newDot: SessionSnapshot.SidebarRow.Dot = {
            if daemon.liveTabs.isEmpty { return .empty }
            if daemon.busy { return .busy }
            return daemon.clients > 0 ? .attached : .idle
        }()
        if newSubtitle != subtitle || newDot != dot {
            subtitle = newSubtitle
            dot = newDot
            changed = true
        }

        return (changed, dead)
    }
}
