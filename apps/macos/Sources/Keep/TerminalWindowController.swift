import AppKit
import SwiftUI

/// One window per root tab, which is what native tabs are. Panes split from
/// that tab render side by side inside the same window.
final class TerminalWindowController: NSWindowController, NSWindowDelegate {
    /// What this window currently shows. Not fixed at birth: a window is a
    /// slot the app rebinds, so that switching workspace changes what is on
    /// screen without changing which windows exist.
    private(set) var workspace: String = ""
    /// The tab that anchors this window.
    private(set) var rootTab: UInt32 = 0

    /// Set while the daemon says the tab is gone, so closing the window does
    /// not try to close its tabs a second time.
    var isClosingBecauseTabEnded = false

    /// Set while the app is shrinking the window pool. The tabs this window
    /// was showing keep running: the window is surplus, not the work in it.
    var isRecycling = false

    private let store: Store
    private let paneSplit = NSSplitView()
    private var panes: [(tab: UInt32, surface: TerminalSurfaceView)] = []

    /// Every daemon tab shown in this window, root first.
    var tabs: [UInt32] { panes.map(\.tab) }

    init(store: Store) {
        self.store = store

        paneSplit.dividerStyle = .thin
        paneSplit.isVertical = true

        // The window's content extends under the titlebar (fullSizeContentView,
        // which the full-height sidebar needs), so the panes hang off the safe
        // area or their first rows render behind the title and tab bar.
        let container = NSView()
        let chromeBackdrop = TerminalTintBackdropView()
        chromeBackdrop.wantsLayer = true
        chromeBackdrop.translatesAutoresizingMaskIntoConstraints = false
        paneSplit.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(chromeBackdrop)
        container.addSubview(paneSplit)
        NSLayoutConstraint.activate([
            // The terminal renderer begins below the safe area, so continue
            // its resolved background through the otherwise-clear toolbar
            // strip. Keeping this inside the terminal split item avoids
            // placing another tint below the flat leading panel.
            chromeBackdrop.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            chromeBackdrop.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            chromeBackdrop.topAnchor.constraint(equalTo: container.topAnchor),
            chromeBackdrop.bottomAnchor.constraint(equalTo: container.safeAreaLayoutGuide.topAnchor),
            paneSplit.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            paneSplit.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            paneSplit.topAnchor.constraint(equalTo: container.safeAreaLayoutGuide.topAnchor),
            paneSplit.bottomAnchor.constraint(equalTo: container.bottomAnchor),
        ])
        let terminal = NSViewController()
        terminal.view = container

        // ClearMic's inspector is content-owned instead of using AppKit's
        // floating Tahoe sidebar material. Keep the same flat treatment here:
        // the background fills the entire split item while the SwiftUI list
        // starts below the traffic lights via the safe-area guide.
        let sidebar = FlatSidebarViewController(store: store)
        sidebar.view.setFrameSize(NSSize(width: 220, height: 400))

        let split = NSSplitViewController()
        let sidebarItem = NSSplitViewItem(viewController: sidebar)
        sidebarItem.minimumThickness = 180
        sidebarItem.maximumThickness = 320
        sidebarItem.canCollapse = true
        sidebarItem.canCollapseFromWindowResize = false
        sidebarItem.collapseBehavior = .preferResizingSiblingsWithFixedSplitView
        sidebarItem.holdingPriority = .defaultHigh
        sidebarItem.preferredThicknessFraction = 0.21
        split.addSplitViewItem(sidebarItem)
        split.addSplitViewItem(NSSplitViewItem(viewController: terminal))

        let window = KeepWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1040, height: 660),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.contentViewController = split

        window.tabbingMode = .preferred
        // One identifier for the whole app, not one per workspace. Windows of
        // different workspaces used to belong to different tab groups, which
        // is why switching had to hide one group and reveal another — and
        // hiding a window is what the system animates, reassigns focus around,
        // and a tiling window manager reacts to. One group means the windows
        // stay put and only their contents change.
        window.tabbingIdentifier = "keep-workspace"
        window.isReleasedWhenClosed = false
        // Switching workspace reveals one window and hides another. AppKit's
        // default for a titled window is to fade and scale both, which is the
        // switch appearing to blink — the windows are never rebuilt, they are
        // just animated in and out. Nothing here wants that animation.
        window.animationBehavior = .none
        // Restoration otherwise reopens windows at whatever size a previous
        // run left behind.
        window.isRestorable = false

        // ClearMic's SwiftUI `.windowToolbarStyle(.unified)` translates to a
        // real unified NSToolbar here. Its tracking separator binds the
        // toolbar's leading section to this flat split item while the window
        // owns the collapse/resize transition.
        window.installUnifiedToolbar(
            sidebarController: split,
            sidebarItem: sidebarItem,
            contentAnchor: container,
            chromeBackdrop: chromeBackdrop,
            sidebarBackdrop: sidebar.backdropView
        )

        super.init(window: window)
        window.delegate = self
        observeGeometry(of: window)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    // MARK: - tracing

    /// Report geometry changes and who caused them.
    ///
    /// `bySelf=false` means the change did not come from this app: someone
    /// dragged an edge, or a window manager re-tiled us. That distinction is
    /// invisible from outside the process and is the point of tracing here.
    private func observeGeometry(of window: NSWindow) {
        guard Trace.enabled else { return }
        for name in [NSWindow.didResizeNotification, NSWindow.didMoveNotification] {
            NotificationCenter.default.addObserver(
                forName: name, object: window, queue: .main
            ) { [weak self] note in
                guard let self else { return }
                let what = name == NSWindow.didResizeNotification ? "resize" : "move"
                Trace.log("geometry", "\(self.workspace)/\(self.rootTab) \(what) "
                    + "\(Trace.frame(note.object as? NSWindow)) bySelf=\(Trace.insideShow)")
            }
        }
    }

    // MARK: - binding

    /// Point this window at a workspace's root tab and its panes.
    ///
    /// Surfaces come from the pool, so a tab already open keeps the very view
    /// it had — its renderer and its client are untouched by being moved from
    /// one window into another. Panes no longer wanted are taken out of the
    /// layout, not thrown away.
    func bind(workspace: String, rootTab: UInt32, panes wanted: [(tab: UInt32, direction: UInt8?)]) {
        let changed = self.workspace != workspace || self.rootTab != rootTab
        self.workspace = workspace
        self.rootTab = rootTab
        Trace.log("bind", "\(workspace)/\(rootTab) panes=\(wanted.map(\.tab)) changed=\(changed)")

        // Tab ids are numbered per workspace, so `dawd/1` and `luhw/1` are two
        // different tabs wearing the same number. Comparing ids across a
        // workspace change would keep the old workspace's surface and never
        // open the new one's — so a change of workspace replaces everything.
        let keep = changed ? Set<UInt32>() : Set(wanted.map(\.tab))
        for pane in panes where !keep.contains(pane.tab) {
            pane.surface.removeFromSuperview()
        }
        panes.removeAll { !keep.contains($0.tab) }

        for (index, want) in wanted.enumerated() {
            if panes.contains(where: { $0.tab == want.tab }) { continue }
            if index == 1, let direction = want.direction {
                // 2 = down; anything else splits to the right.
                paneSplit.isVertical = direction != 2
            }
            let surface = SurfacePool.shared.surface(workspace: workspace, tab: want.tab)
            panes.append((want.tab, surface))
            paneSplit.addArrangedSubview(surface)
        }

        // Match the daemon's order, so panes do not shuffle between switches.
        let order = wanted.map(\.tab)
        panes.sort { (order.firstIndex(of: $0.tab) ?? 0) < (order.firstIndex(of: $1.tab) ?? 0) }

        equalizePanes()
        if let first = panes.first {
            window?.makeFirstResponder(first.surface)
        }
    }

    // MARK: - panes

    /// Add a pane. `direction` orients the whole split on first use; panes
    /// added later follow the established orientation.
    func addPane(tab: UInt32, direction: UInt8?) {
        guard !panes.contains(where: { $0.tab == tab }) else { return }
        if panes.count == 1, let direction {
            // 2 = down; anything else splits to the right.
            paneSplit.isVertical = direction != 2
        }
        let surface = SurfacePool.shared.surface(workspace: workspace, tab: tab)
        panes.append((tab, surface))
        paneSplit.addArrangedSubview(surface)
        equalizePanes()
        window?.makeFirstResponder(surface)
    }

    func removePane(tab: UInt32) {
        guard let index = panes.firstIndex(where: { $0.tab == tab }) else { return }
        // Out of the layout, not out of existence: the pool owns it.
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

    /// Called just before the window comes back on screen.
    ///
    /// Its surfaces stopped drawing while hidden, so without this the window
    /// is revealed with nothing presented and shows empty until the next draw
    /// lands. Laying out first means the frame they draw is for the geometry
    /// the window is about to have rather than the one it is leaving.
    func willReveal() {
        Trace.log("reveal", "\(workspace)/\(rootTab) panes=\(panes.count) frame=\(Trace.frame(window))")
        window?.contentView?.layoutSubtreeIfNeeded()
        // The keyboard belongs to the terminal. Revealing a window lets the
        // sidebar list grab it otherwise, and then typing goes to a list.
        if let surface = panes.first?.surface {
            window?.makeFirstResponder(surface)
        }
        for pane in panes {
            pane.surface.resumeDrawing()
        }
    }

    /// Called once the window is off screen. The surfaces stay alive — the
    /// workspace is hidden, not closed — but nothing should be drawing them.
    func willHide() {
        for pane in panes {
            pane.surface.suspendDrawing()
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
    ///
    /// Assigned only when it actually changed. The poller calls this for every
    /// window every two seconds, and setting a window's title posts an
    /// accessibility notification whether or not the text differs — which is
    /// exactly what a window manager listens to. Unchanged titles should be
    /// silent.
    func updateTitle(_ title: String, busy: Bool) {
        let trimmed = title.trimmingCharacters(in: .whitespaces)
        var label = trimmed.isEmpty ? "tab \(rootTab)" : trimmed
        if panes.count > 1 { label += "  ⊞" }
        let next = busy ? "✳ \(label)" : label
        guard window?.title != next else { return }
        window?.title = next
    }

    /// The native tab bar shows its "+" button when this is implemented
    /// anywhere in the responder chain.
    override func newWindowForTab(_ sender: Any?) {
        store.newTab(in: workspace)
    }

    func windowWillClose(_ notification: Notification) {
        WindowManager.shared.forget(self)
        // Closing a tab means closing it — otherwise it would linger in the
        // daemon and reappear on the next visit to this workspace. But only
        // when the person closed it: on quit the windows close because the
        // app is going away, and the daemon must keep every tab running.
        if !isClosingBecauseTabEnded && !isRecycling && !WindowManager.shared.isQuitting {
            for tab in tabs {
                store.close(tab: tab, in: workspace)
            }
        }
    }
}

/// A default split item has no Tahoe sidebar glass, inset, or rounded floating
/// container. The hosted content respects the titlebar safe area, while this
/// controller's own background still reaches every edge of the window.
private final class FlatSidebarViewController: NSViewController {
    let backdropView = TerminalTintBackdropView()
    private let hostingController: NSHostingController<WorkspaceSidebar>

    init(store: Store) {
        hostingController = NSHostingController(rootView: WorkspaceSidebar(store: store))
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    override func loadView() {
        let container = NSView()
        backdropView.wantsLayer = true
        backdropView.translatesAutoresizingMaskIntoConstraints = false

        addChild(hostingController)
        let content = hostingController.view
        content.translatesAutoresizingMaskIntoConstraints = false

        container.addSubview(backdropView)
        container.addSubview(content)
        NSLayoutConstraint.activate([
            backdropView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            backdropView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            backdropView.topAnchor.constraint(equalTo: container.topAnchor),
            backdropView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            content.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            content.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            content.topAnchor.constraint(equalTo: container.safeAreaLayoutGuide.topAnchor),
            content.bottomAnchor.constraint(equalTo: container.bottomAnchor),
        ])
        view = container
    }
}

/// A visual continuation of the terminal behind the unified toolbar. It must
/// never take clicks away from the native titlebar controls or window drag.
private final class TerminalTintBackdropView: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}
