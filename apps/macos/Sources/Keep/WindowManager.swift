import AppKit

/// Keeps the windows and the daemon's view of the world in step.
final class WindowManager {
    static let shared = WindowManager()

    static let defaultContentSize = NSSize(width: 960, height: 620)

    private var controllers: [TerminalWindowController] = []
    private var poll: Timer?

    private init() {}

    /// Bring the whole daemon state on screen: a window per live tab, grouped
    /// into one tab bar per session.
    func sync() {
        let sessions = (try? Daemon.list()) ?? []

        for session in sessions {
            for tab in session.liveTabs {
                if let existing = controller(session: session.name, tab: tab.id) {
                    existing.updateTitle(tab.title, busy: tab.busy)
                } else {
                    open(session: session.name, tab: tab.id, title: tab.title, busy: tab.busy)
                }
            }
        }

        // Close windows whose tab is gone from the daemon.
        let live = Set(sessions.flatMap { s in s.liveTabs.map { "\(s.name)#\($0.id)" } })
        for controller in controllers where !live.contains("\(controller.session)#\(controller.tab)") {
            controller.close()
        }
    }

    func startPolling() {
        sync()
        poll = Timer.scheduledTimer(withTimeInterval: 2, repeats: true) { [weak self] _ in
            self?.sync()
        }
    }

    @discardableResult
    func open(session: String, tab: UInt32, title: String = "", busy: Bool = false)
        -> TerminalWindowController
    {
        if let existing = controller(session: session, tab: tab) {
            existing.showWindow(nil)
            return existing
        }

        let controller = TerminalWindowController(session: session, tab: tab)
        controller.updateTitle(title, busy: busy)
        controllers.append(controller)

        // Join this session's tab group rather than opening a loose window.
        let sibling = controllers.first { $0.session == session && $0 !== controller }?.window
        if let sibling, let window = controller.window {
            sibling.addTabbedWindow(window, ordered: .above)
        }

        controller.showWindow(nil)

        // Only the window that opens a group chooses the size; the rest are
        // tabs and must take the group's frame, or joining would resize the
        // window out from under whoever is using it.
        controller.window?.makeKeyAndOrderFront(nil)

        if sibling == nil, let window = controller.window {
            var frame = window.frameRect(forContentRect: NSRect(origin: .zero, size: Self.defaultContentSize))
            if let screen = window.screen ?? NSScreen.main {
                let visible = screen.visibleFrame
                frame.origin = NSPoint(
                    x: visible.midX - frame.width / 2,
                    y: visible.midY - frame.height / 2
                )
            }
            window.setFrame(frame, display: true)
        }
        return controller
    }

    func newTab(in session: String) {
        guard let id = try? Daemon.newTab(in: session) else { return }
        open(session: session, tab: id)
    }

    /// The session the frontmost window belongs to, for "new tab here".
    var currentSession: String? {
        (NSApp.keyWindow?.windowController as? TerminalWindowController)?.session
            ?? controllers.first?.session
    }

    func forget(_ controller: TerminalWindowController) {
        controllers.removeAll { $0 === controller }
    }

    private func controller(session: String, tab: UInt32) -> TerminalWindowController? {
        controllers.first { $0.session == session && $0.tab == tab }
    }
}
