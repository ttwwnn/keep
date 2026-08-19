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
    private(set) var activeTabID: TabID?

    // Cached from the daemon listing for the sidebar row.
    private(set) var subtitle: String = "empty"
    private(set) var dot: SessionSnapshot.SidebarRow.Dot = .empty

    init(name: String) {
        self.name = name
    }

    var activeTab: TabEntity? {
        tabs.first { $0.id == activeTabID }
    }

    func activate(_ id: TabID) {
        guard tabs.contains(where: { $0.id == id }) else { return }
        activeTabID = id
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
        // Daemon order is presentation order.
        let order = rootIDs
        tabs.sort {
            (order.firstIndex(of: $0.id) ?? 0) < (order.firstIndex(of: $1.id) ?? 0)
        }

        // The active tab died: its index neighbor takes over.
        if let active = activeTabID, dead.contains(active) {
            activeTabID = tabs.first?.id
            changed = true
        }
        if activeTabID == nil, let first = tabs.first {
            activeTabID = first.id
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
