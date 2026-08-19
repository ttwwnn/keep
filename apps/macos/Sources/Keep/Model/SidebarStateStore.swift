import Foundation

/// Per-tab sidebar state across app restarts.
///
/// App-side only — the daemon stores no UI state. One small JSON file:
/// ~/Library/Application Support/Keep/sidebar-state.json, shaped
/// `{ "workspace": { "root": { "isCollapsed": false, "width": 220 } } }`.
///
/// Writes are debounced: a divider drag reports continuously and none of it
/// is worth an fsync per event. (Caveat, accepted: daemon tab ids can recycle
/// after a daemon restart, so a recycled id inherits stale sidebar state.)
@MainActor
final class SidebarStateStore {
    private var states: [String: [String: SidebarState]]
    private let file: URL
    private var writeScheduled = false

    init(directory: URL? = nil) {
        let dir = directory ?? FileManager.default.urls(
            for: .applicationSupportDirectory, in: .userDomainMask
        )[0].appendingPathComponent("Keep", isDirectory: true)
        file = dir.appendingPathComponent("sidebar-state.json")
        states = (try? JSONDecoder().decode(
            [String: [String: SidebarState]].self, from: Data(contentsOf: file)
        )) ?? [:]
    }

    func state(for id: TabID) -> SidebarState? {
        states[id.workspace]?[String(id.root)]
    }

    func save(_ state: SidebarState, for id: TabID) {
        states[id.workspace, default: [:]][String(id.root)] = state
        scheduleWrite()
    }

    func forget(_ id: TabID) {
        states[id.workspace]?[String(id.root)] = nil
        if states[id.workspace]?.isEmpty == true { states[id.workspace] = nil }
        scheduleWrite()
    }

    func forgetAll(workspace: String) {
        guard states[workspace] != nil else { return }
        states[workspace] = nil
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
        try? JSONEncoder().encode(states).write(to: file, options: .atomic)
    }

    private func scheduleWrite() {
        guard !writeScheduled else { return }
        writeScheduled = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
            guard let self else { return }
            guard self.writeScheduled else { return }
            self.writeScheduled = false
            self.write()
        }
    }
}
