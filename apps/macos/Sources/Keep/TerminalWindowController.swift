import AppKit

/// One window per tab.
///
/// This is what buys native tabs: macOS groups windows that share a
/// `tabbingIdentifier` into one tab bar, and everything that comes with it —
/// ⌘1…⌘9, drag to reorder, the tab overview — is then the system's job rather
/// than ours. Grouping by session name is what keeps one project's tabs from
/// mixing with another's.
final class TerminalWindowController: NSWindowController, NSWindowDelegate {
    let session: String
    let tab: UInt32
    private let surface: TerminalSurfaceView

    init(session: String, tab: UInt32) {
        self.session = session
        self.tab = tab
        self.surface = TerminalSurfaceView(session: session, tab: tab)

        let window = TerminalWindow(
            contentRect: NSRect(x: 0, y: 0, width: 900, height: 560),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.installToolbar()
        window.tabbingMode = .preferred
        // Same session, same tab group.
        window.tabbingIdentifier = "keep-session-\(session)"
        window.title = "\(session) · tab \(tab)"
        window.contentView = surface
        window.isReleasedWhenClosed = false
        // macOS otherwise restores a frame from a previous run and the window
        // opens at whatever size it last happened to be, ignoring ours.
        window.isRestorable = false

        super.init(window: window)
        window.delegate = self
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    /// The label shown on the tab.
    ///
    /// The title is shown as the program set it, the way a terminal does:
    /// trimming it to the path lost the distinction between tabs whose paths
    /// happen to match. Busy tabs are marked so a running one stands out
    /// without opening it.
    func updateTitle(_ title: String, busy: Bool) {
        let trimmed = title.trimmingCharacters(in: .whitespaces)
        let label = trimmed.isEmpty ? "tab \(tab)" : trimmed
        window?.title = busy ? "✳ \(label)" : label
    }

    func windowWillClose(_ notification: Notification) {
        WindowManager.shared.forget(self)
    }
}
