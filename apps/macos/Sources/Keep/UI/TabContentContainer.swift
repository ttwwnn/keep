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
    var onPaneDrop: ((TabID, UInt32, UInt32, Intent.DropSide) -> Void)?
    var onPaneDetach: ((TabID, UInt32) -> Void)?
    /// Where the pointer is while a pane is being carried, in window
    /// coordinates, and nil when it is put down. The controller answers this
    /// by springing open whatever tab is hovered.
    var onCarryOver: ((NSPoint?) -> Void)?
    /// Which tab the pointer is over in the strip, asked of the controller
    /// because the strip is not this view's to know about.
    var tabUnderPointer: ((NSPoint) -> TabID?)?

    /// Host for a tab, made on first use. Added hidden: presentation order
    /// is the switch pipeline's business.
    func host(for tab: SessionSnapshot.ActiveTab) -> TabHostView {
        if let existing = hosts[tab.id] { return existing }
        let host = TabHostView(id: tab.id)
        host.onPaneFocus = { [weak self] id, pane in self?.onPaneFocus?(id, pane) }
        host.onPaneCarry = { [weak self] id, pane, event in
            self?.carry(from: id, pane: pane, beginning: event)
        }
        host.frame = bounds
        host.autoresizingMask = [.width, .height]
        host.isHidden = true
        addSubview(host)
        hosts[tab.id] = host
        Trace.log("mount", "\(tab.id) hosts=\(hosts.count)")
        return host
    }

    // MARK: - carrying a pane between tabs

    /// Run a pane's drag to its end.
    ///
    /// The drag is tracked here rather than by the view that started it,
    /// because springing a tab open mid-drag hides that view — and a view
    /// that is hidden has no business still steering. A nested event loop
    /// belongs to the window, so it survives whatever happens underneath.
    private func carry(from source: TabID, pane: UInt32, beginning event: NSEvent) {
        guard let window else { return }
        window.trackEvents(
            matching: [.leftMouseDragged, .leftMouseUp],
            timeout: .infinity,
            mode: .eventTracking
        ) { [weak self] event, stop in
            guard let self, let event else {
                stop.pointee = true
                return
            }
            switch event.type {
            case .leftMouseDragged:
                self.carried(source: source, pane: pane, to: event.locationInWindow)
            case .leftMouseUp:
                self.dropped(source: source, pane: pane, at: event.locationInWindow)
                stop.pointee = true
            default:
                break
            }
        }
    }

    private func carried(source: TabID, pane: UInt32, to windowPoint: NSPoint) {
        onCarryOver?(windowPoint)
        let point = convert(windowPoint, from: nil)
        for host in hosts.values where host.id != visibleTab { host.hideLanding() }
        guard bounds.contains(point), let id = visibleTab, let host = hosts[id] else {
            hosts[visibleTab ?? source]?.hideLanding()
            return
        }
        // Only a pane of the tab you can see is a place to land, and a pane
        // cannot land on itself — but once the drag has crossed into another
        // tab, every pane there is somewhere it could go.
        host.showLanding(at: host.convert(point, from: self), carrying: id == source ? pane : nil)
    }

    private func dropped(source: TabID, pane: UInt32, at windowPoint: NSPoint) {
        onCarryOver?(nil)
        let point = convert(windowPoint, from: nil)
        for host in hosts.values { host.hideLanding() }

        guard bounds.contains(point) else {
            // Let go over a tab in the strip: put the pane in that tab, beside
            // what is already there. Anywhere else outside is a tab of its own.
            if let over = tabUnderPointer?(windowPoint), over != source,
               let host = hosts[over], let anchor = host.anchorPane {
                onPaneDrop?(source, pane, anchor, .right)
            } else {
                onPaneDetach?(source, pane)
            }
            return
        }
        guard let id = visibleTab, let host = hosts[id] else { return }
        guard let (target, side) = host.showLanding(
            at: host.convert(point, from: self), carrying: id == source ? pane : nil)
        else { return }
        host.hideLanding()
        onPaneDrop?(source, pane, target, side)
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
    /// Somebody picked a pane up. The drag itself belongs to the container:
    /// it can cross into another tab, and this view is hidden the moment it
    /// does.
    var onPaneCarry: ((TabID, UInt32, NSEvent) -> Void)?

    /// The paint that says where a carried pane would land.
    private lazy var landing: NSView = {
        let view = LandingView(frame: .zero)
        view.wantsLayer = true
        view.layer?.cornerRadius = 4
        view.layer?.cornerCurve = .continuous
        view.isHidden = true
        addSubview(view)
        return view
    }()

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
            surface.onGripEvent = { [weak self] event, phase in
                guard let self, phase == .began else { return }
                self.onPaneCarry?(self.id, tab, event)
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
            // Nowhere to go, no handle: a lone pane cannot be rearranged, and
            // taking it out of a tab it is the whole of does nothing.
            surface.isDraggable = many
        }
    }

    // MARK: - carrying a pane

    /// Paint where a pane would land, and say where that is.
    ///
    /// `carrying` names the pane being moved when it belongs to this tab, so
    /// it is not offered as its own destination. Dragging in from another tab
    /// passes nil: every pane here is somewhere it could go.
    @discardableResult
    func showLanding(at point: NSPoint, carrying pane: UInt32?) -> (UInt32, Intent.DropSide)? {
        addSubview(landing, positioned: .above, relativeTo: nil)
        guard let (target, side) = drop(at: point, carrying: pane),
              let other = surfaces.first(where: { $0.value === target })?.key
        else {
            landing.isHidden = true
            return nil
        }
        landing.layer?.backgroundColor = NSColor.controlAccentColor
            .withAlphaComponent(0.28).cgColor
        landing.frame = paint(side, over: target)
        landing.isHidden = false
        return (other, side)
    }

    func hideLanding() {
        landing.isHidden = true
    }

    /// Which pane is under the point, and which of its sides was aimed at.
    ///
    /// Nil when the point is over the pane being carried, or over nothing:
    /// dropping a pane on itself is not a move, and neither is dropping it on
    /// a divider.
    private func drop(
        at point: NSPoint, carrying pane: UInt32?
    ) -> (TerminalSurfaceView, Intent.DropSide)? {
        for (id, surface) in surfaces where id != pane {
            let local = surface.convert(point, from: self)
            guard surface.bounds.contains(local) else { continue }
            let edge: CGFloat = 0.3
            let x = local.x / max(1, surface.bounds.width)
            let y = local.y / max(1, surface.bounds.height)
            let side: Intent.DropSide
            if x < edge {
                side = .left
            } else if x > 1 - edge {
                side = .right
            } else if y < edge {
                // Not flipped: the origin is the bottom.
                side = .bottom
            } else if y > 1 - edge {
                side = .top
            } else {
                side = .onto
            }
            return (surface, side)
        }
        return nil
    }

    /// The half of the target the pane would take, or all of it for a trade.
    private func paint(_ side: Intent.DropSide, over target: NSView) -> NSRect {
        let frame = convert(target.bounds, from: target)
        switch side {
        case .onto: return frame.insetBy(dx: 2, dy: 2)
        case .left: return NSRect(
            x: frame.minX, y: frame.minY, width: frame.width / 2, height: frame.height)
        case .right: return NSRect(
            x: frame.midX, y: frame.minY, width: frame.width / 2, height: frame.height)
        case .top: return NSRect(
            x: frame.minX, y: frame.midY, width: frame.width, height: frame.height / 2)
        case .bottom: return NSRect(
            x: frame.minX, y: frame.minY, width: frame.width, height: frame.height / 2)
        }
    }

    /// A pane to hang something off when a drop names the tab and not a
    /// place in it: the one with the keyboard, or the root.
    var anchorPane: UInt32? {
        if let focusedPane, surfaces[focusedPane] != nil { return focusedPane }
        return surfaces.keys.contains(id.root) ? id.root : surfaces.keys.first
    }

    func surface(for tab: UInt32) -> TerminalSurfaceView? {
        surfaces[tab]
    }

    var paneSurfaces: [TerminalSurfaceView] { Array(surfaces.values) }
}

/// The landing paint never takes a click: it sits over terminals.
private final class LandingView: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}
