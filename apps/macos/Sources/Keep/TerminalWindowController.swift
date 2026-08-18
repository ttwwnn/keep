import AppKit
import SwiftUI

/// One window per root tab, which is what native tabs are. Panes split from
/// that tab render side by side inside the same window.
final class TerminalWindowController: NSWindowController, NSWindowDelegate {
    let workspace: String
    /// The tab that anchors this window.
    let rootTab: UInt32

    /// Set while the daemon says the tab is gone, so closing the window does
    /// not try to close its tabs a second time.
    var isClosingBecauseTabEnded = false

    private let store: Store
    private let paneSplit = NSSplitView()
    private var panes: [(tab: UInt32, surface: TerminalSurfaceView)] = []

    /// Every daemon tab shown in this window, root first.
    var tabs: [UInt32] { panes.map(\.tab) }

    init(workspace: String, rootTab: UInt32, store: Store) {
        self.workspace = workspace
        self.rootTab = rootTab
        self.store = store

        paneSplit.dividerStyle = .thin
        paneSplit.isVertical = true

        // The window's content extends under the titlebar (fullSizeContentView,
        // which the full-height sidebar needs), so the panes hang off the safe
        // area or their first rows render behind the title and tab bar.
        let container = NSView()
        paneSplit.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(paneSplit)
        NSLayoutConstraint.activate([
            paneSplit.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            paneSplit.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            paneSplit.topAnchor.constraint(equalTo: container.safeAreaLayoutGuide.topAnchor),
            paneSplit.bottomAnchor.constraint(equalTo: container.bottomAnchor),
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

        addPane(tab: rootTab, direction: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    // MARK: - panes

    /// Add a pane. `direction` orients the whole split on first use; panes
    /// added later follow the established orientation.
    func addPane(tab: UInt32, direction: UInt8?) {
        guard !panes.contains(where: { $0.tab == tab }) else { return }
        if panes.count == 1, let direction {
            // 2 = down; anything else splits to the right.
            paneSplit.isVertical = direction != 2
        }
        let surface = TerminalSurfaceView(workspace: workspace, tab: tab)
        surface.translatesAutoresizingMaskIntoConstraints = false
        panes.append((tab, surface))
        paneSplit.addArrangedSubview(surface)
        equalizePanes()
        window?.makeFirstResponder(surface)
    }

    func removePane(tab: UInt32) {
        guard let index = panes.firstIndex(where: { $0.tab == tab }) else { return }
        let (_, surface) = panes.remove(at: index)
        surface.removeFromSuperview()
        equalizePanes()
        if let first = panes.first {
            window?.makeFirstResponder(first.surface)
        }
    }

    private func equalizePanes() {
        guard panes.count > 1 else { return }
        paneSplit.layoutSubtreeIfNeeded()
        let total = paneSplit.isVertical ? paneSplit.bounds.width : paneSplit.bounds.height
        guard total > 0 else { return }
        for i in 1..<panes.count {
            paneSplit.setPosition(total * CGFloat(i) / CGFloat(panes.count), ofDividerAt: i - 1)
        }
    }

    /// The pane the keyboard is in — where a further split should hang.
    var focusedTab: UInt32 {
        if let responder = window?.firstResponder as? TerminalSurfaceView,
            let pane = panes.first(where: { $0.surface === responder })
        {
            return pane.tab
        }
        return rootTab
    }

    /// The tab's label. Shown as the program set it, the way a terminal does.
    func updateTitle(_ title: String, busy: Bool) {
        let trimmed = title.trimmingCharacters(in: .whitespaces)
        var label = trimmed.isEmpty ? "tab \(rootTab)" : trimmed
        if panes.count > 1 { label += "  ⊞" }
        window?.title = busy ? "✳ \(label)" : label
    }

    func windowWillClose(_ notification: Notification) {
        WindowManager.shared.forget(self)
        // Closing a tab means closing it — otherwise it would linger in the
        // daemon and reappear on the next visit to this workspace. But only
        // when the person closed it: on quit the windows close because the
        // app is going away, and the daemon must keep every tab running.
        if !isClosingBecauseTabEnded && !WindowManager.shared.isQuitting {
            for tab in tabs {
                store.close(tab: tab, in: workspace)
            }
        }
    }
}
