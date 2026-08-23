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
    /// Which window this container is in. Carried so the surfaces it mounts
    /// are borrowed in this window's name and not another's.
    var windowID: WindowID = .first

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

    /// What is being carried, shown under the pointer while it is.
    private var card: PaneCard?

    /// Host for a tab, made on first use. Added hidden: presentation order
    /// is the switch pipeline's business.
    func host(for tab: SessionSnapshot.ActiveTab) -> TabHostView {
        if let existing = hosts[tab.id] { return existing }
        let host = TabHostView(id: tab.id, window: windowID)
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
        let start = event.locationInWindow
        var carrying = false
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
                // A press on the handle is not yet a move. Until the pointer
                // has gone somewhere, nothing is picked up and nothing is
                // painted — otherwise a click on the dots would flash a card
                // and a landing strip at somebody who only clicked.
                let travelled = hypot(
                    event.locationInWindow.x - start.x, event.locationInWindow.y - start.y)
                if !carrying {
                    guard travelled > 4 else { return }
                    carrying = true
                    self.card = PaneCard(workspace: source.workspace, pane: pane)
                }
                self.card?.follow(window.convertPoint(toScreen: event.locationInWindow))
                self.carried(source: source, pane: pane, to: event.locationInWindow)
            case .leftMouseUp:
                if carrying {
                    self.dropped(source: source, pane: pane, at: event.locationInWindow)
                }
                self.card?.close()
                self.card = nil
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

        // Where the pointer actually is, in screen terms: `point` only says it
        // is outside this view, which is equally true of the tab strip above
        // it and of somebody else's screen.
        let leftTheWindow = window.map { !$0.frame.contains(NSEvent.mouseLocation) } ?? false

        guard bounds.contains(point) else {
            // Let go over a tab in the strip: put the pane in that tab, beside
            // what is already there. Anywhere else outside is a tab of its own.
            if let over = tabUnderPointer?(windowPoint), over != source,
               let host = hosts[over], let anchor = host.anchorPane {
                onPaneDrop?(source, pane, anchor, .right)
            } else if leftTheWindow {
                // Dropped on another window. The tracking loop belongs to the
                // window the press started in, so there is no way from here to
                // hand the pane to the window under the pointer — and making a
                // new tab in *this* window instead is the worst answer: the
                // pane vanishes from where it was dropped and reappears
                // somewhere nobody was looking. Until a drop can cross, a drag
                // that leaves is a drag that did not happen.
                Trace.log("pane", "drag left the window; nothing moved")
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
    /// The window this host hangs in, for borrowing surfaces.
    let windowID: WindowID
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

    /// Which way to look for a neighbouring pane.
    enum Direction { case left, right, up, down }

    /// Give the keyboard to the pane beside the one that has it.
    ///
    /// Answered from where the panes are on screen rather than from where
    /// they sit in the tree. The two are not the same question: a pane is the
    /// left child of some split, but what is *to the left of it* may be two
    /// levels up the tree and one across, and the answer changes as splits
    /// nest. Geometry is asked the question that was actually asked.
    @discardableResult
    func moveFocus(_ direction: Direction) -> Bool {
        guard let from = focusedPane, let next = pane(beside: from, going: direction),
              let view = surfaces[next]
        else { return false }
        // Becoming first responder is what tells the model, through the
        // surface's own `onFocusGained` — so nothing is announced twice.
        window?.makeFirstResponder(view)
        return true
    }

    /// Move the divider the focused pane sits against.
    ///
    /// A pane can be inside several splits at once — that is what nesting
    /// them means — and only one of those has a divider that runs the way
    /// this asks. So the tree is walked outward from the pane until a split
    /// facing the right way is found, and it is that one's divider that
    /// moves. Nothing is asked of the daemon: how a tab is divided on screen
    /// is the window's business, and the daemon holds the shells.
    @discardableResult
    func resizeSplit(_ direction: Direction, by amount: CGFloat) -> Bool {
        guard let pane = focusedPane, let view = surfaces[pane] else { return false }
        let wantsVertical = direction == .left || direction == .right
        var node: NSView = view
        while let parent = node.superview {
            defer { node = parent }
            guard let split = parent as? NSSplitView,
                  split.isVertical == wantsVertical,
                  split.arrangedSubviews.count == 2
            else { continue }
            let total = split.isVertical ? split.bounds.width : split.bounds.height
            guard total > 0 else { return false }
            // The divider's position is the near side's extent: the width of
            // the left pane, or the height of the top one.
            let near = split.arrangedSubviews[0]
            let position = split.isVertical ? near.frame.width : near.frame.height
            let step = (direction == .right || direction == .down) ? amount : -amount
            split.setPosition(min(max(0, position + step), total), ofDividerAt: 0)
            return true
        }
        return false
    }

    private func pane(beside origin: UInt32, going direction: Direction) -> UInt32? {
        guard let from = surfaces[origin] else { return nil }
        let source = from.convert(from.bounds, to: self)
        var best: (pane: UInt32, gap: CGFloat, offset: CGFloat)?
        for (id, view) in surfaces where id != origin {
            let frame = view.convert(view.bounds, to: self)
            let gap: CGFloat
            let alongside: Bool
            // AppKit's y grows upward, so "down" is toward smaller y.
            switch direction {
            case .left:
                gap = source.minX - frame.maxX
                alongside = frame.minY < source.maxY && frame.maxY > source.minY
            case .right:
                gap = frame.minX - source.maxX
                alongside = frame.minY < source.maxY && frame.maxY > source.minY
            case .down:
                gap = source.minY - frame.maxY
                alongside = frame.minX < source.maxX && frame.maxX > source.minX
            case .up:
                gap = frame.minY - source.maxY
                alongside = frame.minX < source.maxX && frame.maxX > source.minX
            }
            // A divider is a point or two wide, so touching counts as beside.
            guard alongside, gap >= -1 else { continue }
            let offset = abs(frame.midX - source.midX) + abs(frame.midY - source.midY)
            let better = best.map { gap < $0.gap || (gap == $0.gap && offset < $0.offset) } ?? true
            if better { best = (id, gap, offset) }
        }
        return best?.pane
    }

    init(id: TabID, window: WindowID) {
        self.id = id
        self.windowID = window
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
            let surface = SurfacePool.shared.surface(
                window: windowID, workspace: id.workspace, tab: tab)
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

/// The pane you are carrying, as a card under the pointer.
///
/// It shows the pane's text rather than a picture of it. A terminal is drawn
/// by Metal, and a Metal layer cannot be read back into an image — the only
/// way to photograph one is to photograph the screen, which would put this
/// app behind a screen-recording prompt for the sake of a drag. The daemon
/// already holds every tab's text, so the card asks it.
@MainActor
final class PaneCard {
    private let window: NSWindow
    private static let size = NSSize(width: 300, height: 190)

    init(workspace: String, pane: UInt32) {
        let background = GhosttyApp.shared.terminalBackground ?? .black
        let text = (try? Daemon.preview(workspace: workspace, tab: pane)) ?? ""
        // The screen, not the history: a card is a reminder of what you
        // picked up, and the last lines are what you were looking at.
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false).suffix(26)

        let body = NSTextField(labelWithString: lines.joined(separator: "\n"))
        body.font = GhosttyApp.shared.terminalFont(size: 7)
        body.textColor = NSColor.white.withAlphaComponent(0.75)
        body.maximumNumberOfLines = 0
        body.lineBreakMode = .byClipping
        body.translatesAutoresizingMaskIntoConstraints = false

        let card = NSView(frame: NSRect(origin: .zero, size: Self.size))
        card.wantsLayer = true
        card.layer?.backgroundColor = background.cgColor
        card.layer?.cornerRadius = 8
        card.layer?.cornerCurve = .continuous
        card.layer?.borderWidth = 1
        card.layer?.borderColor = NSColor.white.withAlphaComponent(0.18).cgColor
        card.layer?.masksToBounds = true
        card.addSubview(body)
        NSLayoutConstraint.activate([
            body.leadingAnchor.constraint(equalTo: card.leadingAnchor, constant: 8),
            body.trailingAnchor.constraint(lessThanOrEqualTo: card.trailingAnchor, constant: -8),
            body.topAnchor.constraint(equalTo: card.topAnchor, constant: 8),
        ])

        window = NSWindow(
            contentRect: NSRect(origin: .zero, size: Self.size),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false)
        window.contentView = card
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = true
        window.level = .floating
        // It follows the pointer; it must never be in the way of it.
        window.ignoresMouseEvents = true
        window.alphaValue = 0.92
        window.orderFront(nil)
    }

    /// Held below and right of the pointer, out from under it, the way a
    /// dragged thing hangs off the hand carrying it.
    func follow(_ screenPoint: NSPoint) {
        window.setFrameOrigin(NSPoint(
            x: (screenPoint.x - 24).rounded(),
            y: (screenPoint.y - Self.size.height + 16).rounded()))
    }

    func close() {
        window.orderOut(nil)
    }
}
