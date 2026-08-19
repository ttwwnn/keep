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
    private var backgroundObserver: NSObjectProtocol?

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true

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
        let plusWidth: CGFloat = 30
        newTabButton.frame = NSRect(
            x: bounds.width - plusWidth - 4, y: (height - 24) / 2, width: plusWidth, height: 24)

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
                selectedFill: ink.withAlphaComponent(dark ? 0.10 : 0.07),
                hoverFill: ink.withAlphaComponent(dark ? 0.05 : 0.035)
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

    private let fill = NSView()
    private let label = NSTextField(labelWithString: "")
    private let shortcutLabel = NSTextField(labelWithString: "")
    private let closeButton = NSButton()

    /// How much of the row the fill leaves alone, so a tab reads as a shape
    /// inside the titlebar rather than as a full-height block.
    private let verticalInset: CGFloat = 10
    private let horizontalInset: CGFloat = 3

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true

        fill.wantsLayer = true
        fill.layer?.cornerRadius = 6
        fill.layer?.cornerCurve = .continuous
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
        // and hovering one hints at it without claiming to be it.
        let background: NSColor = item.isActive
            ? palette.selectedFill
            : (hovered ? palette.hoverFill : .clear)
        fill.layer?.backgroundColor = background.cgColor

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
