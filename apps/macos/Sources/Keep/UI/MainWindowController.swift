import AppKit

/// THE window, alive for the app's lifetime.
///
/// Layer 6's root: it receives immutable snapshots from the session and
/// drives exactly four things — the tab strip, the content container, the
/// sidebar, and the window title. It dispatches intents and mutates no model
/// state. No window is ever created, closed, ordered, animated, or resized
/// by a switch; that sentence is the fix for every transition bug this app
/// ever had.
@MainActor
final class MainWindowController: NSWindowController, NSWindowDelegate {
    private let session: Session
    private let tabStrip = TabStripView()
    private let container = TabContentContainer()
    private let sidebarHost: SidebarHost
    private weak var sidebarItem: NSSplitViewItem?
    private weak var splitView: NSSplitView?

    private var applied: SessionSnapshot?
    /// True while render is moving geometry, so the split-view delegate does
    /// not echo model-driven changes back as user intents.
    private var isApplyingSnapshot = false
    private var dividerReportScheduled = false
    private var lastExpandedWidth: CGFloat = SidebarState.initial.width

    init(session: Session) {
        self.session = session
        sidebarHost = SidebarHost(dispatch: { [weak session] in session?.dispatch($0) })

        // The window's content extends under the titlebar (fullSizeContentView,
        // which the full-height sidebar needs), so the terminal container hangs
        // off the safe area or its first rows render behind the chrome row.
        let content = NSView()
        let chromeBackdrop = TerminalTintBackdropView()
        chromeBackdrop.wantsLayer = true
        chromeBackdrop.translatesAutoresizingMaskIntoConstraints = false
        container.translatesAutoresizingMaskIntoConstraints = false
        tabStrip.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(chromeBackdrop)
        content.addSubview(container)
        // The strip lives in the chrome row — the same region the backdrop
        // tints, above the terminal, beside the sidebar. Plain content: no
        // toolbar sizing, no private views, and empty regions still drag the
        // window because the strip's hitTest passes them through.
        content.addSubview(tabStrip)
        NSLayoutConstraint.activate([
            chromeBackdrop.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            chromeBackdrop.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            chromeBackdrop.topAnchor.constraint(equalTo: content.topAnchor),
            chromeBackdrop.bottomAnchor.constraint(equalTo: content.safeAreaLayoutGuide.topAnchor),
            tabStrip.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            tabStrip.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            tabStrip.topAnchor.constraint(equalTo: content.topAnchor),
            tabStrip.bottomAnchor.constraint(equalTo: content.safeAreaLayoutGuide.topAnchor),
            container.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            container.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            container.topAnchor.constraint(equalTo: content.safeAreaLayoutGuide.topAnchor),
            container.bottomAnchor.constraint(equalTo: content.bottomAnchor),
        ])
        let terminal = NSViewController()
        terminal.view = content

        let split = NSSplitViewController()
        let sidebarSplitItem = NSSplitViewItem(viewController: sidebarHost)
        sidebarSplitItem.minimumThickness = 180
        sidebarSplitItem.maximumThickness = 320
        sidebarSplitItem.canCollapse = true
        sidebarSplitItem.canCollapseFromWindowResize = false
        sidebarSplitItem.collapseBehavior = .preferResizingSiblingsWithFixedSplitView
        sidebarSplitItem.holdingPriority = .defaultHigh
        split.addSplitViewItem(sidebarSplitItem)
        split.addSplitViewItem(NSSplitViewItem(viewController: terminal))

        let window = KeepWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1040, height: 660),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.contentViewController = split
        // Native tabbing is gone for good; switching is a visibility flip.
        window.tabbingMode = .disallowed
        window.animationBehavior = .none
        window.isReleasedWhenClosed = false
        // Restoration otherwise reopens at whatever size a previous run left.
        window.isRestorable = false
        window.setFrame(NSRect(x: 0, y: 0, width: 1040, height: 660), display: false)
        window.center()

        window.installUnifiedToolbar(
            sidebarController: split,
            chromeBackdrop: chromeBackdrop,
            sidebarBackdrop: sidebarHost.backdropView
        )

        self.sidebarItem = sidebarSplitItem
        self.splitView = split.splitView

        super.init(window: window)
        window.delegate = self

        window.onToggleSidebar = { [weak self] in
            guard let self, let state = self.currentSidebarGeometry() else { return }
            self.session.dispatch(.setSidebar(
                SidebarState(isCollapsed: !state.isCollapsed, width: state.width)))
        }
        container.onPaneFocus = { [weak self] id, pane in
            self?.session.dispatch(.focusPane(id, pane))
        }
        tabStrip.onSelect = { [weak self] id in self?.session.dispatch(.activateTab(id)) }
        tabStrip.onClose = { [weak self] id in self?.session.dispatch(.closeTab(id)) }
        tabStrip.onNewTab = { [weak self] in self?.session.dispatch(.newTab(in: nil)) }

        // Divider drags become model facts, debounced; model-driven geometry
        // is guarded out so it cannot echo back as intent.
        NotificationCenter.default.addObserver(
            forName: NSSplitView.didResizeSubviewsNotification,
            object: split.splitView,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.dividerMoved() }
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    // MARK: - SessionRendering

    func render(_ snapshot: SessionSnapshot) {
        // Tabs the daemon lost take their hosts with them.
        for id in container.hosts.keys where !snapshot.universe.contains(id) {
            container.unmount(id)
        }

        sidebarHost.render(snapshot.rows)
        tabStrip.apply(snapshot.strip)
        // The sidebar is one state for the whole app: applied here, once,
        // rather than restored per tab. The user's own toggle animates.
        if applied?.sidebar != snapshot.sidebar {
            applySidebar(snapshot.sidebar, animated: applied != nil)
        }

        guard let active = snapshot.active else {
            if let visible = container.visibleTab {
                container.hide(visible)
                container.markVisible(TabID(workspace: "", root: 0))
            }
            applied = snapshot
            return
        }

        if active.id != container.visibleTab {
            switchVisible(to: active)
        } else {
            // Same tab: apply what changed around it.
            let host = container.host(for: active)
            host.apply(root: active.id.root, panes: active.panes)
            host.setFocusedPane(active.focusedPane)
            if applied?.active?.focusedPane != active.focusedPane,
                let surface = host.surface(for: active.focusedPane),
                window?.firstResponder !== surface
            {
                window?.makeFirstResponder(surface)
            }
            updateTitle(active)
        }
        applied = snapshot
    }

    func focusActiveTerminal() {
        guard let id = container.visibleTab,
              let host = container.hosts[id],
              let pane = applied?.active?.focusedPane,
              let surface = host.surface(for: pane) ?? host.paneSurfaces.first,
              window?.firstResponder !== surface
        else { return }
        window?.makeFirstResponder(surface)
    }

    func present(error: String) {
        let alert = NSAlert()
        alert.messageText = "Keep"
        alert.informativeText = error
        alert.runModal()
    }

    // MARK: - the switch pipeline

    /// One synchronous pass, one run-loop turn, no dispatch hops. Everything
    /// commits in a single CoreAnimation transaction, so no composited frame
    /// can show an intermediate state.
    private func switchVisible(to active: SessionSnapshot.ActiveTab) {
        Trace.insideSwitch = true
        defer { Trace.insideSwitch = false }
        Trace.log("switch", "→ \(active.id) from=\(container.visibleTab.map(String.init(describing:)) ?? "none")")

        let previous = container.visibleTab
        let incoming = container.host(for: active)
        let outgoing = previous.flatMap { container.hosts[$0] }

        CATransaction.begin()
        CATransaction.setDisableActions(true)

        // 2. Arrangement and layout while still hidden (hidden views lay out
        //    fine; surface sizes flush on unhide).
        incoming.apply(root: active.id.root, panes: active.panes)
        incoming.setFocusedPane(active.focusedPane)
        incoming.frame = container.bounds
        incoming.layoutSubtreeIfNeeded()

        // 3. Reveal. Unhiding runs `viewDidUnhide` on every pane synchronously,
        //    inside this transaction: each flushes the size it deferred while
        //    hidden and draws one frame at final geometry. So the fresh frame
        //    lands before the commit, and at worst the layer still held its
        //    last presented one — either way, never a hole.
        incoming.isHidden = false

        // 4. Hide the outgoing LAST, in the same transaction: no commit ever
        //    has zero visible tabs. Its display links stop from viewDidHide.
        if let outgoing, outgoing !== incoming {
            outgoing.isHidden = true
        }

        CATransaction.commit()

        // 5. Focus: an intra-window responder move. The one window has been
        //    key since launch and never resigns; there is nothing here for
        //    the system, or a tiling window manager, to react to.
        if let surface = incoming.surface(for: active.focusedPane) {
            window?.makeFirstResponder(surface)
        }

        container.markVisible(active.id)
        updateTitle(active)
    }

    // MARK: - sidebar geometry

    /// A collapsed sidebar has no width to read, and reading zero — then
    /// substituting a default — would overwrite the width the tab actually
    /// remembers. The last real expanded width is the only honest answer
    /// while collapsed.
    private func currentSidebarGeometry() -> SidebarState? {
        guard let sidebarItem else { return nil }
        let width = sidebarItem.viewController.view.frame.width
        if !sidebarItem.isCollapsed && width > 1 { lastExpandedWidth = width }
        return SidebarState(isCollapsed: sidebarItem.isCollapsed, width: lastExpandedWidth)
    }

    private func applySidebar(_ state: SidebarState, animated: Bool) {
        guard let sidebarItem, let splitView else { return }
        let current = currentSidebarGeometry()
        guard current != state else { return }
        isApplyingSnapshot = true
        defer { isApplyingSnapshot = false }

        if animated {
            NSAnimationContext.runAnimationGroup { context in
                context.duration = 0.2
                sidebarItem.animator().isCollapsed = state.isCollapsed
            }
        } else {
            sidebarItem.isCollapsed = state.isCollapsed
        }
        if !state.isCollapsed, current?.width != state.width {
            lastExpandedWidth = state.width
            splitView.setPosition(state.width, ofDividerAt: 0)
        }
        // With the sidebar collapsed the content starts at the window's left
        // edge, under the traffic lights and the toggle button; the strip
        // clears them. Expanded, the sidebar itself is the clearance.
        tabStrip.leadingClearance = state.isCollapsed ? 116 : 0
    }

    private func dividerMoved() {
        guard !isApplyingSnapshot, !dividerReportScheduled else { return }
        dividerReportScheduled = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { [weak self] in
            guard let self else { return }
            self.dividerReportScheduled = false
            guard let state = self.currentSidebarGeometry(),
                  state != self.applied?.sidebar
            else { return }
            self.session.dispatch(.setSidebar(state))
        }
    }

    private func updateTitle(_ active: SessionSnapshot.ActiveTab) {
        let title = active.title.isEmpty ? "tab \(active.id.root)" : active.title
        if window?.title != title { window?.title = title }
    }
}

extension MainWindowController: SessionRendering {}
