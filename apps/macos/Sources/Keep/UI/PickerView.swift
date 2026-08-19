import AppKit

/// The "go to" overlay: type, land somewhere.
///
/// Flat on purpose. You do not pick a workspace and then a tab — you pick the
/// thing and you are in it. Everything running is one row, most recently
/// visited first, and below it the directories worth starting something new
/// in. A preview shows what a row actually is, drawn from the screen the
/// daemon is already keeping.
///
/// It draws itself rather than hosting SwiftUI: this is a keyboard surface
/// that must never take focus away from anything by accident, and owning the
/// responder story outright is simpler than negotiating for it.
@MainActor
final class PickerView: NSView {
    var onFilter: ((String) -> Void)?
    var onHighlight: ((String?) -> Void)?
    var onChoose: ((String) -> Void)?
    var onDismissItem: ((String) -> Void)?
    var onCancel: (() -> Void)?

    private let field = NSTextField()
    private let scroll = NSScrollView()
    private let table = NSTableView()
    private let preview = NSTextView()
    private let previewScroll = NSScrollView()
    private let card = NSVisualEffectView()

    private var all: [PickerModel.Item] = []
    private var shown: [PickerModel.Item] = []
    private var query = ""
    private var mode: PickerModel.Mode = .goTo

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        // A dimmed ground, so the terminal behind reads as "not now".
        layer?.backgroundColor = NSColor.black.withAlphaComponent(0.35).cgColor

        card.material = .hudWindow
        card.blendingMode = .withinWindow
        card.state = .active
        card.wantsLayer = true
        card.layer?.cornerRadius = 10
        card.layer?.borderWidth = 1
        card.layer?.borderColor = NSColor.white.withAlphaComponent(0.12).cgColor
        card.translatesAutoresizingMaskIntoConstraints = false
        addSubview(card)

        field.placeholderString = "go to…"
        field.font = .systemFont(ofSize: 15)
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.delegate = self
        field.translatesAutoresizingMaskIntoConstraints = false
        card.addSubview(field)

        let divider = NSBox()
        divider.boxType = .separator
        divider.translatesAutoresizingMaskIntoConstraints = false
        card.addSubview(divider)

        table.headerView = nil
        table.rowHeight = 34
        table.backgroundColor = .clear
        table.style = .plain
        table.selectionHighlightStyle = .regular
        table.intercellSpacing = NSSize(width: 0, height: 0)
        table.dataSource = self
        table.delegate = self
        table.target = self
        table.doubleAction = #selector(chooseSelected)
        table.addTableColumn(NSTableColumn(identifier: .init("row")))
        scroll.documentView = table
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.translatesAutoresizingMaskIntoConstraints = false
        card.addSubview(scroll)

        preview.isEditable = false
        preview.isSelectable = false
        preview.drawsBackground = false
        preview.font = .monospacedSystemFont(ofSize: 10, weight: .regular)
        preview.textColor = .secondaryLabelColor
        preview.textContainerInset = NSSize(width: 10, height: 8)
        previewScroll.documentView = preview
        previewScroll.drawsBackground = false
        previewScroll.hasVerticalScroller = false
        previewScroll.translatesAutoresizingMaskIntoConstraints = false
        card.addSubview(previewScroll)

        let previewDivider = NSBox()
        previewDivider.boxType = .separator
        previewDivider.translatesAutoresizingMaskIntoConstraints = false
        card.addSubview(previewDivider)

        NSLayoutConstraint.activate([
            card.centerXAnchor.constraint(equalTo: centerXAnchor),
            card.topAnchor.constraint(equalTo: topAnchor, constant: 90),
            card.widthAnchor.constraint(equalTo: widthAnchor, multiplier: 0.66),
            card.heightAnchor.constraint(equalTo: heightAnchor, multiplier: 0.62),

            field.topAnchor.constraint(equalTo: card.topAnchor, constant: 14),
            field.leadingAnchor.constraint(equalTo: card.leadingAnchor, constant: 16),
            field.trailingAnchor.constraint(equalTo: card.trailingAnchor, constant: -16),

            divider.topAnchor.constraint(equalTo: field.bottomAnchor, constant: 12),
            divider.leadingAnchor.constraint(equalTo: card.leadingAnchor),
            divider.trailingAnchor.constraint(equalTo: card.trailingAnchor),

            scroll.topAnchor.constraint(equalTo: divider.bottomAnchor),
            scroll.leadingAnchor.constraint(equalTo: card.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: card.trailingAnchor),
            scroll.heightAnchor.constraint(equalTo: card.heightAnchor, multiplier: 0.54),

            previewDivider.topAnchor.constraint(equalTo: scroll.bottomAnchor),
            previewDivider.leadingAnchor.constraint(equalTo: card.leadingAnchor),
            previewDivider.trailingAnchor.constraint(equalTo: card.trailingAnchor),

            previewScroll.topAnchor.constraint(equalTo: previewDivider.bottomAnchor),
            previewScroll.leadingAnchor.constraint(equalTo: card.leadingAnchor),
            previewScroll.trailingAnchor.constraint(equalTo: card.trailingAnchor),
            previewScroll.bottomAnchor.constraint(equalTo: card.bottomAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    /// Clicking the dimmed ground outside the card dismisses.
    override func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        if !card.frame.contains(point) { onCancel?() }
    }

    func apply(_ model: PickerModel) {
        if mode != model.mode {
            mode = model.mode
            field.placeholderString = mode == .search ? "search history…" : "go to…"
            field.stringValue = ""
            query = ""
        }
        if all != model.items {
            all = model.items
            refilter(preservingSelection: true)
        }
        if preview.string != model.previewText {
            preview.string = model.previewText
            preview.scrollToBeginningOfDocument(nil)
        }
    }

    /// The field owns the keyboard for as long as the picker is up.
    func takeFocus() {
        window?.makeFirstResponder(field)
    }

    var selectedItemID: String? {
        table.selectedRow >= 0 && table.selectedRow < shown.count
            ? shown[table.selectedRow].id : nil
    }

    // MARK: - filtering

    private func refilter(preservingSelection: Bool) {
        let previous = preservingSelection ? selectedItemID : nil
        // In search the daemon has already decided what matches; filtering
        // its answer again with a different rule would hide real hits.
        shown = mode == .search ? all : Self.matches(all, query: query)
        table.reloadData()
        let index = previous.flatMap { id in shown.firstIndex { $0.id == id } } ?? 0
        select(row: shown.isEmpty ? -1 : index)
    }

    private func select(row: Int) {
        guard row >= 0, row < shown.count else {
            table.deselectAll(nil)
            onHighlight?(nil)
            return
        }
        // The table's own delegate reports the change; announcing it here as
        // well asked the daemon for the same preview twice.
        table.selectRowIndexes([row], byExtendingSelection: false)
        table.scrollRowToVisible(row)
    }

    /// Subsequence matching, the way every fuzzy finder behaves: the letters
    /// you type must appear in order, and rows where they appear closer
    /// together rank higher.
    static func matches(_ items: [PickerModel.Item], query: String) -> [PickerModel.Item] {
        let needle = query.lowercased().filter { !$0.isWhitespace }
        guard !needle.isEmpty else { return items }
        return items.compactMap { item -> (PickerModel.Item, Int)? in
            guard let score = score(item.title.lowercased(), needle)
                ?? score(item.detail.lowercased(), needle)
            else { return nil }
            return (item, score)
        }
        .sorted { $0.1 < $1.1 }
        .map(\.0)
    }

    /// Lower is better: the span the match occupies, plus where it starts.
    private static func score(_ haystack: String, _ needle: String) -> Int? {
        var index = haystack.startIndex
        var first: Int?
        var last = 0
        var position = 0
        for character in needle {
            guard let found = haystack[index...].firstIndex(of: character) else { return nil }
            position = haystack.distance(from: haystack.startIndex, to: found)
            if first == nil { first = position }
            last = position
            index = haystack.index(after: found)
        }
        return (last - (first ?? 0)) + (first ?? 0) / 2
    }
}

// MARK: - keyboard

extension PickerView: NSTextFieldDelegate {
    func controlTextDidChange(_ notification: Notification) {
        query = field.stringValue
        refilter(preservingSelection: false)
        onFilter?(query)
    }

    func control(
        _ control: NSControl, textView: NSTextView, doCommandBy selector: Selector
    ) -> Bool {
        switch selector {
        case #selector(NSResponder.moveDown(_:)):
            select(row: min(table.selectedRow + 1, shown.count - 1))
            return true
        case #selector(NSResponder.moveUp(_:)):
            select(row: max(table.selectedRow - 1, 0))
            return true
        case #selector(NSResponder.insertNewline(_:)):
            if let id = selectedItemID { onChoose?(id) }
            return true
        case #selector(NSResponder.cancelOperation(_:)):
            onCancel?()
            return true
        case #selector(NSResponder.deleteForward(_:)):
            // ⌃D on a row ends what it points at, as in the picker this
            // replaces. The field is empty-handed here, so it is unambiguous.
            if let id = selectedItemID { onDismissItem?(id) }
            return true
        default:
            return false
        }
    }
}

// MARK: - rows

extension PickerView: NSTableViewDataSource, NSTableViewDelegate {
    func numberOfRows(in tableView: NSTableView) -> Int { shown.count }

    func tableView(
        _ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int
    ) -> NSView? {
        let item = shown[row]
        let cell = NSView()

        let title = NSTextField(labelWithString: item.title)
        if case .hit = item.kind {
            title.font = .monospacedSystemFont(ofSize: 11, weight: .regular)
        } else {
            title.font = .systemFont(ofSize: 13)
        }
        title.lineBreakMode = .byTruncatingTail
        title.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(title)

        let detail = NSTextField(labelWithString: item.detail)
        detail.font = .systemFont(ofSize: 11)
        detail.textColor = .secondaryLabelColor
        detail.lineBreakMode = .byTruncatingHead
        detail.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(detail)

        // Two kinds of row, and you should not have to read the text to tell
        // them apart: a terminal you are going back to, or a folder you are
        // starting something in. Busy terminals wear the accent colour, which
        // is the same thing the sidebar dot says.
        let badge = NSImageView()
        switch item.kind {
        case .running:
            badge.image = NSImage(
                systemSymbolName: item.busy ? "terminal.fill" : "terminal",
                accessibilityDescription: item.busy ? "running" : "terminal")
            badge.contentTintColor = item.busy ? .controlAccentColor : .secondaryLabelColor
        case .destination:
            badge.image = NSImage(
                systemSymbolName: "folder", accessibilityDescription: "folder")
            badge.contentTintColor = .tertiaryLabelColor
        case .hit:
            badge.image = NSImage(
                systemSymbolName: "text.magnifyingglass", accessibilityDescription: "match")
            badge.contentTintColor = .secondaryLabelColor
        }
        badge.symbolConfiguration = .init(pointSize: 12, weight: .regular)
        badge.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(badge)

        NSLayoutConstraint.activate([
            badge.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 14),
            badge.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            badge.widthAnchor.constraint(equalToConstant: 16),
            title.leadingAnchor.constraint(equalTo: badge.trailingAnchor, constant: 6),
            title.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            title.trailingAnchor.constraint(
                lessThanOrEqualTo: detail.leadingAnchor, constant: -10),
            detail.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -14),
            detail.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
        ])
        return cell
    }

    func tableViewSelectionDidChange(_ notification: Notification) {
        onHighlight?(selectedItemID)
    }

    @objc private func chooseSelected() {
        if let id = selectedItemID { onChoose?(id) }
    }
}
