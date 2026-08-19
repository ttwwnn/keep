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

/// One tab's pane arrangement, as a tree of nested split views.
///
/// One `NSSplitView` has one orientation, so a flat list of panes forces the
/// whole tab to share whichever direction the first split used — splitting
/// right and then down would silently give you two side-by-side panes. The
/// arrangement is a tree (see `PaneTree`), and it is built as one: each split
/// node becomes its own `NSSplitView` holding two children, which are either
/// surfaces or further splits.
@MainActor
final class TabHostView: NSView {
    let id: TabID
    private var tree: PaneTree?
    private var focusedPane: UInt32?
    private var splitViews: [NSSplitView] = []
    private var surfaces: [UInt32: TerminalSurfaceView] = [:]

    /// A pane surface took the keyboard; forwarded up to become a model fact.
    var onPaneFocus: ((UInt32) -> Void)?

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

    init(id: TabID) {
        self.id = id
        super.init(frame: .zero)
        addSubview(focusRing)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    deinit {
        NotificationCenter.default.removeObserver(self)
    }

    override func layout() {
        super.layout()
        subviews.first { $0 !== focusRing }?.frame = bounds
        positionFocusRing()
    }

    /// Rebuild the arrangement when its shape changed. Surfaces come from the
    /// pool, so re-parenting one costs nothing: its renderer and its client
    /// process are untouched by moving between split views.
    func apply(root: UInt32, panes: [PaneState]) {
        let next = PaneTree.build(root: root, panes: panes)
        guard next != tree else { return }
        tree = next

        for split in splitViews {
            NotificationCenter.default.removeObserver(
                self, name: NSSplitView.didResizeSubviewsNotification, object: split)
        }
        splitViews.removeAll()
        surfaces.removeAll()
        for view in subviews where view !== focusRing { view.removeFromSuperview() }

        let content = build(next)
        content.frame = bounds
        content.autoresizingMask = [.width, .height]
        addSubview(content, positioned: .below, relativeTo: focusRing)

        // Equalize after the tree is in the hierarchy and has a size.
        layoutSubtreeIfNeeded()
        for split in splitViews { equalize(split) }
        positionFocusRing()
    }

    private func build(_ node: PaneTree) -> NSView {
        switch node {
        case .leaf(let tab):
            let surface = SurfacePool.shared.surface(workspace: id.workspace, tab: tab)
            surface.onFocusGained = { [weak self] in self?.onPaneFocus?(tab) }
            surfaces[tab] = surface
            return surface

        case .split(let vertical, let first, let second):
            let split = NSSplitView()
            split.dividerStyle = .thin
            split.isVertical = vertical
            split.addArrangedSubview(build(first))
            split.addArrangedSubview(build(second))
            splitViews.append(split)
            // Dragging a divider resizes that split's own subviews, not this
            // host, so nothing here lays out and the focus ring would stay on
            // the pane's old edge. Every split in the tree reports.
            NotificationCenter.default.addObserver(
                forName: NSSplitView.didResizeSubviewsNotification,
                object: split,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.positionFocusRing() }
            }
            return split
        }
    }

    private func equalize(_ split: NSSplitView) {
        let total = split.isVertical ? split.bounds.width : split.bounds.height
        guard total > 0, split.arrangedSubviews.count == 2 else { return }
        split.setPosition(total / 2, ofDividerAt: 0)
    }

    /// Mark the pane holding the keyboard.
    func setFocusedPane(_ tab: UInt32) {
        guard focusedPane != tab else { return }
        focusedPane = tab
        positionFocusRing()
    }

    private func positionFocusRing() {
        guard let tree, tree.leaves.count > 1,
              let focusedPane, let surface = surfaces[focusedPane]
        else {
            focusRing.isHidden = true
            return
        }
        focusRing.layer?.borderColor = NSColor.controlAccentColor
            .withAlphaComponent(0.9).cgColor
        focusRing.frame = convert(surface.bounds, from: surface)
        focusRing.isHidden = false
    }

    func surface(for tab: UInt32) -> TerminalSurfaceView? {
        surfaces[tab]
    }

    var paneSurfaces: [TerminalSurfaceView] { Array(surfaces.values) }
}

/// The focus marker never takes a click: it sits over a terminal.
private final class FocusRingView: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}
