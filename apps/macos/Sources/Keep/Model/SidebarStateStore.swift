import Foundation

/// Where this app keeps what it remembers between launches.
///
/// `KEEP_STATE_DIR` first, so a test can be handed a directory of its own.
/// Without it a test either writes into the state of whoever is running it —
/// `tools/tabs-test.sh` used to scrub its own leftovers out of the real file
/// afterwards — or reads their arrangement and reports on it as though it were
/// its own setup.
func stateDirectory() -> URL {
    if let override = ProcessInfo.processInfo.environment["KEEP_STATE_DIR"],
       !override.isEmpty {
        return URL(fileURLWithPath: override, isDirectory: true)
    }
    return FileManager.default.urls(
        for: .applicationSupportDirectory, in: .userDomainMask
    )[0].appendingPathComponent("Keep", isDirectory: true)
}

/// The sidebar's state across app restarts.
///
/// App-side only — the daemon stores no UI state. One small JSON file:
/// ~/Library/Application Support/Keep/sidebar-state.json.
///
/// Writes are debounced: a divider drag reports continuously and none of it
/// is worth an fsync per event. `flush` settles the debt at quit.
@MainActor
final class SidebarStateStore {
    private(set) var state: SidebarState
    private let file: URL
    private var writeScheduled = false

    init(directory: URL? = nil) {
        let dir = directory ?? stateDirectory()
        file = dir.appendingPathComponent("sidebar-state.json")
        state = (try? JSONDecoder().decode(
            SidebarState.self, from: Data(contentsOf: file)
        )) ?? .initial
    }

    func save(_ newState: SidebarState) {
        guard newState != state else { return }
        state = newState
        scheduleWrite()
    }

    /// Write now, debounce or not.
    func flush() {
        writeScheduled = false
        write()
    }

    private func write() {
        try? FileManager.default.createDirectory(
            at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? JSONEncoder().encode(state).write(to: file, options: .atomic)
    }

    private func scheduleWrite() {
        guard !writeScheduled else { return }
        writeScheduled = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
            guard let self, self.writeScheduled else { return }
            self.writeScheduled = false
            self.write()
        }
    }
}

/// The order the workspaces are listed in.
///
/// App-side, like the sidebar's own state, because this is a preference about
/// looking rather than a fact about what is running: the daemon knows which
/// workspaces exist, and nothing about which one you want at the top.
///
/// Names, not indices. A workspace that goes away and comes back keeps its
/// place, and one that has never been seen is new rather than misplaced.
@MainActor
final class WorkspaceOrderStore {
    private(set) var order: [String]
    private let file: URL

    init(directory: URL? = nil) {
        let dir = directory ?? stateDirectory()
        file = dir.appendingPathComponent("workspace-order.json")
        order = (try? JSONDecoder().decode(
            [String].self, from: Data(contentsOf: file)
        )) ?? []
    }

    func save(_ names: [String]) {
        guard names != order else { return }
        order = names
        try? FileManager.default.createDirectory(
            at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? JSONEncoder().encode(order).write(to: file, options: .atomic)
    }

    /// Sort names into the remembered order, with anything unheard-of at the
    /// end in alphabetical order — a workspace made a moment ago appears where
    /// it was made, at the bottom, rather than jumping into the middle of a
    /// list somebody arranged by hand.
    func arrange(_ names: [String]) -> [String] {
        let placed = order.filter(names.contains)
        let rest = names.filter { !order.contains($0) }.sorted()
        return placed + rest
    }
}

/// The order the tabs of each workspace are listed in.
///
/// App-side, like the workspaces' own order: the daemon knows which tabs
/// exist and nothing about which one you want first. What it does own is the
/// numbers — so this remembers ids, and ids are only meaningful while the
/// daemon that issued them is alive. A daemon restarted hands out fresh ones
/// and the remembered order quietly stops applying, which is the right way
/// for it to fail.
@MainActor
final class TabOrderStore {
    private(set) var order: [String: [UInt32]]
    private let file: URL

    init(directory: URL? = nil) {
        let dir = directory ?? stateDirectory()
        file = dir.appendingPathComponent("tab-order.json")
        order = (try? JSONDecoder().decode(
            [String: [UInt32]].self, from: Data(contentsOf: file)
        )) ?? [:]
    }

    func save(_ tabs: [UInt32], in workspace: String) {
        guard order[workspace] != tabs else { return }
        order[workspace] = tabs
        try? FileManager.default.createDirectory(
            at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? JSONEncoder().encode(order).write(to: file, options: .atomic)
    }

    /// Sort ids into the remembered order, with anything unheard-of kept where
    /// the daemon had it — a tab made a moment ago appears where it was made,
    /// at the end, rather than at the front of a row somebody arranged.
    func arrange(_ ids: [UInt32], in workspace: String) -> [UInt32] {
        guard let remembered = order[workspace] else { return ids }
        let placed = remembered.filter(ids.contains)
        let rest = ids.filter { !remembered.contains($0) }
        return placed + rest
    }
}
