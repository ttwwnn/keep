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
