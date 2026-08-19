import AppKit

/// The stack of every visited tab's content, all mounted, exactly one
/// visible.
///
/// Mounting is hydration: the first time a tab is presented, its host is
/// built and its surfaces — each a renderer plus a `keep` client process —
/// are borrowed from the pool. From then on the tab's cost of appearing is
/// two `isHidden` flips. Nothing here ever touches a window.
@MainActor
final class TabContentContainer: NSView {
    private(set) var hosts: [TabID: TabHostView] = [:]
    private(set) var visibleTab: TabID?

    /// A pane took the keyboard. Wired by the controller into an intent, so
    /// which pane has focus stays a model fact rather than something the UI
    /// is asked for later.
    var onPaneFocus: ((UInt32) -> Void)?

    /// Host for a tab, made on first use. Added hidden: presentation order
    /// is the switch pipeline's business.
    func host(for tab: SessionSnapshot.ActiveTab) -> TabHostView {
        if let existing = hosts[tab.id] { return existing }
        let host = TabHostView(id: tab.id)
        host.onPaneFocus = { [weak self] pane in self?.onPaneFocus?(pane) }
        host.frame = bounds
        host.autoresizingMask = [.width, .height]
        host.isHidden = true
        addSubview(host)
        hosts[tab.id] = host
        Trace.log("mount", "\(tab.id) hosts=\(hosts.count)")
        return host
    }

    func hide(_ id: TabID) {
        hosts[id]?.isHidden = true
    }

    func markVisible(_ id: TabID) {
        visibleTab = id
    }

    /// A tab the daemon no longer has. The surfaces' lifetime is the pool's
    /// business; this only takes the host out of the hierarchy.
    func unmount(_ id: TabID) {
        guard let host = hosts.removeValue(forKey: id) else { return }
        host.removeFromSuperview()
        if visibleTab == id { visibleTab = nil }
        Trace.log("mount", "unmounted \(id) hosts=\(hosts.count)")
    }
}

/// One tab's pane arrangement: an NSSplitView of surfaces borrowed from the
/// pool. The pane logic is the old window controller's, kept: orientation
/// set by the first split, panes in daemon order, equalized on change.
@MainActor
final class TabHostView: NSView {
    let id: TabID
    private let paneSplit = NSSplitView()
    private var panes: [(tab: UInt32, surface: TerminalSurfaceView)] = []
    private var appliedPanes: [PaneState]?
    private var focusedPane: UInt32?

    /// Which pane has the keyboard, drawn only when there is more than one.
    /// A split with no visible focus leaves you guessing where the next
    /// keystroke — or the next split — is going to land.
    private let focusRing: NSView = {
        let ring = FocusRingView()
        ring.wantsLayer = true
        ring.layer?.borderWidth = 2
        ring.layer?.cornerRadius = 3
        ring.isHidden = true
        return ring
    }()

    /// A pane surface took the keyboard; forwarded up to become a model fact.
    var onPaneFocus: ((UInt32) -> Void)?

    init(id: TabID) {
        self.id = id
        super.init(frame: .zero)
        paneSplit.dividerStyle = .thin
        paneSplit.isVertical = true
        paneSplit.autoresizingMask = [.width, .height]
        addSubview(paneSplit)
        addSubview(focusRing)

        // Dragging a divider resizes the split's arranged subviews, not this
        // view, so nothing here lays out and the ring would stay behind on
        // the pane's old edge. The split says when its panes moved.
        NotificationCenter.default.addObserver(
            forName: NSSplitView.didResizeSubviewsNotification,
            object: paneSplit,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.positionFocusRing() }
        }
    }

    deinit {
        NotificationCenter.default.removeObserver(
            self, name: NSSplitView.didResizeSubviewsNotification, object: paneSplit)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    override func layout() {
        super.layout()
        paneSplit.frame = bounds
        positionFocusRing()
    }

    /// Mark the pane holding the keyboard.
    func setFocusedPane(_ tab: UInt32) {
        guard focusedPane != tab else { return }
        focusedPane = tab
        positionFocusRing()
    }

    private func positionFocusRing() {
        guard panes.count > 1, let focusedPane,
              let surface = surface(for: focusedPane)
        else {
            focusRing.isHidden = true
            return
        }
        focusRing.layer?.borderColor = NSColor.controlAccentColor
            .withAlphaComponent(0.9).cgColor
        focusRing.frame = convert(surface.bounds, from: surface)
        focusRing.isHidden = false
    }

    /// Bring the split in line with the daemon's arrangement. The root pane
    /// is implicit; `wanted` is everything beyond it, in daemon order.
    func apply(panes wanted: [PaneState]) {
        guard wanted != appliedPanes else { return }
        appliedPanes = wanted

        // The first split orients the whole arrangement (2 = down).
        if panes.count <= 1, let first = wanted.first {
            paneSplit.isVertical = first.splitDir != 2
        }

        let keep = Set([id.root] + wanted.map(\.tab))
        for pane in panes where !keep.contains(pane.tab) {
            pane.surface.removeFromSuperview()
        }
        panes.removeAll { !keep.contains($0.tab) }

        let order = [id.root] + wanted.map(\.tab)
        for tab in order where !panes.contains(where: { $0.tab == tab }) {
            let surface = SurfacePool.shared.surface(workspace: id.workspace, tab: tab)
            surface.onFocusGained = { [weak self] in self?.onPaneFocus?(tab) }
            panes.append((tab, surface))
        }
        panes.sort { (order.firstIndex(of: $0.tab) ?? 0) < (order.firstIndex(of: $1.tab) ?? 0) }
        needsLayout = true

        // Rebuild the arranged list when it disagrees — the split's visual
        // order must match the daemon's, not just our array's.
        let arranged = paneSplit.arrangedSubviews
        let desired = panes.map(\.surface)
        if arranged.count != desired.count || !zip(arranged, desired).allSatisfy({ $0 === $1 }) {
            for view in arranged { paneSplit.removeArrangedSubview(view) }
            for surface in desired { paneSplit.addArrangedSubview(surface) }
            equalizePanes()
        }
    }

    private func equalizePanes() {
        guard panes.count > 1 else { return }
        paneSplit.layoutSubtreeIfNeeded()
        let total = paneSplit.isVertical ? paneSplit.bounds.width : paneSplit.bounds.height
        guard total > 0 else { return }
        for index in 1..<panes.count {
            paneSplit.setPosition(
                total * CGFloat(index) / CGFloat(panes.count), ofDividerAt: index - 1)
        }
    }

    func surface(for tab: UInt32) -> TerminalSurfaceView? {
        panes.first { $0.tab == tab }?.surface
    }

    var paneSurfaces: [TerminalSurfaceView] { panes.map(\.surface) }
}


/// The focus marker never takes a click: it sits over a terminal.
private final class FocusRingView: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}
