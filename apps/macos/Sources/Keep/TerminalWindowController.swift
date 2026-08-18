import AppKit
import SwiftUI

/// One window per tab, which is what native tabs are.
///
/// macOS groups windows sharing a `tabbingIdentifier` into a single window
/// with a tab bar, so grouping by workspace gives each workspace its own set
/// of tabs. ⌘1…⌘9, drag to reorder and the tab overview then come from the
/// system rather than from us.
final class TerminalWindowController: NSWindowController, NSWindowDelegate {
    let workspace: String
    let tab: UInt32

    /// Set while the daemon says the tab is gone, so closing the window does
    /// not try to close the tab a second time.
    var isClosingBecauseTabEnded = false

    private let store: Store

    init(workspace: String, tab: UInt32, store: Store) {
        self.workspace = workspace
        self.tab = tab
        self.store = store

        let surface = TerminalSurfaceView(workspace: workspace, tab: tab)
        let terminal = NSViewController()
        terminal.view = surface

        let sidebar = NSHostingController(rootView: WorkspaceSidebar(store: store))
        sidebar.view.setFrameSize(NSSize(width: 220, height: 400))

        let split = NSSplitViewController()
        let sidebarItem = NSSplitViewItem(sidebarWithViewController: sidebar)
        sidebarItem.minimumThickness = 180
        sidebarItem.maximumThickness = 320
        sidebarItem.canCollapse = true
        split.addSplitViewItem(sidebarItem)
        split.addSplitViewItem(NSSplitViewItem(viewController: terminal))

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1040, height: 660),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.contentViewController = split
        window.tabbingMode = .preferred
        window.tabbingIdentifier = "keep-workspace-\(workspace)"
        window.isReleasedWhenClosed = false
        // Restoration otherwise reopens windows at whatever size a previous
        // run left behind.
        window.isRestorable = false

        // The tab bar sits in the titlebar only when the window carries a
        // compact unified toolbar, and there is no title beside it.
        let toolbar = NSToolbar(identifier: "KeepToolbar")
        toolbar.showsBaselineSeparator = false
        window.toolbar = toolbar
        window.toolbarStyle = .unifiedCompact
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden

        super.init(window: window)
        window.delegate = self
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    /// The tab's label. Shown as the program set it, the way a terminal does.
    func updateTitle(_ title: String, busy: Bool) {
        let trimmed = title.trimmingCharacters(in: .whitespaces)
        let label = trimmed.isEmpty ? "tab \(tab)" : trimmed
        window?.title = busy ? "✳ \(label)" : label
    }

    func windowWillClose(_ notification: Notification) {
        WindowManager.shared.forget(self)
        // Closing a tab means closing it — otherwise it would linger in the
        // daemon and reappear on the next visit to this workspace. But only
        // when the person closed it: on quit the windows close because the
        // app is going away, and the daemon must keep every tab running.
        if !isClosingBecauseTabEnded && !WindowManager.shared.isQuitting {
            store.close(tab: tab, in: workspace)
        }
    }
}
