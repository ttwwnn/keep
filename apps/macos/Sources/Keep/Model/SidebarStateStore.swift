import Foundation

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
        let dir = directory ?? FileManager.default.urls(
            for: .applicationSupportDirectory, in: .userDomainMask
        )[0].appendingPathComponent("Keep", isDirectory: true)
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
        let dir = directory ?? FileManager.default.urls(
            for: .applicationSupportDirectory, in: .userDomainMask
        )[0].appendingPathComponent("Keep", isDirectory: true)
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
