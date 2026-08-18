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
        // The window's content extends under the titlebar (fullSizeContentView,
        // which the full-height sidebar needs), so the terminal must hang off
        // the safe area or its first rows render behind the title and tab bar.
        let container = NSView()
        surface.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(surface)
        NSLayoutConstraint.activate([
            surface.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            surface.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            surface.topAnchor.constraint(equalTo: container.safeAreaLayoutGuide.topAnchor),
            surface.bottomAnchor.constraint(equalTo: container.bottomAnchor),
        ])
        let terminal = NSViewController()
        terminal.view = container

        let sidebar = NSHostingController(rootView: WorkspaceSidebar(store: store))
        sidebar.view.setFrameSize(NSSize(width: 220, height: 400))

        let split = NSSplitViewController()
        let sidebarItem = NSSplitViewItem(sidebarWithViewController: sidebar)
        sidebarItem.minimumThickness = 180
        sidebarItem.maximumThickness = 320
        sidebarItem.canCollapse = true
        // The Finder treatment: the sidebar runs the full height of the
        // window, under the titlebar, with the traffic lights floating over
        // it. The split item's material provides the translucency.
        sidebarItem.allowsFullHeightLayout = true
        sidebarItem.titlebarSeparatorStyle = .none
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

        // The Finder arrangement: the toolbar row shows the active tab's
        // title over the content area, the sidebar owns the strip to its
        // left, and the native tab bar slots in beneath when there are tabs.
        let toolbar = NSToolbar(identifier: "KeepToolbar")
        toolbar.showsBaselineSeparator = false
        window.toolbar = toolbar
        window.toolbarStyle = .unified
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .visible

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
