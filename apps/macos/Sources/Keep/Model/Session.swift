import Foundation

/// What the UI implements: it receives immutable snapshots and shows errors.
/// It never reaches back into the model except by dispatching intents.
@MainActor
protocol SessionRendering: AnyObject {
    func render(_ snapshot: SessionSnapshot)
    func present(error: String)
}

/// Layer 5's root, and the single writer of all selection state.
///
/// Which workspace is active, which tab, which pane has focus, what a tab's
/// sidebar looks like — every one of those facts has exactly one copy, here,
/// and changes only inside `dispatch`. The UI learns them by being handed a
/// snapshot. That is the whole cure for the era when the same facts lived in
/// a Store, a WindowManager and N per-window sidebars, and disagreed.
@MainActor
final class Session {
    weak var renderer: SessionRendering?

    private var workspaces: [WorkspaceEntity] = []
    private var activeWorkspaceName: String?
    private let sidebarStore = SidebarStateStore()
    private var lastSnapshot: SessionSnapshot?

    private var activeWorkspace: WorkspaceEntity? {
        workspaces.first { $0.name == activeWorkspaceName }
    }

    // MARK: - lifecycle

    /// Write anything the debounce still owes. Called on quit: a sidebar the
    /// person collapsed a moment before quitting must survive it.
    func flush() {
        sidebarStore.flush()
    }

    func start() {
        do {
            try Daemon.ensureRunning()
        } catch {
            renderer?.present(error: error.localizedDescription)
            return
        }
        refreshFromDaemon()

        // Something must be on screen; a fresh daemon gets a default
        // workspace named after the user.
        if workspaces.allSatisfy({ $0.tabs.isEmpty }) {
            _ = try? Daemon.newTab(in: NSUserName())
            refreshFromDaemon()
        }
        if let first = workspaces.first(where: { !$0.tabs.isEmpty }) {
            activate(first.activeTabID ?? first.tabs.first?.id)
        }
        publish()
    }

    /// One daemon poll, delivered by the poller. Reconciliation only: it can
    /// prune and relabel, and repair a dead active tab — it cannot mount,
    /// present, or switch to something new.
    func reconcile(_ listing: [Daemon.Workspace]) {
        var changed = false

        // Workspaces gone from the daemon take their entities with them.
        let liveNames = Set(listing.map(\.name))
        let vanished = workspaces.map(\.name).filter { !liveNames.contains($0) }
        for name in vanished {
            SurfacePool.shared.discardAll(workspace: name)
            sidebarStore.forgetAll(workspace: name)
            workspaces.removeAll { $0.name == name }
            changed = true
        }

        for daemon in listing {
            let entity: WorkspaceEntity
            if let existing = workspaces.first(where: { $0.name == daemon.name }) {
                entity = existing
            } else {
                entity = WorkspaceEntity(name: daemon.name)
                workspaces.append(entity)
                changed = true
            }
            let result = entity.reconcile(with: daemon)
            changed = changed || result.changed
            for dead in result.dead {
                sidebarStore.forget(dead)
            }

            // Any surface whose tab the daemon no longer has, root or pane
            // alike. Keying this off dead *roots* left a pane that died on
            // its own holding a renderer and a client process forever.
            let live = Set(daemon.liveTabs.map(\.id))
            for tab in SurfacePool.shared.tabs(in: daemon.name) where !live.contains(tab) {
                SurfacePool.shared.discard(workspace: daemon.name, tab: tab)
            }
        }
        workspaces.sort { $0.name < $1.name }

        // The active workspace vanished: fall to the first remaining.
        if activeWorkspaceName != nil, activeWorkspace == nil {
            activeWorkspaceName = workspaces.first?.name
            changed = true
        }

        // Whoever ended up active — including a successor the reconcile
        // promoted after a tab died — must have its sidebar hydrated, or the
        // snapshot substitutes a default and the next write clobbers what was
        // on disk.
        hydrateActive()
        if changed { publish() }
    }

    /// Pull the active tab's persisted sidebar state if this is its first
    /// activation. Idempotent.
    private func hydrateActive() {
        _ = activeWorkspace?.activeTab?.sidebarState(loading: sidebarStore, seed: nil)
    }

    // MARK: - intents

    func dispatch(_ intent: Intent) {
        Trace.log("intent", "\(intent)")
        switch intent {
        case .activateWorkspace(let name):
            guard let workspace = workspaces.first(where: { $0.name == name }) else { return }
            if workspace.tabs.isEmpty {
                // Entering an empty workspace means opening a tab in it.
                dispatch(.newTab(in: name))
            } else {
                activate(workspace.activeTabID ?? workspace.tabs.first?.id)
                publish()
            }

        case .activateTab(let id):
            activate(id)
            publish()

        case .activateTabIndex(let index):
            guard let tabs = activeWorkspace?.tabs, !tabs.isEmpty else { return }
            let resolved = index == -1 ? tabs.count - 1 : index
            guard tabs.indices.contains(resolved) else { return }
            activate(tabs[resolved].id)
            publish()

        case .nextTab, .previousTab:
            guard let workspace = activeWorkspace, workspace.tabs.count > 1,
                  let current = workspace.tabs.firstIndex(where: { $0.id == workspace.activeTabID })
            else { return }
            let step = { if case .nextTab = intent { return 1 } else { return -1 } }()
            let next = (current + step + workspace.tabs.count) % workspace.tabs.count
            activate(workspace.tabs[next].id)
            publish()

        case .newTab(let name):
            guard let name = name ?? activeWorkspaceName else { return }
            do {
                let id = try Daemon.newTab(in: name)
                refreshFromDaemon()
                activate(TabID(workspace: name, root: id))
                publish()
            } catch {
                renderer?.present(error: error.localizedDescription)
            }

        case .newWorkspace(let raw):
            let name = raw.trimmingCharacters(in: .whitespaces)
            guard !name.isEmpty else { return }
            dispatch(.newTab(in: name))

        case .closeTab(let id):
            guard let id = id ?? activeWorkspace?.activeTabID else { return }
            do {
                // Close the panes first: they are daemon tabs of their own.
                let panes = workspaces.first { $0.name == id.workspace }?
                    .tabs.first { $0.id == id }?.panes ?? []
                for pane in panes {
                    try? Daemon.closeTab(pane.tab, in: id.workspace)
                }
                try Daemon.closeTab(id.root, in: id.workspace)
                SurfacePool.shared.discard(workspace: id.workspace, tab: id.root)
                for pane in panes {
                    SurfacePool.shared.discard(workspace: id.workspace, tab: pane.tab)
                }
                sidebarStore.forget(id)
                refreshFromDaemon()
                hydrateActive()
                publish()
            } catch {
                renderer?.present(error: error.localizedDescription)
            }

        case .killWorkspace(let name):
            do {
                try Daemon.kill(name)
            } catch {
                renderer?.present(error: error.localizedDescription)
            }
            SurfacePool.shared.discardAll(workspace: name)
            sidebarStore.forgetAll(workspace: name)
            refreshFromDaemon()
            if activeWorkspace == nil || activeWorkspace?.tabs.isEmpty == true {
                activeWorkspaceName = workspaces.first(where: { !$0.tabs.isEmpty })?.name
                if let workspace = activeWorkspace {
                    activate(workspace.activeTabID ?? workspace.tabs.first?.id)
                }
            }
            publish()

        case .split(let direction):
            guard let workspace = activeWorkspace, let tab = workspace.activeTab else { return }
            do {
                let pane = try Daemon.newTab(
                    in: workspace.name, splitOf: tab.focusedPane, splitDir: direction)
                tab.noteFocus(pane: pane)
                refreshFromDaemon()
                publish()
            } catch {
                renderer?.present(error: error.localizedDescription)
            }

        case .focusPane(let pane):
            activeWorkspace?.activeTab?.noteFocus(pane: pane)
            publish()

        case .setSidebar(let state):
            guard let tab = activeWorkspace?.activeTab else { return }
            tab.setSidebar(state)
            sidebarStore.save(state, for: tab.id)
            publish()
        }
    }

    // MARK: - internals

    /// THE switch. Workspace clicks, strip clicks, ⌘1–9 and empty-workspace
    /// entry all funnel here; there is exactly one switch path in the program.
    private func activate(_ id: TabID?) {
        guard let id, let workspace = workspaces.first(where: { $0.name == id.workspace })
        else { return }
        // Seed a first-time sidebar from whatever is on screen, so entering a
        // new tab never jumps the sidebar.
        let seed = activeWorkspace?.activeTab?.sidebarIfHydrated
        activeWorkspaceName = id.workspace
        workspace.activate(id)
        _ = workspace.activeTab?.sidebarState(loading: sidebarStore, seed: seed)
    }

    /// Re-list and reconcile after any mutation the daemon took part in.
    private func refreshFromDaemon() {
        guard let listing = try? Daemon.list() else { return }
        reconcile(listing)
    }

    private func publish() {
        let snapshot = makeSnapshot()
        guard snapshot != lastSnapshot else { return }
        lastSnapshot = snapshot
        renderer?.render(snapshot)
    }

    private func makeSnapshot() -> SessionSnapshot {
        let rows = workspaces.map { workspace in
            SessionSnapshot.SidebarRow(
                name: workspace.name,
                subtitle: workspace.subtitle,
                dot: workspace.dot,
                isActive: workspace.name == activeWorkspaceName
            )
        }
        let strip = (activeWorkspace?.tabs ?? []).map { tab in
            SessionSnapshot.StripItem(
                id: tab.id,
                title: tab.title,
                busy: tab.busy,
                hasPanes: !tab.panes.isEmpty,
                isActive: tab.id == activeWorkspace?.activeTabID
            )
        }
        let active = activeWorkspace?.activeTab.map { tab in
            SessionSnapshot.ActiveTab(
                id: tab.id,
                title: tab.title,
                panes: tab.panes,
                sidebar: tab.sidebarIfHydrated ?? .initial,
                focusedPane: tab.focusedPane
            )
        }
        let universe = Set(workspaces.flatMap { $0.tabs.map(\.id) })
        return SessionSnapshot(rows: rows, strip: strip, active: active, universe: universe)
    }
}
