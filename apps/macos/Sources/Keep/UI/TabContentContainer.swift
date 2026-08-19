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
    var onPaneFocus: ((TabID, UInt32) -> Void)?

    /// Host for a tab, made on first use. Added hidden: presentation order
    /// is the switch pipeline's business.
    func host(for tab: SessionSnapshot.ActiveTab) -> TabHostView {
        if let existing = hosts[tab.id] { return existing }
        let host = TabHostView(id: tab.id)
        host.onPaneFocus = { [weak self] id, pane in self?.onPaneFocus?(id, pane) }
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
    /// A pane surface took the keyboard; forwarded up with this tab's identity
    /// so a report from a hidden tab cannot be mistaken for the active one's.
    var onPaneFocus: ((TabID, UInt32) -> Void)?

    init(id: TabID) {
        self.id = id
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    override func layout() {
        super.layout()
        subviews.first?.frame = bounds
    }

    /// Rebuild the arrangement when its shape changed. Surfaces come from the
    /// pool, so re-parenting one costs nothing: its renderer and its client
    /// process are untouched by moving between split views.
    ///
    /// Returns whether it rebuilt. A rebuild takes the focused surface out of
    /// the responder chain, and AppKit answers that by handing the keyboard
    /// to something of its own choosing — so the caller has to put it back.
    @discardableResult
    func apply(root: UInt32, panes: [PaneState]) -> Bool {
        let next = PaneTree.build(root: root, panes: panes)
        guard next != tree else { return false }
        tree = next

        splitViews.removeAll()
        surfaces.removeAll()
        for view in subviews { view.removeFromSuperview() }

        let content = build(next)
        content.frame = bounds
        content.autoresizingMask = [.width, .height]
        addSubview(content)

        // Outermost first, laying out between levels: a nested split has no
        // size of its own until its parent has given it one, and halving zero
        // is how a pane ends up with no height at all.
        layoutSubtreeIfNeeded()
        for split in splitViews {
            equalize(split)
            split.layoutSubtreeIfNeeded()
        }
        applyResting()
        return true
    }

    private func build(_ node: PaneTree) -> NSView {
        switch node {
        case .leaf(let tab):
            let surface = SurfacePool.shared.surface(workspace: id.workspace, tab: tab)
            surface.onFocusGained = { [weak self] in
                guard let self else { return }
                self.onPaneFocus?(self.id, tab)
            }
            surfaces[tab] = surface
            return surface

        case .split(let vertical, let first, let second):
            let split = NSSplitView()
            split.dividerStyle = .thin
            split.isVertical = vertical
            // Recorded before its children, so `splitViews` runs outermost
            // first — the order the equalizing pass needs.
            splitViews.append(split)
            split.addArrangedSubview(build(first))
            split.addArrangedSubview(build(second))
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
        applyResting()
    }

    /// Step every pane but the one you are in back a little.
    ///
    /// A lone pane is never dimmed: with nothing to tell it apart from, dim
    /// would only mean the window is not in front, which the window says for
    /// itself.
    private func applyResting() {
        let many = (tree?.leaves.count ?? 0) > 1
        for (pane, surface) in surfaces {
            surface.isResting = many && pane != focusedPane
        }
    }

    func surface(for tab: UInt32) -> TerminalSurfaceView? {
        surfaces[tab]
    }

    var paneSurfaces: [TerminalSurfaceView] { Array(surfaces.values) }
}
