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

    /// Points kept free at the left for chrome that overlaps this row when
    /// the sidebar is collapsed (traffic lights, sidebar toggle).
    var leadingClearance: CGFloat = 0 {
        didSet { if leadingClearance != oldValue { needsLayout = true } }
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

    /// Empty regions stay draggable titlebar, like the tint backdrops.
    override func hitTest(_ point: NSPoint) -> NSView? {
        let hit = super.hitTest(point)
        return hit === self ? nil : hit
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
            cell.apply(items[index], palette: palette, shortcut: Self.shortcut(
                index: index, count: items.count))
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
        // control, which is what it is.
        let plusSide: CGFloat = 24
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

        guard !cells.isEmpty else { return }
        let left = leadingClearance
        let available = max(0, bounds.width - left - plusWidth - 8)
        // Tabs fill the row rather than sitting in a corner of it.
        let width = max(80, min(available, available / CGFloat(cells.count)))
        var x = left
        for cell in cells {
            // Whole pixels: a fill edge on a half pixel renders soft.
            let next = (x + width).rounded()
            cell.frame = NSRect(x: x.rounded(), y: 0, width: next - x.rounded(), height: height)
            x = next
        }
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
    }

    override func mouseMoved(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        let overPlus = newTabBackground.frame.contains(point)
        if overPlus != newTabHovered {
            newTabHovered = overPlus
            tintNewTabButton()
        }
    }

    override func mouseExited(with event: NSEvent) {
        if newTabHovered {
            newTabHovered = false
            tintNewTabButton()
        }
        for cell in cells { cell.clearHover() }
    }

    @objc private func newTabPressed() {
        onNewTab?()
    }

    /// Colours resolved from the terminal background's luminance, so the bar
    /// belongs to whatever theme the terminal is wearing.
    struct Palette: Equatable {
        let text: NSColor
        let dimText: NSColor
        /// The selected tab's fill. Nothing paints the unselected ones.
        let selectedFill: NSColor
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
private final class TabCellView: NSView {
    var onSelect: ((TabID) -> Void)?
    var onClose: ((TabID) -> Void)?

    private var item: SessionSnapshot.StripItem?
    private var palette: TabStripView.Palette?
    private var shortcut: String?
    private var hovered = false

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
    private let label = NSTextField(labelWithString: "")
    private let shortcutLabel = NSTextField(labelWithString: "")
    private let closeButton = NSButton()

    /// How much of the row the fill leaves alone, so a tab reads as a shape
    /// inside the titlebar rather than as a full-height block.
    private let verticalInset: CGFloat = 11
    /// Half the gap between two capsules: each tab insets its own fill, so
    /// neighbours end up twice this far apart.
    private let horizontalInset: CGFloat = 2

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true

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

        NSLayoutConstraint.activate([
            label.centerXAnchor.constraint(equalTo: centerXAnchor),
            label.centerYAnchor.constraint(equalTo: centerYAnchor),
            label.leadingAnchor.constraint(greaterThanOrEqualTo: leadingAnchor, constant: 28),
            label.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -36),

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
        // A capsule, not a rounded rectangle: the radius is half the height,
        // which is the shape a tab lozenge has.
        // A capsule: the radius is half the height, which is the shape a tab
        // lozenge has.
        let radius = fill.frame.height / 2
        if fillIsGlass {
            Glass.setCornerRadius(fill, radius)
        } else {
            fill.layer?.cornerRadius = radius
        }
        // The number is a hint, not a control: it steps aside when a tab is
        // too narrow to carry both it and a readable title.
        shortcutLabel.isHidden = shortcut == nil || bounds.width < 160
    }

    func apply(
        _ item: SessionSnapshot.StripItem,
        palette: TabStripView.Palette,
        shortcut: String?
    ) {
        self.item = item
        self.palette = palette
        self.shortcut = shortcut

        var title = item.title.isEmpty ? "untitled" : item.title
        if item.hasPanes { title += "  ⊞" }
        if item.busy { title = "✳ \(title)" }
        if label.stringValue != title { label.stringValue = title }
        label.font = .systemFont(ofSize: 12, weight: item.isActive ? .medium : .regular)
        label.textColor = item.isActive ? palette.text : palette.dimText

        // Only the selected tab is painted. The rest are text on the chrome,
        // and hovering one hints at it without claiming to be it. On glass
        // the lozenge is simply present or absent — refraction is the
        // highlight, so painting a colour under it as well would muddy it.
        if fillIsGlass {
            // Present or absent, tinted rather than painted: refraction is
            // the highlight, and a tint is how it is aimed lighter.
            // The same two states the "+" button has, and made of the same
            // thing: untinted glass under the pointer, which refracts into a
            // dark well, and tinted glass when selected, which is what aims
            // it lighter than the bar.
            fill.isHidden = !item.isActive && !hovered
            Glass.tint(fill, item.isActive ? palette.glassTint : nil)
        } else {
            fill.layer?.borderWidth = item.isActive ? 1 : 0
            fill.layer?.borderColor = palette.edge.cgColor
            let background: NSColor = item.isActive
                ? palette.selectedFill
                : (hovered ? palette.hoverFill : .clear)
            fill.layer?.backgroundColor = background.cgColor
        }

        shortcutLabel.stringValue = shortcut ?? ""
        shortcutLabel.textColor = palette.dimText
        closeButton.contentTintColor = palette.text
        closeButton.isHidden = !hovered

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

    /// Told from the row that the pointer is gone, for the case AppKit does
    /// not say so itself.
    func clearHover() {
        guard hovered else { return }
        hovered = false
        refresh()
    }

    private func refresh() {
        if let item, let palette {
            apply(item, palette: palette, shortcut: shortcut)
        }
    }

    override func mouseDown(with event: NSEvent) {
        if let item { onSelect?(item.id) }
    }

    override func accessibilityPerformPress() -> Bool {
        if let item { onSelect?(item.id); return true }
        return false
    }

    @objc private func closePressed() {
        if let item { onClose?(item.id) }
    }
}
