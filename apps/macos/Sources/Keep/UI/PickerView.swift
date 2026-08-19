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
    /// What the card holds; the card itself is glass around it.
    private let cardContent = NSView()
    private var card: NSView!
    /// "3 of 47", the way a browser counts.
    private let counter = NSTextField(labelWithString: "")

    private var all: [PickerModel.Item] = []
    private var shown: [PickerModel.Item] = []
    private var query = ""
    private var mode: PickerModel.Mode = .goTo
    private var matches: [String: PickerModel.Match] = [:]
    private var isSearching: Bool {
        if case .search = mode { return true }
        return false
    }

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        // A dimmed ground, so the terminal behind reads as "not now".
        layer?.backgroundColor = NSColor.black.withAlphaComponent(0.35).cgColor

        card = Glass.panel(cardContent, cornerRadius: 14)
        card.translatesAutoresizingMaskIntoConstraints = false
        addSubview(card)

        field.placeholderString = "go to…"
        field.font = .systemFont(ofSize: 15)
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.delegate = self
        field.translatesAutoresizingMaskIntoConstraints = false
        cardContent.addSubview(field)

        counter.font = .systemFont(ofSize: 11)
        counter.textColor = .tertiaryLabelColor
        counter.alignment = .right
        counter.translatesAutoresizingMaskIntoConstraints = false
        cardContent.addSubview(counter)

        let divider = NSBox()
        divider.boxType = .separator
        divider.translatesAutoresizingMaskIntoConstraints = false
        cardContent.addSubview(divider)

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
        cardContent.addSubview(scroll)

        preview.isEditable = false
        preview.isSelectable = false
        preview.drawsBackground = false
        preview.font = GhosttyApp.shared.terminalFont(size: 10)
        preview.textColor = .secondaryLabelColor
        preview.textContainerInset = NSSize(width: 10, height: 8)
        previewScroll.documentView = preview
        previewScroll.drawsBackground = false
        previewScroll.hasVerticalScroller = false
        previewScroll.translatesAutoresizingMaskIntoConstraints = false
        cardContent.addSubview(previewScroll)

        let previewDivider = NSBox()
        previewDivider.boxType = .separator
        previewDivider.translatesAutoresizingMaskIntoConstraints = false
        cardContent.addSubview(previewDivider)

        NSLayoutConstraint.activate([
            card.centerXAnchor.constraint(equalTo: centerXAnchor),
            card.topAnchor.constraint(equalTo: topAnchor, constant: 90),
            card.widthAnchor.constraint(equalTo: widthAnchor, multiplier: 0.66),
            card.heightAnchor.constraint(equalTo: heightAnchor, multiplier: 0.62),

            field.topAnchor.constraint(equalTo: cardContent.topAnchor, constant: 14),
            field.leadingAnchor.constraint(equalTo: cardContent.leadingAnchor, constant: 16),
            field.trailingAnchor.constraint(equalTo: counter.leadingAnchor, constant: -10),
            counter.centerYAnchor.constraint(equalTo: field.centerYAnchor),
            counter.trailingAnchor.constraint(equalTo: cardContent.trailingAnchor, constant: -16),

            divider.topAnchor.constraint(equalTo: field.bottomAnchor, constant: 12),
            divider.leadingAnchor.constraint(equalTo: cardContent.leadingAnchor),
            divider.trailingAnchor.constraint(equalTo: cardContent.trailingAnchor),

            scroll.topAnchor.constraint(equalTo: divider.bottomAnchor),
            scroll.leadingAnchor.constraint(equalTo: cardContent.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: cardContent.trailingAnchor),
            scroll.heightAnchor.constraint(equalTo: cardContent.heightAnchor, multiplier: 0.54),

            previewDivider.topAnchor.constraint(equalTo: scroll.bottomAnchor),
            previewDivider.leadingAnchor.constraint(equalTo: cardContent.leadingAnchor),
            previewDivider.trailingAnchor.constraint(equalTo: cardContent.trailingAnchor),

            previewScroll.topAnchor.constraint(equalTo: previewDivider.bottomAnchor),
            previewScroll.leadingAnchor.constraint(equalTo: cardContent.leadingAnchor),
            previewScroll.trailingAnchor.constraint(equalTo: cardContent.trailingAnchor),
            previewScroll.bottomAnchor.constraint(equalTo: cardContent.bottomAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    /// Clicking the dimmed ground outside the card dismisses.
    override func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        if !card.frame.contains(point) { onCancel?() }
    }

    /// The terminal's font may change with a config reload, and the preview
    /// shows terminal output — including glyphs only a Nerd Font supplies.
    private func adoptTerminalFont() {
        let font = GhosttyApp.shared.terminalFont(size: 10)
        if preview.font != font { preview.font = font }
    }

    func apply(_ model: PickerModel) {
        adoptTerminalFont()
        if mode != model.mode {
            mode = model.mode
            switch mode {
            case .goTo:
                field.placeholderString = "go to…"
            case .search(let global):
                field.placeholderString = global
                    ? "find everywhere…"
                    : "find in \(model.scopeLabel ?? "this pane")…"
            }
            field.stringValue = ""
            query = ""
        }
        matches = model.matches
        if all != model.items {
            all = model.items
            refilter(preservingSelection: true)
        }
        // A hit previews as the lines around it, which is what tells you
        // whether it is the place you meant; anything else previews as the
        // screen the daemon holds.
        if let id = model.previewOf, let match = model.matches[id],
           let item = model.items.first(where: { $0.id == id }) {
            let context = (match.before + [item.title] + match.after).joined(separator: "\n")
            if preview.string != context {
                preview.string = context
                preview.scrollToBeginningOfDocument(nil)
            }
        } else if preview.string != model.previewText {
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
        shown = isSearching ? all : Self.matches(all, query: query)
        table.reloadData()
        let index = previous.flatMap { id in shown.firstIndex { $0.id == id } } ?? 0
        select(row: shown.isEmpty ? -1 : index)
    }

    private func updateCounter() {
        guard isSearching else {
            counter.stringValue = ""
            return
        }
        if shown.isEmpty {
            counter.stringValue = query.isEmpty ? "" : "no matches"
        } else {
            counter.stringValue = "\(max(table.selectedRow, 0) + 1) of \(shown.count)"
        }
    }

    private func select(row: Int) {
        defer { updateCounter() }
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

    /// The matched text, marked the way a browser marks it: a tinted run
    /// inside the line rather than a differently coloured line.
    static func marked(_ text: String, range: Range<Int>, font: NSFont) -> NSAttributedString {
        let attributed = NSMutableAttributedString(
            string: text,
            attributes: [.font: font, .foregroundColor: NSColor.labelColor])
        let bytes = Array(text.utf8)
        guard range.lowerBound >= 0, range.upperBound <= bytes.count,
              range.lowerBound < range.upperBound,
              // Byte offsets from the daemon; NSAttributedString wants UTF-16.
              let lower = String(decoding: bytes[..<range.lowerBound], as: UTF8.self)
                .utf16.count as Int?,
              let length = String(decoding: bytes[range], as: UTF8.self).utf16.count as Int?,
              lower + length <= attributed.length
        else { return attributed }
        attributed.addAttributes(
            [
                .backgroundColor: NSColor.systemYellow.withAlphaComponent(0.35),
                .foregroundColor: NSColor.labelColor,
            ],
            range: NSRange(location: lower, length: length))
        return attributed
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
            let font = GhosttyApp.shared.terminalFont(size: 11)
            title.font = font
            if let match = matches[item.id] {
                title.attributedStringValue = Self.marked(
                    item.title, range: match.range, font: font)
            }
        } else {
            title.font = .systemFont(ofSize: 13)
        }
        title.lineBreakMode = .byTruncatingTail
        title.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(title)

        let detailText = matches[item.id].map { "\($0.group):\(item.detail)" } ?? item.detail
        let detail = NSTextField(labelWithString: detailText)
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
        updateCounter()
        onHighlight?(selectedItemID)
    }

    @objc private func chooseSelected() {
        if let id = selectedItemID { onChoose?(id) }
    }
}
