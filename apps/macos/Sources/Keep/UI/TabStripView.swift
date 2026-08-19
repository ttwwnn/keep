import AppKit

/// The app-drawn tab bar, imitating the native macOS one: a 28 pt row of
/// flat, equal-width tabs with hairline separators, centered titles, a close
/// button on hover, and "+" pinned at the trailing edge.
///
/// It exists because the native bar cannot be kept: AppKit's tab bar means
/// AppKit's window-per-tab, and windows coming and going is the bug class
/// this shell was rebuilt to remove. This view is plain content — selecting
/// a tab is a message upward, not a window operation.
///
/// It draws no base color: it sits on the existing terminal-tinted chrome,
/// and its overlays resolve from the terminal background's luminance, so the
/// selected tab reads as continuous with the content below it — which is
/// exactly what the native selected tab does.
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
        for (cell, item) in zip(cells, items) {
            cell.apply(item, palette: Palette.current)
        }
        needsLayout = true
    }

    private func retint() {
        for (cell, item) in zip(cells, items) {
            cell.apply(item, palette: Palette.current)
        }
        newTabButton.contentTintColor = Palette.current.title.withAlphaComponent(0.55)
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
        let width = min(220, max(60, available / CGFloat(cells.count)))
        for (index, cell) in cells.enumerated() {
            cell.frame = NSRect(x: left + CGFloat(index) * width, y: 0, width: width, height: height)
        }
    }

    @objc private func newTabPressed() {
        onNewTab?()
    }

    /// Colors resolved from the terminal background's luminance.
    struct Palette {
        let title: NSColor
        let unselectedOverlay: NSColor
        let hoverOverlay: NSColor
        let hairline: NSColor

        static var current: Palette {
            let background = GhosttyApp.shared.terminalBackground ?? .black
            let rgb = background.usingColorSpace(.sRGB) ?? .black
            let luminance = 0.2126 * rgb.redComponent
                + 0.7152 * rgb.greenComponent
                + 0.0722 * rgb.blueComponent
            let dark = luminance < 0.5
            let title: NSColor = dark ? .white : .black
            return Palette(
                title: title.withAlphaComponent(0.85),
                unselectedOverlay: NSColor.black.withAlphaComponent(dark ? 0.22 : 0.07),
                hoverOverlay: NSColor.black.withAlphaComponent(dark ? 0.11 : 0.035),
                hairline: title.withAlphaComponent(0.14)
            )
        }
    }
}

/// One tab. The selected cell draws no overlay — it shows the chrome tint,
/// i.e. the terminal's own color, continuous with the content below.
@MainActor
private final class TabCellView: NSView {
    var onSelect: ((TabID) -> Void)?
    var onClose: ((TabID) -> Void)?

    private var item: SessionSnapshot.StripItem?
    private var hovered = false
    private let label = NSTextField(labelWithString: "")
    private let closeButton = NSButton()
    private let hairline = NSView()

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true

        label.font = .systemFont(ofSize: 11)
        label.alignment = .center
        label.lineBreakMode = .byTruncatingMiddle
        label.translatesAutoresizingMaskIntoConstraints = false
        addSubview(label)

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

        hairline.wantsLayer = true
        hairline.translatesAutoresizingMaskIntoConstraints = false
        addSubview(hairline)

        NSLayoutConstraint.activate([
            label.centerXAnchor.constraint(equalTo: centerXAnchor),
            label.centerYAnchor.constraint(equalTo: centerYAnchor),
            label.leadingAnchor.constraint(greaterThanOrEqualTo: leadingAnchor, constant: 20),
            label.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -20),
            closeButton.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 4),
            closeButton.centerYAnchor.constraint(equalTo: centerYAnchor),
            closeButton.widthAnchor.constraint(equalToConstant: 16),
            closeButton.heightAnchor.constraint(equalToConstant: 16),
            hairline.trailingAnchor.constraint(equalTo: trailingAnchor),
            hairline.topAnchor.constraint(equalTo: topAnchor, constant: 7),
            hairline.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -7),
            hairline.widthAnchor.constraint(equalToConstant: 1),
        ])

        setAccessibilityElement(true)
        setAccessibilityRole(.button)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    func apply(_ item: SessionSnapshot.StripItem, palette: TabStripView.Palette) {
        self.item = item
        var title = item.title
        if item.hasPanes { title += "  ⊞" }
        if item.busy { title = "✳ \(title)" }
        if label.stringValue != title { label.stringValue = title }
        label.font = .systemFont(ofSize: 11, weight: item.isActive ? .medium : .regular)
        label.textColor = item.isActive
            ? palette.title
            : palette.title.withAlphaComponent(0.55)
        layer?.backgroundColor = item.isActive
            ? nil
            : (hovered ? palette.hoverOverlay : palette.unselectedOverlay).cgColor
        hairline.layer?.backgroundColor = palette.hairline.cgColor
        closeButton.contentTintColor = palette.title.withAlphaComponent(0.7)
        setAccessibilityLabel(title)
        toolTip = item.title
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for area in trackingAreas { removeTrackingArea(area) }
        addTrackingArea(NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect],
            owner: self
        ))
    }

    override func mouseEntered(with event: NSEvent) {
        hovered = true
        closeButton.isHidden = false
        refresh()
    }

    override func mouseExited(with event: NSEvent) {
        hovered = false
        closeButton.isHidden = true
        refresh()
    }

    private func refresh() {
        if let item { apply(item, palette: .current) }
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
