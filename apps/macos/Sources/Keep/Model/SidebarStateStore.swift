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

/// Each window's sidebar, across app restarts.
///
/// App-side only — the daemon stores no UI state. One small JSON file:
/// sidebar-state.json, keyed by window slot.
///
/// One value per window, not one per tab and not one for the app. Per tab was
/// tried and read as a glitch: you collapse it, move to another tab, and it
/// is back. But a window is not a tab, and one value for the app reproduces
/// that same glitch across windows — collapse it in the narrow one and the
/// wide one you did not touch rearranges itself. The rule is what it always
/// was, with a word added: furniture stays where you put it, in the room you
/// put it in.
///
/// Writes are debounced: a divider drag reports continuously and none of it
/// is worth an fsync per event. `flush` settles the debt at quit.
@MainActor
final class SidebarStateStore {
    private var states: [Int: SidebarState]
    private let file: URL
    private var writeScheduled = false

    init(directory: URL? = nil) {
        let dir = directory ?? stateDirectory()
        file = dir.appendingPathComponent("sidebar-state.json")
        let data = (try? Data(contentsOf: file)) ?? Data()
        if let keyed = try? JSONDecoder().decode([Int: SidebarState].self, from: data) {
            states = keyed
        } else if let bare = try? JSONDecoder().decode(SidebarState.self, from: data) {
            // What every earlier version wrote. Whoever had one sidebar keeps
            // it, and it becomes what a second window starts from.
            states = [WindowID.first.slot: bare]
        } else {
            states = [:]
        }
    }

    /// A window nobody has arranged yet inherits the first window's, and the
    /// factory setting if there is no first window either — so a new window
    /// opens looking like the one it was opened from.
    func state(for window: WindowID) -> SidebarState {
        states[window.slot] ?? states[WindowID.first.slot] ?? .initial
    }

    func save(_ newState: SidebarState, for window: WindowID) {
        guard states[window.slot] != newState else { return }
        states[window.slot] = newState
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
