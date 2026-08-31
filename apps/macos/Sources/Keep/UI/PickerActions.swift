import AppKit

/// What can be done to a row of the picker, beyond going to it.
///
/// A short, closed list on purpose. Everything here is something the row
/// itself already implies — where it is, what it is holding — so the panel
/// that shows them never has to explain what it is a panel of.
enum PickerAction: Hashable {
    /// What Return already does. It is in the list anyway, because a menu of
    /// what can be done to a row that omits the main thing reads as a menu of
    /// afterthoughts.
    case open
    /// A new tab in the workspace the row belongs to.
    case newTab
    case reveal
    case copyPath
    /// A line of history, as text.
    case copyText
    /// End what the row points at.
    case close
}

/// One line of the actions panel: what it does, and how else to ask for it.
struct PickerActionEntry: Equatable {
    let action: PickerAction
    let title: String
    /// An SF Symbol. The list is read down the left edge before it is read
    /// across, and a column of glyphs is what makes that possible.
    let symbol: String
    /// The keys that do the same thing without opening this panel, drawn as
    /// caps. Empty when there are none — an action reachable only from here.
    var keys: [String] = []
    /// Drawn in red, and always last: the one entry that does not undo.
    var destructive = false
    /// A rule above this entry, separating it from what came before.
    var startsGroup = false

    /// What can be done to this row.
    ///
    /// Decided from the kind, because that is what the actions differ over: a
    /// terminal can be closed and a folder cannot, a folder can be opened in
    /// Finder and a line of history has no folder to open.
    static func list(for item: PickerModel.Item) -> [PickerActionEntry] {
        switch item.kind {
        case .running:
            var entries = [
                PickerActionEntry(
                    action: .open, title: "Go to this terminal",
                    symbol: "arrow.turn.down.right", keys: ["↩"]),
                PickerActionEntry(
                    action: .newTab,
                    title: item.workspace.isEmpty
                        ? "New tab here" : "New tab in \(item.workspace)",
                    symbol: "plus.rectangle", startsGroup: true),
            ]
            if !item.path.isEmpty {
                entries.append(PickerActionEntry(
                    action: .reveal, title: "Reveal in Finder",
                    symbol: "folder", keys: ["⇧", "⌘", "R"]))
                entries.append(PickerActionEntry(
                    action: .copyPath, title: "Copy directory path",
                    symbol: "doc.on.doc", keys: ["⇧", "⌘", "C"]))
            }
            entries.append(PickerActionEntry(
                action: .close, title: "Close this terminal",
                symbol: "xmark", keys: ["⌃", "D"], destructive: true,
                startsGroup: true))
            return entries
        case .destination:
            return [
                PickerActionEntry(
                    action: .open, title: "Open a workspace here",
                    symbol: "folder.badge.plus", keys: ["↩"]),
                PickerActionEntry(
                    action: .reveal, title: "Reveal in Finder",
                    symbol: "folder", keys: ["⇧", "⌘", "R"], startsGroup: true),
                PickerActionEntry(
                    action: .copyPath, title: "Copy path",
                    symbol: "doc.on.doc", keys: ["⇧", "⌘", "C"]),
            ]
        case .hit:
            return [
                PickerActionEntry(
                    action: .open, title: "Go to this line",
                    symbol: "arrow.turn.down.right", keys: ["↩"]),
                PickerActionEntry(
                    action: .copyText, title: "Copy line",
                    symbol: "doc.on.doc", keys: ["⇧", "⌘", "C"],
                    startsGroup: true),
            ]
        case .command(let command):
            // One thing, which Return already does. The panel is here for
            // consistency — every row in this list answers ⌘K — and because
            // a panel that names the command is a confirmation of the
            // destructive ones.
            return [
                PickerActionEntry(
                    action: .open, title: command.title,
                    symbol: command.symbol, keys: ["↩"]),
            ]
        case .theme:
            return [
                PickerActionEntry(
                    action: .open, title: "Wear this theme",
                    symbol: "paintpalette", keys: ["↩"]),
                PickerActionEntry(
                    action: .copyText, title: "Copy its name",
                    symbol: "doc.on.doc", keys: ["⇧", "⌘", "C"], startsGroup: true),
            ]
        case .fontFamily:
            return [
                PickerActionEntry(
                    action: .open, title: "Set the terminal in this face",
                    symbol: "textformat", keys: ["↩"]),
                PickerActionEntry(
                    action: .copyText, title: "Copy its name",
                    symbol: "doc.on.doc", keys: ["⇧", "⌘", "C"], startsGroup: true),
            ]
        }
    }
}

// MARK: - key caps

/// One key, drawn as a key.
///
/// A shortcut written as `⇧⌘C` is a string to be parsed; drawn as three caps
/// it is three things to be pressed. The eye counts them without reading.
final class KeyCapView: NSView {
    private let label = NSTextField(labelWithString: "")

    init(_ key: String) {
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerCurve = .continuous
        layer?.cornerRadius = 6
        layer?.backgroundColor = NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
                ? NSColor.white.withAlphaComponent(0.09)
                : NSColor.black.withAlphaComponent(0.06)
        }.cgColor
        // The system face, not the terminal's: these are keys on a keyboard,
        // not text a program printed.
        label.font = .systemFont(ofSize: 11, weight: .medium)
        label.textColor = .secondaryLabelColor
        label.stringValue = key
        label.alignment = .center
        label.translatesAutoresizingMaskIntoConstraints = false
        addSubview(label)
        translatesAutoresizingMaskIntoConstraints = false
        // Nothing may stretch a key. Two greater-thans and no upper bound
        // leave the width ambiguous, and a row that pins only the trailing
        // edge resolves that ambiguity by making the cap as wide as the row
        // — which is how a single ↩ came out a hand's width of grey.
        setContentHuggingPriority(.required, for: .horizontal)
        let fit = widthAnchor.constraint(equalTo: label.widthAnchor, constant: 10)
        fit.priority = .defaultHigh
        NSLayoutConstraint.activate([
            label.centerXAnchor.constraint(equalTo: centerXAnchor),
            label.centerYAnchor.constraint(equalTo: centerYAnchor),
            heightAnchor.constraint(equalToConstant: 18),
            // Square for one glyph, wider only when the key needs it.
            widthAnchor.constraint(greaterThanOrEqualToConstant: 18),
            fit,
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    /// A row of caps, in the order they are held down.
    static func row(_ keys: [String]) -> NSStackView {
        let stack = NSStackView(views: keys.map { KeyCapView($0) })
        stack.orientation = .horizontal
        stack.spacing = 4
        stack.setHuggingPriority(.required, for: .horizontal)
        stack.translatesAutoresizingMaskIntoConstraints = false
        return stack
    }
}

// MARK: - the footer

/// What Return does, and where the rest of it is — one pane of glass in the
/// corner, not a bar across the card.
///
/// A full-width footer is a second surface inside a card that is meant to
/// read as one, and it takes a strip of the list with it. This floats over
/// the card instead, in the corner the actions panel rises out of, so the
/// chip and the panel it opens are visibly the same object.
@MainActor
final class PickerFooter: NSView {
    var onOpen: (() -> Void)?
    var onActions: (() -> Void)?

    /// The room the card leaves beneath the list for it: the glass, and the
    /// margin it floats on.
    static let height: CGFloat = 34
    static let margin: CGFloat = 12

    private let row = NSView()
    private var pill: NSView!
    private let openChip = ChipView(title: "Open", keys: ["↩"])
    private let actionsChip = ChipView(title: "Actions", keys: ["⌘", "K"])

    override init(frame: NSRect) {
        super.init(frame: frame)
        translatesAutoresizingMaskIntoConstraints = false

        // Almost the full half-height: a squircle rather than a rectangle
        // that has had its corners taken off.
        pill = Glass.panel(row, cornerRadius: 16)
        // A shade darker than the card it sits on, or a pane of glass over
        // glass is a corner of the card that has simply gone slightly bright.
        // The same shade the panel above it is made of, since the panel opens
        // out of this chip.
        Glass.tint(pill, PickerActionsPanel.material)
        pill.translatesAutoresizingMaskIntoConstraints = false
        row.translatesAutoresizingMaskIntoConstraints = false
        addSubview(pill)

        openChip.onClick = { [weak self] in self?.onOpen?() }
        actionsChip.onClick = { [weak self] in self?.onActions?() }

        // Between the two, because they are two things: one happens now, the
        // other opens a list of things that might.
        let divider = NSView()
        divider.wantsLayer = true
        divider.layer?.backgroundColor = NSColor.separatorColor.cgColor
        divider.translatesAutoresizingMaskIntoConstraints = false

        for view in [openChip, divider, actionsChip] as [NSView] { row.addSubview(view) }

        NSLayoutConstraint.activate([
            pill.leadingAnchor.constraint(equalTo: leadingAnchor),
            pill.trailingAnchor.constraint(equalTo: trailingAnchor),
            pill.topAnchor.constraint(equalTo: topAnchor),
            pill.bottomAnchor.constraint(equalTo: bottomAnchor),
            row.leadingAnchor.constraint(equalTo: pill.leadingAnchor),
            row.trailingAnchor.constraint(equalTo: pill.trailingAnchor),
            row.topAnchor.constraint(equalTo: pill.topAnchor),
            row.bottomAnchor.constraint(equalTo: pill.bottomAnchor),

            heightAnchor.constraint(equalToConstant: Self.height),
            openChip.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 5),
            openChip.centerYAnchor.constraint(equalTo: row.centerYAnchor),
            divider.leadingAnchor.constraint(equalTo: openChip.trailingAnchor, constant: 5),
            divider.centerYAnchor.constraint(equalTo: row.centerYAnchor),
            divider.widthAnchor.constraint(equalToConstant: 1),
            divider.heightAnchor.constraint(equalToConstant: 16),
            actionsChip.leadingAnchor.constraint(equalTo: divider.trailingAnchor, constant: 5),
            actionsChip.centerYAnchor.constraint(equalTo: row.centerYAnchor),
            actionsChip.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -5),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    /// Gone when nothing is highlighted, rather than offering to open a row
    /// that is not there.
    func show(_ item: PickerModel.Item?) { isHidden = item == nil }

    /// A label and its keys, as one thing you can also click.
    private final class ChipView: NSView {
        var onClick: (() -> Void)?
        private let wash = NSView()

        init(title: String, keys: [String]) {
            super.init(frame: .zero)
            wantsLayer = true
            translatesAutoresizingMaskIntoConstraints = false

            wash.wantsLayer = true
            wash.layer?.cornerCurve = .continuous
            wash.layer?.cornerRadius = 10
            wash.layer?.backgroundColor = NSColor(name: nil) { appearance in
                appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
                    ? NSColor.white.withAlphaComponent(0.08)
                    : NSColor.black.withAlphaComponent(0.05)
            }.cgColor
            wash.isHidden = true
            wash.translatesAutoresizingMaskIntoConstraints = false
            addSubview(wash)

            let label = NSTextField(labelWithString: title)
            label.font = .systemFont(ofSize: 12)
            label.textColor = .secondaryLabelColor
            label.translatesAutoresizingMaskIntoConstraints = false
            addSubview(label)

            let caps = KeyCapView.row(keys)
            addSubview(caps)

            NSLayoutConstraint.activate([
                label.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 9),
                label.centerYAnchor.constraint(equalTo: centerYAnchor),
                caps.leadingAnchor.constraint(equalTo: label.trailingAnchor, constant: 7),
                caps.centerYAnchor.constraint(equalTo: centerYAnchor),
                caps.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -7),
                heightAnchor.constraint(equalToConstant: 26),
                wash.leadingAnchor.constraint(equalTo: leadingAnchor),
                wash.trailingAnchor.constraint(equalTo: trailingAnchor),
                wash.topAnchor.constraint(equalTo: topAnchor),
                wash.bottomAnchor.constraint(equalTo: bottomAnchor),
            ])
        }

        @available(*, unavailable)
        required init?(coder: NSCoder) { fatalError("not supported") }

        override func updateTrackingAreas() {
            super.updateTrackingAreas()
            for area in trackingAreas { removeTrackingArea(area) }
            addTrackingArea(NSTrackingArea(
                rect: .zero,
                options: [.mouseEnteredAndExited, .activeInActiveApp, .inVisibleRect],
                owner: self))
        }

        override func mouseEntered(with event: NSEvent) { wash.isHidden = false }
        override func mouseExited(with event: NSEvent) { wash.isHidden = true }

        // Taken here so that the matching mouse-up is sent here: AppKit gives
        // the release to whoever accepted the press, and a view that lets the
        // press travel up the chain never hears the click it was drawn for.
        override func mouseDown(with event: NSEvent) {}

        override func mouseUp(with event: NSEvent) {
            guard bounds.contains(convert(event.locationInWindow, from: nil)) else { return }
            onClick?()
        }
    }
}


// MARK: - the actions panel

/// The panel that ⌘K and the right button both open.
///
/// One surface, however it was asked for: a right-click does not deserve a
/// different menu from the one the keyboard gets, and two of them would be
/// two things to keep in step. It rises from the bottom-right corner, over
/// the footer that named it, and takes the keyboard for as long as it is up —
/// its own field, so typing filters the actions instead of the list behind.
@MainActor
final class PickerActionsPanel: NSView {
    var onRun: ((PickerAction) -> Void)?
    var onClose: (() -> Void)?

    static let width: CGFloat = 300

    /// The rules inside the panel.
    ///
    /// `separatorColor` is drawn for a window's own chrome, and on glass at
    /// half strength it came out as bright as the text it was separating.
    /// What a group needs is the least line that still reads as one.
    static let rule = NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
            ? NSColor.white.withAlphaComponent(0.07)
            : NSColor.black.withAlphaComponent(0.07)
    }

    /// The glass the corner is made of, shared with the chip below it.
    ///
    /// Thin at both ends. A tint is laid *over* the refraction, so the more
    /// of it there is the less glass is left: at half strength this was a
    /// grey rectangle with a corner radius. Black rather than white in the
    /// dark — the card underneath is already dark, and lifting the glass off
    /// it with white made a pale chip in a dark corner; taking it down
    /// instead keeps the refraction and lets the edge do the lifting.
    static let material = NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
            ? NSColor(white: 0.0, alpha: 0.30)
            : NSColor(white: 1.0, alpha: 0.55)
    }

    private let content = NSView()
    private var panel: NSView!
    private let heading = NSTextField(labelWithString: "")
    private let rows = NSStackView()
    private let field = NSTextField()

    private var entries: [PickerActionEntry] = []
    private var shown: [PickerActionEntry] = []
    private var selected = 0

    override init(frame: NSRect) {
        super.init(frame: frame)
        panel = Glass.panel(content, cornerRadius: 18)
        // The chip's own material, exactly: this panel rises out of the chip
        // and lands on its edge, and two pieces of glass that meet at a
        // corner in different tints read as two panels, one of them stuck to
        // the other.
        Glass.tint(panel, Self.material)
        panel.translatesAutoresizingMaskIntoConstraints = false
        content.translatesAutoresizingMaskIntoConstraints = false
        addSubview(panel)

        // The row these actions are about, said once at the top. Without it
        // a panel opened by ⌘K is a list of verbs with no object.
        heading.font = .systemFont(ofSize: 11, weight: .medium)
        heading.textColor = .tertiaryLabelColor
        heading.lineBreakMode = .byTruncatingTail
        heading.maximumNumberOfLines = 1
        heading.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(heading)

        rows.orientation = .vertical
        rows.spacing = 0
        rows.alignment = .leading
        rows.distribution = .fill
        rows.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(rows)

        // A rule here is structure, not a seam: the field below it is a
        // different kind of thing from the list above it, and this panel is
        // small enough that the line reads as the join between the two.
        let rule = NSView()
        rule.wantsLayer = true
        rule.layer?.backgroundColor = Self.rule.cgColor
        rule.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(rule)

        field.font = .systemFont(ofSize: 13)
        field.placeholderString = "Search for actions…"
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.delegate = self
        field.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(field)

        NSLayoutConstraint.activate([
            panel.leadingAnchor.constraint(equalTo: leadingAnchor),
            panel.trailingAnchor.constraint(equalTo: trailingAnchor),
            panel.topAnchor.constraint(equalTo: topAnchor),
            panel.bottomAnchor.constraint(equalTo: bottomAnchor),
            content.leadingAnchor.constraint(equalTo: panel.leadingAnchor),
            content.trailingAnchor.constraint(equalTo: panel.trailingAnchor),
            content.topAnchor.constraint(equalTo: panel.topAnchor),
            content.bottomAnchor.constraint(equalTo: panel.bottomAnchor),

            heading.topAnchor.constraint(equalTo: content.topAnchor, constant: 14),
            // Over the titles, not over the glyph column: the heading names
            // the row, and the row's name starts where its icon ends.
            heading.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 18),
            heading.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -18),

            rows.topAnchor.constraint(equalTo: heading.bottomAnchor, constant: 10),
            rows.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            rows.trailingAnchor.constraint(equalTo: content.trailingAnchor),

            // Inset from the rounded edges it would otherwise run into.
            rule.topAnchor.constraint(equalTo: rows.bottomAnchor, constant: 8),
            rule.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 12),
            rule.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -12),
            rule.heightAnchor.constraint(equalToConstant: 1),

            // Padded on both sides rather than given a tall box to sit in:
            // a text field draws its line at the top of whatever frame it is
            // handed, so a 42-point one put the words against the rule and
            // left the air underneath, where it does nothing.
            field.topAnchor.constraint(equalTo: rule.bottomAnchor, constant: 13),
            field.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 18),
            field.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -18),
            field.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -13),

            widthAnchor.constraint(equalToConstant: Self.width),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    /// Open on a row: what it is, and what can be done to it.
    func present(_ item: PickerModel.Item) {
        heading.stringValue = item.title.isEmpty ? item.path : item.title
        entries = PickerActionEntry.list(for: item)
        field.stringValue = ""
        rebuild(filter: "")
    }

    /// The panel's own field owns the keyboard while it is up, so that typing
    /// narrows the actions rather than the list underneath.
    func takeFocus() {
        window?.makeFirstResponder(field)
        if let editor = window?.fieldEditor(true, for: field) as? NSTextView {
            editor.insertionPointColor = .white
        }
    }

    private func rebuild(filter: String) {
        let needle = filter.lowercased()
        shown = needle.isEmpty
            ? entries
            : entries.filter { $0.title.lowercased().contains(needle) }
        selected = shown.isEmpty ? -1 : 0
        for view in rows.arrangedSubviews {
            rows.removeArrangedSubview(view)
            view.removeFromSuperview()
        }
        for (index, entry) in shown.enumerated() {
            // A rule between groups, but never above the first row: a list
            // that opens with a divider is a list missing its first entry.
            if entry.startsGroup, index > 0, needle.isEmpty {
                let band = NSView()
                band.translatesAutoresizingMaskIntoConstraints = false
                let rule = NSView()
                rule.wantsLayer = true
                rule.layer?.backgroundColor = Self.rule.cgColor
                rule.translatesAutoresizingMaskIntoConstraints = false
                band.addSubview(rule)
                rows.addArrangedSubview(band)
                NSLayoutConstraint.activate([
                    // The line is one point; the band is the air around it.
                    // Drawn without that air, four rules in a panel this size
                    // read as a table, not as three groups.
                    band.heightAnchor.constraint(equalToConstant: 11),
                    band.widthAnchor.constraint(equalTo: rows.widthAnchor),
                    rule.leadingAnchor.constraint(equalTo: band.leadingAnchor, constant: 12),
                    rule.trailingAnchor.constraint(equalTo: band.trailingAnchor, constant: -12),
                    rule.centerYAnchor.constraint(equalTo: band.centerYAnchor),
                    rule.heightAnchor.constraint(equalToConstant: 1),
                ])
            }
            let row = ActionRowView(entry)
            row.onHover = { [weak self] in self?.select(index) }
            row.onClick = { [weak self] in self?.run(index) }
            rows.addArrangedSubview(row)
            row.widthAnchor.constraint(equalTo: rows.widthAnchor).isActive = true
        }
        paint()
    }

    private var actionRows: [ActionRowView] {
        rows.arrangedSubviews.compactMap { $0 as? ActionRowView }
    }

    private func paint() {
        for (index, row) in actionRows.enumerated() { row.isSelected = index == selected }
    }

    private func select(_ index: Int) {
        guard index >= 0, index < shown.count, index != selected else { return }
        selected = index
        paint()
    }

    private func step(_ by: Int) {
        guard !shown.isEmpty else { return }
        select(min(max(selected + by, 0), shown.count - 1))
    }

    private func run(_ index: Int) {
        guard index >= 0, index < shown.count else { return }
        onRun?(shown[index].action)
    }

}

extension PickerActionsPanel: NSTextFieldDelegate {
    func controlTextDidChange(_ notification: Notification) {
        rebuild(filter: field.stringValue)
    }

    func control(
        _ control: NSControl, textView: NSTextView, doCommandBy selector: Selector
    ) -> Bool {
        switch selector {
        case #selector(NSResponder.moveDown(_:)):
            step(1)
            return true
        case #selector(NSResponder.moveUp(_:)):
            step(-1)
            return true
        case #selector(NSResponder.insertNewline(_:)):
            run(selected)
            return true
        case #selector(NSResponder.deleteForward(_:)):
            // ⌃D, which this panel advertises on the row that has it. The
            // field is empty-handed here, so it is unambiguous.
            if let index = shown.firstIndex(where: { $0.action == .close }) { run(index) }
            return true
        case #selector(NSResponder.cancelOperation(_:)):
            onClose?()
            return true
        default:
            return false
        }
    }
}

/// One action, drawn the way the picker's own rows are drawn.
private final class ActionRowView: NSView {
    var onHover: (() -> Void)?
    var onClick: (() -> Void)?

    private let lozenge = AdaptiveLozengeView(
        cornerRadius: 11, lightFill: NSColor.black.withAlphaComponent(0.08))

    init(_ entry: PickerActionEntry) {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        lozenge.isHidden = true
        addSubview(lozenge, positioned: .below, relativeTo: nil)

        let colour: NSColor = entry.destructive ? .systemRed : .labelColor
        let badge = NSImageView()
        badge.image = NSImage(
            systemSymbolName: entry.symbol, accessibilityDescription: entry.title)
        badge.contentTintColor = entry.destructive ? .systemRed : .secondaryLabelColor
        badge.symbolConfiguration = .init(pointSize: 13, weight: .regular)
        badge.translatesAutoresizingMaskIntoConstraints = false
        addSubview(badge)

        let label = NSTextField(labelWithString: entry.title)
        label.font = .systemFont(ofSize: 13)
        label.textColor = colour
        label.lineBreakMode = .byTruncatingTail
        label.maximumNumberOfLines = 1
        label.translatesAutoresizingMaskIntoConstraints = false
        addSubview(label)

        NSLayoutConstraint.activate([
            heightAnchor.constraint(equalToConstant: 36),
            badge.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 18),
            badge.centerYAnchor.constraint(equalTo: centerYAnchor),
            badge.widthAnchor.constraint(equalToConstant: 18),
            label.leadingAnchor.constraint(equalTo: badge.trailingAnchor, constant: 10),
            label.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])

        guard !entry.keys.isEmpty else { return }
        let caps = KeyCapView.row(entry.keys)
        addSubview(caps)
        NSLayoutConstraint.activate([
            caps.centerYAnchor.constraint(equalTo: centerYAnchor),
            // Inside the lozenge, which stops eight points short of the
            // panel's edge — keys drawn to the panel's edge instead sit half
            // out of the shape that is meant to be holding them.
            caps.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            label.trailingAnchor.constraint(
                lessThanOrEqualTo: caps.leadingAnchor, constant: -8),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    var isSelected = false {
        didSet {
            guard isSelected != oldValue else { return }
            lozenge.isHidden = !isSelected
            guard isSelected else { return }
            lozenge.set(
                cornerRadius: 11,
                tint: NSColor.white.withAlphaComponent(0.22),
                lightFill: NSColor.black.withAlphaComponent(0.08))
        }
    }

    override func layout() {
        super.layout()
        lozenge.frame = bounds.insetBy(dx: 8, dy: 1)
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for area in trackingAreas { removeTrackingArea(area) }
        addTrackingArea(NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .activeInActiveApp, .inVisibleRect],
            owner: self))
    }

    // The pointer moves the selection rather than drawing a second mark of
    // its own: in a list this short, two highlights is one too many.
    override func mouseEntered(with event: NSEvent) { onHover?() }

    /// Taken so the release comes back here — see the footer's chips.
    override func mouseDown(with event: NSEvent) {}

    override func mouseUp(with event: NSEvent) {
        guard bounds.contains(convert(event.locationInWindow, from: nil)) else { return }
        onClick?()
    }
}
