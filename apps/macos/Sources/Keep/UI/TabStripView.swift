import AppKit

/// The tab bar, drawn to match Ghostty's.
///
/// The shape of it: the selected tab is a rounded fill sitting inside the
/// titlebar row; the others are bare text on the chrome, with no separators
/// between them. Each carries its ⌘-number on the right, so the shortcut is
/// learned by being seen rather than by being looked up.
///
/// It is the app's own view rather than AppKit's tab bar because AppKit's is
/// not a bar — it is a consequence of one window per tab, and windows coming
/// and going is the bug class this shell was rebuilt to remove.
@MainActor
final class TabStripView: NSView {
    var onSelect: ((TabID) -> Void)?
    var onClose: ((TabID) -> Void)?
    var onNewTab: (() -> Void)?
    /// The row, in the order somebody just put it in.
    var onReorder: (([UInt32]) -> Void)?
    /// Told when this row's leading edge moves.
    ///
    /// Which is the sidebar's trailing edge, since the row begins where the
    /// content does. Reported from here because this is the one place that
    /// already hears about it at every frame of the sidebar's animation —
    /// a laid-out view is told its new geometry; a view watching a
    /// notification is told a story about it afterwards.
    var onLeadingEdgeMoved: ((CGFloat) -> Void)?
    private var lastLeadingEdge: CGFloat?

    /// Where the window's own chrome ends, in the window's coordinates.
    ///
    /// The toggle sits at 92 and is 26 across, so it ends at 118 — measured,
    /// not guessed. Ten points further on, a tab's capsule (inset two from its
    /// cell) starts twelve points clear of it, which is exactly the gap the
    /// new-tab button keeps from the last tab at the other end. The capsule is
    /// what the eye measures from, not the close button inside it, so it is
    /// the capsule the two ends are matched on.
    private static let chromeWidth: CGFloat = 128

    /// Points kept free at the leading edge for the chrome that overlaps this
    /// row — traffic lights, sidebar toggle.
    ///
    /// Measured from where this row actually begins rather than announced by
    /// whoever last toggled the sidebar. The two agree once things have
    /// settled and they do not agree while the sidebar is moving: the row is
    /// handed its new width over two tenths of a second, and a clearance
    /// flipped in a single instant is wrong for every frame in between. Told,
    /// it went to zero the moment the sidebar began to open — while the row
    /// was still the full width of the window — and the tabs spread out under
    /// the traffic lights and the toggle before sliding back. Measured, the
    /// row simply starts wherever the chrome has stopped, at every frame,
    /// because the chrome is not going anywhere and this row is.
    private var leadingClearance: CGFloat {
        guard window != nil else { return 0 }
        return max(0, Self.chromeWidth - convert(NSPoint.zero, to: nil).x)
    }

    private var items: [SessionSnapshot.StripItem] = []
    private var cells: [TabCellView] = []
    private let newTabButton = NSButton()
    /// The button's ground: glass where the system has it.
    private let newTabBackground: NSView = Glass.lozenge(cornerRadius: 12) ?? NSView()
    private let newTabIsGlass = Glass.isAvailable
    private var newTabHovered = false
    private var backgroundObserver: NSObjectProtocol?

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true

        newTabBackground.wantsLayer = true
        addSubview(newTabBackground)

        newTabButton.image = NSImage(
            systemSymbolName: "plus", accessibilityDescription: "New Tab")
        newTabButton.bezelStyle = .accessoryBarAction
        newTabButton.isBordered = false
        newTabButton.imagePosition = .imageOnly
        newTabButton.target = self
        newTabButton.action = #selector(newTabPressed)
        newTabButton.setAccessibilityLabel("New Tab")
        addSubview(newTabButton)

        backgroundObserver = NotificationCenter.default.addObserver(
            forName: GhosttyApp.backgroundDidChange, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.retint() }
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    deinit {
        if let backgroundObserver {
            NotificationCenter.default.removeObserver(backgroundObserver)
        }
    }

    /// Which tab is at a point in this view, if any. Asked while a pane is
    /// being carried, to know which tab to spring open.
    func tab(at point: NSPoint) -> TabID? {
        guard bounds.contains(point) else { return nil }
        for (index, cell) in cells.enumerated() where cell.frame.contains(point) {
            return items[index].id
        }
        return nil
    }

    /// Empty regions stay draggable titlebar, like the tint backdrops.
    override func hitTest(_ point: NSPoint) -> NSView? {
        let hit = super.hitTest(point)
        return hit === self ? nil : hit
    }

    /// Whether the window may be dragged by its titlebar right now, decided
    /// by what the pointer is over.
    ///
    /// The window server runs a titlebar drag itself, without waking the app,
    /// and the titlebar is a handle unconditionally: `mouseDownCanMoveWindow`
    /// is not consulted there, and neither the row nor a cell can refuse on
    /// its own behalf. What the window server does honour is `isMovable` —
    /// but only the answer it already has. Refusing from `mouseDown` is too
    /// late by then: the drag is underway, the window is travelling with the
    /// pointer, and a pointer that keeps its place *within* the window looks
    /// to this row exactly like a finger that never moved, which is why no
    /// tab would change place however far it was dragged.
    ///
    /// So the answer is given in advance, on hover. Over a tab the window
    /// holds still and the drag is the tab's; over the bare stretches of the
    /// row it is a titlebar again, which is what those stretches are for.
    private func updateWindowDragging(pointerAt point: NSPoint) {
        // A lone tab is not a tab, it is the window's title — drawn without a
        // capsule for exactly that reason — and there is nowhere to reorder it
        // to. Holding the window still under it would take the title bar away
        // from a window whose title bar is all this row is.
        guard cells.count > 1 else {
            setWindowDraggable(true)
            return
        }
        setWindowDraggable(!cells.contains { $0.frame.contains(point) })
    }

    private func setWindowDraggable(_ draggable: Bool) {
        guard let window, window.isMovable != draggable else { return }
        window.isMovable = draggable
        Trace.log("strip", "window is \(draggable ? "draggable" : "held still")")
    }

    func apply(_ newItems: [SessionSnapshot.StripItem]) {
        guard newItems != items else { return }
        items = newItems

        // Reuse cells in place; a poll that only changes a title touches text.
        while cells.count > items.count {
            cells.removeLast().removeFromSuperview()
        }
        while cells.count < items.count {
            let cell = TabCellView()
            cell.onSelect = { [weak self] id in self?.onSelect?(id) }
            cell.onPress = { [weak self] cell, event in self?.carry(cell, from: event) }
            cell.onClose = { [weak self] id in self?.onClose?(id) }
            addSubview(cell)
            cells.append(cell)
        }
        applyCells()
        needsLayout = true
    }

    private func retint() {
        applyCells()
        newTabButton.contentTintColor = Palette.current.dimText
        if newTabIsGlass {
            tintNewTabButton()
        } else {
            newTabBackground.layer?.backgroundColor = Palette.current.controlFill.cgColor
        }
    }

    private func applyCells() {
        let palette = Palette.current
        for (index, cell) in cells.enumerated() {
            cell.apply(
                items[index],
                palette: palette,
                shortcut: items.count == 1 ? nil : Self.shortcut(
                    index: index, count: items.count),
                alone: items.count == 1)
        }
    }

    /// ⌘1-8 by position and ⌘9 for the last, which is the rule the Window
    /// menu uses and the convention macOS trained.
    private static func shortcut(index: Int, count: Int) -> String? {
        if index == count - 1 && count >= 9 { return "⌘9" }
        return index < 8 ? "⌘\(index + 1)" : nil
    }

    override func layout() {
        super.layout()
        let height = bounds.height
        // A circle with a plus in it, not a bare glyph: it reads as a
        // control, which is what it is. The same across as a tab's capsule is
        // tall, and as the sidebar toggle.
        let plusSide: CGFloat = 26
        let plusWidth = plusSide + 12
        let plusRect = NSRect(
            x: bounds.width - plusSide - 10, y: (height - plusSide) / 2,
            width: plusSide, height: plusSide)
        newTabButton.frame = plusRect
        newTabBackground.frame = plusRect
        if newTabIsGlass {
            Glass.setCornerRadius(newTabBackground, plusSide / 2)
            tintNewTabButton()
        } else {
            newTabBackground.layer?.cornerRadius = plusSide / 2
            newTabBackground.layer?.backgroundColor = Palette.current.controlFill.cgColor
        }

        if window != nil {
            let edge = convert(NSPoint.zero, to: nil).x
            // Remembered only once it has actually been said to somebody.
            //
            // This row lays out before the window is wired up, so the first
            // edge it measures has no one to tell. Recording it anyway meant
            // recording that it had been reported, and every later layout
            // measured the same edge, found it unchanged, and stayed quiet —
            // so a window opened with the sidebar out kept its toggle beside
            // the traffic lights until something moved the sidebar.
            if edge != lastLeadingEdge, let onLeadingEdgeMoved {
                lastLeadingEdge = edge
                onLeadingEdgeMoved(edge)
            }
        }

        guard !cells.isEmpty else { return }
        let left = leadingClearance
        let available = max(0, bounds.width - left - plusWidth - 8)
        // Tabs fill the row rather than sitting in a corner of it, and they
        // share what there is rather than insisting on a width. A floor here
        // was a promise the row could not keep: past the point where the
        // floor times the count exceeded the space, the last tabs ran under
        // the new-tab button and off the end of the strip. Tabs give ground
        // instead, and a cell narrow enough drops what it cannot show.
        let width = available / CGFloat(cells.count)
        // The row's arithmetic, said once each time it changes. Where the
        // tabs are is the first thing anybody asks when a press lands on the
        // wrong one, and it is not otherwise recoverable from outside.
        let shape = "row clear=\(Int(left)) slot=\(Int(width)) tabs=\(cells.count)"
        if shape != lastShape {
            lastShape = shape
            Trace.log("strip", shape)
        }
        var x = left
        for cell in cells {
            // Whole pixels: a fill edge on a half pixel renders soft.
            let next = (x + width).rounded()
            let place = NSRect(x: x.rounded(), y: 0, width: next - x.rounded(), height: height)
            // The one being carried follows the pointer, not the row. Its
            // width still comes from here, so the row it is being dropped
            // into is the row it will belong to.
            if cell === carried {
                cell.frame.size = place.size
            } else if animating {
                cell.animator().frame = place
            } else {
                cell.frame = place
            }
            // A lone title is the window's title and belongs on the window's
            // centre, not on the centre of what is left after the chrome.
            cell.titleOffset = cells.count == 1 ? bounds.midX - cell.frame.midX : 0
            x = next
        }
    }

    // MARK: - carrying a tab along the row

    /// The tab under the pointer, while it is being moved.
    private var carried: TabCellView?
    /// Whether the others should slide to their new places rather than jump.
    private var animating = false
    /// The last row geometry traced, so a relayout that changes nothing is
    /// not worth a line.
    private var lastShape = ""

    /// Run a tab's drag to its end.
    ///
    /// A press on a tab is not yet a move: below the threshold it is the click
    /// that selects, and a row that rearranged itself every time somebody
    /// picked a tab would be unusable. Past it, the tab follows the pointer
    /// and the rest of the row opens a place for it.
    func carry(_ cell: TabCellView, from event: NSEvent) {
        guard let item = cell.tabID else { return }
        // Nothing to rearrange, or nowhere to run a drag: the press is a
        // click and must still select. A tab that stops selecting because the
        // code that moves tabs bailed out early is worse than one that cannot
        // be moved.
        guard let window, cells.count > 1 else {
            onSelect?(item)
            return
        }
        // Hovering the tab said this already. Said again because a press that
        // arrives without one — the app activated by this very click, the
        // pointer never having moved since — would otherwise leave the window
        // movable for the whole drag. Too late for this press, in time for
        // the next.
        setWindowDraggable(false)

        let start = convert(event.locationInWindow, from: nil)
        let originX = cell.frame.minX
        var moved = false
        var sawDrag = 0
        Trace.log("strip", "press on \(item.root) at \(Int(start.x))")

        window.trackEvents(
            matching: [.leftMouseDragged, .leftMouseUp],
            timeout: .infinity,
            mode: .eventTracking
        ) { [weak self] event, stop in
            guard let self, let event else {
                stop.pointee = true
                return
            }
            let point = self.convert(event.locationInWindow, from: nil)
            switch event.type {
            case .leftMouseDragged:
                sawDrag += 1
                if !moved {
                    guard abs(point.x - start.x) > 4 else { return }
                    moved = true
                    Trace.log("strip", "carrying \(item.root)")
                    self.carried = cell
                    // Above the others, so it passes over them rather than
                    // through them.
                    self.addSubview(cell, positioned: .above, relativeTo: nil)
                }
                cell.frame.origin.x = originX + (point.x - start.x)
                self.settle(cell)

            case .leftMouseUp:
                defer { stop.pointee = true }
                self.carried = nil
                if moved {
                    self.needsLayout = true
                    let order = self.cells.compactMap(\.tabID?.root)
                    Trace.log("strip", "dropped, order now \(order)")
                    self.onReorder?(order)
                } else {
                    Trace.log("strip", "released after \(sawDrag) drag events; treated as a click")
                    self.onSelect?(item)
                }

            default:
                break
            }
        }
        // `trackEvents` returns only once the drag is over, however it ended.
        // Where the pointer came to rest decides, not where it started: a tab
        // dropped under the pointer should still be holding the window.
        updateWindowDragging(
            pointerAt: convert(window.mouseLocationOutsideOfEventStream, from: nil))
    }

    /// Move the carried tab into the place its middle is over, and let the
    /// others slide.
    ///
    /// The place, not a fixed distance. A tab is as wide as the row allows,
    /// which is a third of a wide window and a ninth of a narrow one, so any
    /// number of points chosen here would be several places in one window and
    /// a fraction of one in the next.
    private func settle(_ cell: TabCellView) {
        guard let at = cells.firstIndex(of: cell) else { return }
        let width = cell.frame.width
        guard width > 0 else { return }
        let target = min(
            cells.count - 1,
            max(0, Int(((cell.frame.midX - leadingClearance) / width).rounded(.down))))
        guard target != at else { return }
        cells.remove(at: at)
        cells.insert(cell, at: target)
        Trace.log("strip", "moved to place \(target)")
        animating = true
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.14
            layout()
        }
        animating = false
    }

    /// Untinted glass at rest, which refracts darker than the bar and reads
    /// as a well rather than a lamp; tinted only under the pointer.
    private func tintNewTabButton() {
        Glass.tint(newTabBackground, newTabHovered ? Palette.current.glassTint : nil)
        newTabButton.contentTintColor = newTabHovered
            ? Palette.current.text
            : Palette.current.dimText
    }

    /// One tracking area for the whole row.
    ///
    /// Per-cell areas alone left a close button showing after the pointer had
    /// gone: a cell that is resized or reused out from under the mouse never
    /// hears `mouseExited`. The row knows when the mouse has left it entirely,
    /// and that is the only moment every cell can be told at once.
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for area in trackingAreas { removeTrackingArea(area) }
        addTrackingArea(NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .mouseMoved, .activeAlways, .inVisibleRect],
            owner: self
        ))
        // Where the pointer is, rather than where it was last seen moving.
        // Tabs open and close and the row relays itself under a pointer that
        // is holding still, and a window's movability decided only on motion
        // would keep answering for a tab that is no longer there.
        if let window {
            updateWindowDragging(
                pointerAt: convert(window.mouseLocationOutsideOfEventStream, from: nil))
        }
    }

    override func mouseMoved(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        updateWindowDragging(pointerAt: point)
        let overPlus = newTabBackground.frame.contains(point)
        if overPlus != newTabHovered {
            newTabHovered = overPlus
            tintNewTabButton()
        }
    }

    override func mouseExited(with event: NSEvent) {
        setWindowDraggable(true)
        if newTabHovered {
            newTabHovered = false
            tintNewTabButton()
        }
        for cell in cells { cell.clearHover() }
    }

    /// A row taken out of its window leaves that window movable, whatever the
    /// pointer was over when it went.
    override func viewWillMove(toWindow newWindow: NSWindow?) {
        if newWindow !== window { setWindowDraggable(true) }
        super.viewWillMove(toWindow: newWindow)
    }

    @objc private func newTabPressed() {
        onNewTab?()
    }

    /// Colours resolved from the terminal background's luminance, so the bar
    /// belongs to whatever theme the terminal is wearing.
    struct Palette: Equatable {
        let text: NSColor
        let dimText: NSColor
        /// A title under the pointer. Between the two above on purpose: a tab
        /// being offered should answer, and still not answer as loudly as the
        /// tab you are actually in.
        let hoverText: NSColor
        /// The selected tab's fill. Nothing paints the unselected ones.
        let selectedFill: NSColor
        /// The faint capsule an unselected tab wears under the pointer.
        let hoverFill: NSColor
        /// The hairline around the selected tab, and the "+" button's ground.
        let edge: NSColor
        let controlFill: NSColor
        /// What glass is aimed at. Refraction alone comes out darker than a
        /// dark bar, and the shape this is modelled on is lighter than one.
        let glassTint: NSColor

        static var current: Palette {
            let background = GhosttyApp.shared.terminalBackground ?? .black
            let rgb = background.usingColorSpace(.sRGB) ?? .black
            let luminance = 0.2126 * rgb.redComponent
                + 0.7152 * rgb.greenComponent
                + 0.0722 * rgb.blueComponent
            let dark = luminance < 0.5
            let ink: NSColor = dark ? .white : .black
            return Palette(
                text: ink.withAlphaComponent(dark ? 0.92 : 0.85),
                dimText: ink.withAlphaComponent(0.45),
                hoverText: ink.withAlphaComponent(dark ? 0.72 : 0.66),
                selectedFill: ink.withAlphaComponent(dark ? 0.14 : 0.09),
                hoverFill: ink.withAlphaComponent(dark ? 0.05 : 0.035),
                edge: ink.withAlphaComponent(dark ? 0.16 : 0.10),
                controlFill: ink.withAlphaComponent(dark ? 0.09 : 0.06),
                glassTint: ink.withAlphaComponent(dark ? 0.22 : 0.14)
            )
        }
    }
}

/// One tab: a rounded fill when selected, bare text when not.
@MainActor
final class TabCellView: NSView {
    var onSelect: ((TabID) -> Void)?
    var onClose: ((TabID) -> Void)?

    private var item: SessionSnapshot.StripItem?
    private var palette: TabStripView.Palette?
    private var shortcut: String?
    private var hovered = false
    /// Whether this is the only tab there is. One tab is not a choice between
    /// tabs, so it is not drawn as one: no capsule, no number, no close
    /// button — just the title, and a window that reads as one thing.
    private var alone = false
    /// How far the title has to move to sit on the window's centre rather
    /// than on this cell's. Only a lone title asks for it.
    var titleOffset: CGFloat = 0 {
        didSet { if titleOffset != oldValue { needsLayout = true } }
    }

    /// The selected tab's capsule, in glass where the system has it.
    ///
    /// Two things had to be right for it to read: a glass view refracts what
    /// is behind it and needs a light tint to come out lighter than a dark
    /// bar rather than darker, and a border must not be set on its layer —
    /// that layer is a plain rectangle, so the hairline came out as a box
    /// around the capsule instead of following it. Glass carries its own
    /// edge; it does not want one drawn on.
    private let fill: NSView = Glass.lozenge(cornerRadius: 12) ?? NSView()
    private let fillIsGlass = Glass.isAvailable
    /// The capsule an unselected tab wears under the pointer.
    ///
    /// Flat, and its own view rather than the selected tab's. That one is
    /// glass, and glass under a hover refracts into a dark well: it reads as a
    /// hole punched in the bar rather than as a tab being offered, which is
    /// why hovering used to paint nothing at all. A plain tint is what being
    /// offered looks like, and it is what the palette named this colour for.
    private let hoverFill = NSView()
    /// Whether the capsule is currently being offered, so that a poll which
    /// only changed a title does not restart the fade.
    private var offering = false
    private let label = NSTextField(labelWithString: "")
    /// Kept, because how much room the title is owed changes with how much
    /// room the tab has. As inequalities against a centred label they become
    /// unsatisfiable in a narrow cell — 28 in from the left and 36 in from the
    /// right do not both fit in 40 points — and AppKit resolves that by
    /// breaking one and saying so.
    private var labelCenter: NSLayoutConstraint!
    private var labelLeading: NSLayoutConstraint!
    private var labelTrailing: NSLayoutConstraint!
    private let shortcutLabel = NSTextField(labelWithString: "")
    private let closeButton = NSButton()

    /// How much of the row the fill leaves alone, so a tab reads as a shape
    /// inside the titlebar rather than as a full-height block. Taken off both
    /// edges, so the capsule is two points shorter than this number suggests.
    private let verticalInset: CGFloat = 13
    /// Half the gap between two capsules: each tab insets its own fill, so
    /// neighbours end up twice this far apart.
    private let horizontalInset: CGFloat = 2

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true

        // Under everything, including the selected tab's glass: the two are
        // never shown together, but the order says which is the ground.
        hoverFill.wantsLayer = true
        hoverFill.layer?.cornerCurve = .continuous
        hoverFill.alphaValue = 0
        addSubview(hoverFill)

        fill.wantsLayer = true
        if !fillIsGlass { fill.layer?.cornerCurve = .continuous }
        addSubview(fill)

        label.font = .systemFont(ofSize: 12)
        label.alignment = .center
        label.lineBreakMode = .byTruncatingTail
        label.translatesAutoresizingMaskIntoConstraints = false
        addSubview(label)

        shortcutLabel.font = .systemFont(ofSize: 11)
        shortcutLabel.alignment = .right
        shortcutLabel.translatesAutoresizingMaskIntoConstraints = false
        addSubview(shortcutLabel)

        closeButton.image = NSImage(
            systemSymbolName: "xmark",
            accessibilityDescription: "Close Tab"
        )?.withSymbolConfiguration(.init(pointSize: 8, weight: .bold))
        closeButton.isBordered = false
        closeButton.imagePosition = .imageOnly
        closeButton.target = self
        closeButton.action = #selector(closePressed)
        closeButton.isHidden = true
        closeButton.translatesAutoresizingMaskIntoConstraints = false
        addSubview(closeButton)

        labelCenter = label.centerXAnchor.constraint(equalTo: centerXAnchor)
        labelLeading = label.leadingAnchor.constraint(
            greaterThanOrEqualTo: leadingAnchor, constant: 28)
        labelTrailing = label.trailingAnchor.constraint(
            lessThanOrEqualTo: trailingAnchor, constant: -36)

        NSLayoutConstraint.activate([
            labelCenter,
            label.centerYAnchor.constraint(equalTo: centerYAnchor),
            labelLeading,
            labelTrailing,

            closeButton.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            closeButton.centerYAnchor.constraint(equalTo: centerYAnchor),
            closeButton.widthAnchor.constraint(equalToConstant: 14),
            closeButton.heightAnchor.constraint(equalToConstant: 14),

            shortcutLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -14),
            shortcutLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])

        setAccessibilityElement(true)
        setAccessibilityRole(.button)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    override func layout() {
        super.layout()
        fill.frame = bounds.insetBy(dx: horizontalInset, dy: verticalInset)
        // A capsule: the radius is half the height, which is the shape a tab
        // lozenge has.
        let radius = fill.frame.height / 2
        if fillIsGlass {
            Glass.setCornerRadius(fill, radius)
        } else {
            fill.layer?.cornerRadius = radius
        }
        // The same shape in the same place, so that hovering a tab and then
        // choosing it is one capsule firming up rather than two capsules.
        hoverFill.frame = fill.frame
        hoverFill.layer?.cornerRadius = radius
        // What a tab shows is decided by how much of it there is. The number
        // is a hint and steps aside first; the close button goes next, since
        // a tab too narrow to name is not one to be closed by aim; the title
        // is last, and truncates. Each thing that leaves gives its room back
        // to the title.
        shortcutLabel.isHidden = shortcut == nil || bounds.width < 160
        closeButton.isHidden = !hovered || alone || bounds.width < 96
        labelLeading.constant = closeButton.isHidden ? 8 : 28
        labelTrailing.constant = shortcutLabel.isHidden ? -8 : -36
        labelCenter.constant = titleOffset
    }

    func apply(
        _ item: SessionSnapshot.StripItem,
        palette: TabStripView.Palette,
        shortcut: String?,
        alone: Bool
    ) {
        self.item = item
        self.palette = palette
        self.shortcut = shortcut
        self.alone = alone

        var title = item.title.isEmpty ? "untitled" : item.title
        if item.hasPanes { title += "  ⊞" }
        if item.busy { title = "✳ \(title)" }
        if label.stringValue != title { label.stringValue = title }
        label.font = .systemFont(ofSize: 12, weight: item.isActive ? .medium : .regular)

        // The tab you are in already answers, and a lone tab is a window
        // title with nothing to choose between — neither is an offer, so
        // neither takes one.
        let offering = hovered && !item.isActive && !alone
        label.textColor = item.isActive || alone
            ? palette.text
            : (offering ? palette.hoverText : palette.dimText)
        hoverFill.layer?.backgroundColor = palette.hoverFill.cgColor
        offer(offering)

        // One capsule in the row: the tab you are in. The others are text on
        // the chrome until the pointer is over them.
        if fillIsGlass {
            fill.isHidden = !item.isActive || alone
            Glass.tint(fill, palette.glassTint)
        } else {
            fill.isHidden = !item.isActive || alone
            fill.layer?.borderWidth = 1
            fill.layer?.borderColor = palette.edge.cgColor
            fill.layer?.backgroundColor = palette.selectedFill.cgColor
        }

        shortcutLabel.stringValue = shortcut ?? ""
        shortcutLabel.textColor = palette.dimText
        closeButton.contentTintColor = palette.text

        setAccessibilityLabel(title)
        toolTip = item.title
        needsLayout = true
    }

    /// Cells are reused and resized as tabs come and go, and a cell that
    /// moves out from under the pointer is never sent `mouseExited`. Asking
    /// where the mouse actually is settles it.
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for area in trackingAreas { removeTrackingArea(area) }
        addTrackingArea(NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect],
            owner: self
        ))
        let inside = window.map { window -> Bool in
            let point = convert(window.mouseLocationOutsideOfEventStream, from: nil)
            return bounds.contains(point)
        } ?? false
        if inside != hovered {
            hovered = inside
            refresh()
        }
    }

    override func mouseEntered(with event: NSEvent) {
        guard !hovered else { return }
        hovered = true
        refresh()
    }

    override func mouseExited(with event: NSEvent) {
        clearHover()
    }

    /// Fade the capsule in or out, and only when the answer has changed: a
    /// poll arrives every so often and would otherwise restart the fade from
    /// the top while the pointer sits perfectly still.
    private func offer(_ offering: Bool) {
        guard offering != self.offering else { return }
        self.offering = offering
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.12
            context.timingFunction = CAMediaTimingFunction(name: .easeOut)
            hoverFill.animator().alphaValue = offering ? 1 : 0
        }
    }

    /// Told from the row that the pointer is gone, for the case AppKit does
    /// not say so itself.
    func clearHover() {
        guard hovered else { return }
        hovered = false
        refresh()
    }

    private func refresh() {
        if let item, let palette {
            apply(item, palette: palette, shortcut: shortcut, alone: alone)
        }
    }

    /// Which tab this cell is showing, for the row that reorders them.
    var tabID: TabID? { item?.id }

    /// Every press inside a tab is the tab's, wherever it lands.
    ///
    /// A cell is made of a pane of glass and two labels, and a press that
    /// lands on one of those is that view's press, not the cell's — which
    /// meant it never reached the code that moves tabs at all. The close
    /// button is a control and keeps its clicks; everything else in here is
    /// decoration.
    override func hitTest(_ point: NSPoint) -> NSView? {
        let local = convert(point, from: superview)
        guard bounds.contains(local) else { return nil }
        if !closeButton.isHidden, closeButton.frame.contains(local) { return closeButton }
        return self
    }

    /// The press is handed to the row rather than answered here: it might be
    /// a click that selects, or the start of a move, and only the row knows
    /// what the others should do while that is being decided.
    override func mouseDown(with event: NSEvent) {
        guard let onPress else {
            if let item { onSelect?(item.id) }
            return
        }
        onPress(self, event)
    }

    var onPress: ((TabCellView, NSEvent) -> Void)?

    override func accessibilityPerformPress() -> Bool {
        if let item { onSelect?(item.id); return true }
        return false
    }

    @objc private func closePressed() {
        if let item { onClose?(item.id) }
    }
}
