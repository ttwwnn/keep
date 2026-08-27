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
    /// Everything above the two halves: the field's row and the line under
    /// it. Named because the halves are sized as "the card, less this" —
    /// which is what keeps their height something the card hands down rather
    /// than something they ask it for.
    private static let headerHeight: CGFloat = 14 + 20 + 12
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

        // The window's own corner, measured off a capture of one: on the same
        // picture the window's rounding runs twice as far as this card's did
        // at fourteen. An overlay that sits inside a window and is rounded
        // less than it reads as a rectangle somebody softened, rather than as
        // a piece of the same thing.
        card = Glass.panel(cardContent, cornerRadius: 24)
        // Grey, over whatever is behind it. Glass refracts what it is over,
        // and what this is over is a terminal — so in a theme with a blue-dark
        // background the card came out blue, which is not a colour anything in
        // here chose.
        Glass.tint(
            card,
            // Dynamic, so the same grey does not turn a light theme's card
            // into a dark one.
            NSColor(name: nil) { appearance in
                appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
                    ? NSColor(white: 0.10, alpha: 0.55)
                    : NSColor(white: 0.94, alpha: 0.55)
            })
        card.translatesAutoresizingMaskIntoConstraints = false
        // Neither the card nor its insides may shrink to fit what is in them.
        // A container hugs its content at priority 750 by default, which is
        // above the card's own "be a fraction of the window" — so the card
        // came out the height of its header, forty-eight points of glass with
        // a list one point tall inside it.
        for view in [card!, cardContent] {
            view.setContentHuggingPriority(NSLayoutConstraint.Priority(1), for: .vertical)
            view.setContentHuggingPriority(NSLayoutConstraint.Priority(1), for: .horizontal)
        }
        addSubview(card)
        // And the insides follow the card, rather than the card following the
        // insides. Left to the panel they are given whatever size they ask
        // for — which, with everything in here laid out edge to edge, is the
        // height of the header and nothing more.
        cardContent.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            cardContent.leadingAnchor.constraint(equalTo: card.leadingAnchor),
            cardContent.trailingAnchor.constraint(equalTo: card.trailingAnchor),
            cardContent.topAnchor.constraint(equalTo: card.topAnchor),
            cardContent.bottomAnchor.constraint(equalTo: card.bottomAnchor),
        ])
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

        table.headerView = nil
        table.rowHeight = 34
        table.backgroundColor = .clear
        table.style = .plain
        // The row draws its own selection, inset and rounded like the
        // sidebar's; AppKit's own is a rectangle from edge to edge, which
        // inside a card with rounded corners reads as a different app's list
        // pasted into this one.
        table.selectionHighlightStyle = .none
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
        // Terminal lines are not prose: they are placed, and a line that
        // wraps is a line that has moved. Given a column of the card rather
        // than the whole width, wrapping would fold most of them — so the
        // container is left wide and the long ones simply run past the edge,
        // the way they do on the screen this is a picture of.
        preview.textContainer?.widthTracksTextView = false
        preview.textContainer?.size = NSSize(
            width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        preview.isHorizontallyResizable = true
        previewScroll.documentView = preview
        previewScroll.drawsBackground = false
        previewScroll.hasVerticalScroller = false
        previewScroll.translatesAutoresizingMaskIntoConstraints = false
        cardContent.addSubview(previewScroll)

        // The card is a fraction of the window, and it says so quietly.
        //
        // A window whose content is laid out with constraints will resize
        // itself to satisfy them: its own "stay where you are" is priority
        // 500, and anything above that wins. An overlay that covers a window
        // for as long as a keystroke has no business deciding how big that
        // window is — and when these were required, ⌘P shrank the window to
        // the height of this card's header and left the terminal a strip two
        // lines tall.
        let cardSize = [
            card.widthAnchor.constraint(equalTo: widthAnchor, multiplier: 0.66),
            card.heightAnchor.constraint(equalTo: heightAnchor, multiplier: 0.62),
            // Said of the list as well as of the card: the panel is required
            // to fit what is inside it, so a card told to be 62% of the
            // window while its insides ask for the height of a header is a
            // contradiction, and the window is what gives. Asking the list
            // for the same share, quietly, makes the two agree.
            scroll.heightAnchor.constraint(
                equalTo: heightAnchor, multiplier: 0.62, constant: -Self.headerHeight),
        ]
        for constraint in cardSize { constraint.priority = NSLayoutConstraint.Priority(499) }

        // The field's row is what the two halves start under; there is no
        // line between them any more, and a rule drawn edge to edge inside a
        // card is a seam in something that is meant to read as one surface.
        let header = field

        NSLayoutConstraint.activate(cardSize + [
            card.centerXAnchor.constraint(equalTo: centerXAnchor),
            card.topAnchor.constraint(equalTo: topAnchor, constant: 90),

            field.topAnchor.constraint(equalTo: cardContent.topAnchor, constant: 14),
            field.heightAnchor.constraint(equalToConstant: 20),
            field.leadingAnchor.constraint(equalTo: cardContent.leadingAnchor, constant: 16),
            field.trailingAnchor.constraint(equalTo: counter.leadingAnchor, constant: -10),
            counter.centerYAnchor.constraint(equalTo: field.centerYAnchor),
            counter.trailingAnchor.constraint(equalTo: cardContent.trailingAnchor, constant: -16),

            // The list on the left, what it is on the right. Side by side
            // rather than stacked: the preview is a piece of a terminal, and
            // a terminal is wide — given the bottom third of the card it had
            // room for six lines of an eighty-column screen and wrapped every
            // one of them. Beside the list it gets the full height of the
            // card, which is the shape the thing being shown actually has.
            //
            // Both halves take their height *from* the card rather than
            // giving the card one. Pinned to its bottom instead, they leave
            // the card's height to be worked out from what is inside it —
            // and since the card's own height is a fraction of the window's,
            // the only way left to satisfy that is to shrink the window. It
            // does, to the height of this header: press ⌘P and the terminal
            // becomes a strip two lines tall.
            scroll.topAnchor.constraint(equalTo: header.bottomAnchor),
            scroll.leadingAnchor.constraint(equalTo: cardContent.leadingAnchor),
            scroll.widthAnchor.constraint(equalTo: cardContent.widthAnchor, multiplier: 0.42),
            // Measured against the window, not against the card. Against the
            // card it is circular — the card is as tall as its insides, and
            // its insides are as tall as the card — and a circle with a
            // smallest answer settles on the smallest answer: a card the
            // height of its own header. Against the window there is nothing
            // to solve: the window is a size already, and everything here is
            // a share of it.
            scroll.heightAnchor.constraint(greaterThanOrEqualToConstant: 240),

            previewScroll.topAnchor.constraint(equalTo: header.bottomAnchor),
            previewScroll.leadingAnchor.constraint(equalTo: scroll.trailingAnchor, constant: 8),
            previewScroll.trailingAnchor.constraint(equalTo: cardContent.trailingAnchor),
            previewScroll.heightAnchor.constraint(equalTo: scroll.heightAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    /// The last geometry traced, so a relayout that changes nothing is silent.
    private var lastShape = ""
    /// What the preview last showed, kept because the drawn text is no longer
    /// the same string as the one it was made from.
    private var lastPreview = ""

    /// Where the two halves of the card ended up.
    ///
    /// The one question about this view that a screenshot answers and nothing
    /// else does — and a screenshot is not available when a tiling window
    /// manager has parked the window off the edge of the display, which from
    /// a test's point of view is most of the time. Asked after forcing the
    /// layout, because the frames are the answer and they are not settled
    /// until then.
    private func traceShape() {
        guard Trace.enabled else { return }
        layoutSubtreeIfNeeded()
        let shape = "view \(Int(frame.width))x\(Int(frame.height))"
            + " card \(Int(card.frame.width))x\(Int(card.frame.height))"
            + " content \(Int(cardContent.frame.width))x\(Int(cardContent.frame.height))"
            + " list \(Int(scroll.frame.width))x\(Int(scroll.frame.height))"
            + " preview \(Int(previewScroll.frame.minX)),\(Int(previewScroll.frame.minY))"
            + " \(Int(previewScroll.frame.width))x\(Int(previewScroll.frame.height))"
        guard shape != lastShape else { return }
        lastShape = shape
        Trace.log("picker", shape)
    }

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
        } else if lastPreview != model.previewText {
            lastPreview = model.previewText
            // Drawn with the colours it had on the screen it came from. The
            // daemon hands the preview over as the sequences it would repaint
            // a terminal with, so what the person is looking at is what they
            // would see if they went there.
            let terminal = GhosttyApp.shared.terminalPalette()
            preview.textStorage?.setAttributedString(
                TerminalText.attributed(
                    model.previewText,
                    font: GhosttyApp.shared.terminalFont(size: 10),
                    palette: terminal.colors,
                    foreground: terminal.foreground.withAlphaComponent(0.85)))
            preview.scrollToBeginningOfDocument(nil)
        }
        // Next turn: the card is measured after the window has laid it out,
        // not while the model is still being applied to it.
        if Trace.enabled { DispatchQueue.main.async { [weak self] in self?.traceShape() } }
    }

    /// The field owns the keyboard for as long as the picker is up.
    func takeFocus() {
        window?.makeFirstResponder(field)
    }

    /// Empty the field, for an overlay that is being opened.
    ///
    /// Called on the way in rather than on every model, and that distinction
    /// is the whole of it: typing refilters the list before it tells anyone
    /// what was typed, so the list's own selection comes back through a
    /// snapshot that still holds the *previous* query. A field that follows
    /// every snapshot therefore erases each letter as it is typed, which from
    /// the keyboard looks exactly like an overlay that will not accept input.
    func prepareForOpen() {
        guard !query.isEmpty else { return }
        query = ""
        field.stringValue = ""
        refilter(preservingSelection: false)
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

    /// ⌃J and ⌃K move down and up the list.
    ///
    /// Caught here rather than in `doCommandBy:`, where the arrows are
    /// handled, because by the time the field editor has had them they are no
    /// longer distinguishable from keys they share a meaning with: the system
    /// binds ⌃K to deleting the rest of the line and ⌃J to inserting a
    /// newline, which arrives as the same selector Return does — so a picker
    /// reading them there would open an item when asked to move down one.
    ///
    /// ⌃N and ⌃P need nothing: the system already turns them into the same
    /// two selectors the arrows arrive as.
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        let held = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        guard held == .control else { return super.performKeyEquivalent(with: event) }
        switch event.charactersIgnoringModifiers {
        case "j":
            select(row: min(table.selectedRow + 1, shown.count - 1))
            return true
        case "k":
            select(row: max(table.selectedRow - 1, 0))
            return true
        default:
            return super.performKeyEquivalent(with: event)
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
        guard let marks = marks(haystack, needle), let first = marks.first, let last = marks.last
        else { return nil }
        return (last - first) + first / 2
    }

    /// Where each letter of the query landed, which is the same walk the score
    /// is made of — kept rather than counted, so the row can show its work.
    ///
    /// A fuzzy list is a claim that these rows match what you typed, and the
    /// claim is unreadable until the letters it matched on are pointed at:
    /// three rows deep it stops being obvious why any of them is there, or
    /// why the one at the top is first.
    static func marks(_ haystack: String, _ needle: String) -> [Int]? {
        var index = haystack.startIndex
        var found: [Int] = []
        for character in needle {
            guard let at = haystack[index...].firstIndex(of: character) else { return nil }
            found.append(haystack.distance(from: haystack.startIndex, to: at))
            index = haystack.index(after: at)
        }
        return found
    }

    /// The same text with the matched letters lit.
    static func lit(
        _ text: String, marks: [Int], font: NSFont, colour: NSColor
    ) -> NSAttributedString {
        let attributed = NSMutableAttributedString(
            string: text, attributes: [.font: font, .foregroundColor: NSColor.labelColor])
        let bold = NSFontManager.shared.convert(font, toHaveTrait: .boldFontMask)
        // The marks are counted in characters and the attributes are applied
        // in UTF-16, which are the same number only until somebody's directory
        // has an emoji in it.
        let utf16 = Array(text.utf16)
        var offsets: [Int] = []
        var cursor = 0
        for character in text {
            offsets.append(cursor)
            cursor += String(character).utf16.count
        }
        for mark in marks where mark < offsets.count {
            let start = offsets[mark]
            let length = mark + 1 < offsets.count ? offsets[mark + 1] - start : utf16.count - start
            guard length > 0 else { continue }
            attributed.addAttributes(
                [.foregroundColor: colour, .font: bold],
                range: NSRange(location: start, length: length))
        }
        return attributed
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

/// A row that lights up the way the sidebar's do.
///
/// The same shape, the same inset, and the same glass — a list of places to go
/// inside a window whose other list of places to go looks like this should not
/// have to be told twice what a chosen row looks like.
private final class PickerRow: NSTableRowView {
    private let lozenge = Glass.lozenge(cornerRadius: 12) ?? NSView()

    override init(frame: NSRect) {
        super.init(frame: frame)
        lozenge.wantsLayer = true
        lozenge.layer?.cornerCurve = .continuous
        lozenge.isHidden = true
        addSubview(lozenge, positioned: .below, relativeTo: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    override func layout() {
        super.layout()
        lozenge.frame = bounds.insetBy(dx: 6, dy: 2)
        Glass.setCornerRadius(lozenge, 12)
        if !Glass.isAvailable {
            lozenge.layer?.cornerRadius = 12
            lozenge.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.10).cgColor
        }
    }

    override var isSelected: Bool {
        didSet {
            guard isSelected != oldValue else { return }
            lozenge.isHidden = !isSelected
            if isSelected, Glass.isAvailable {
                Glass.tint(lozenge, NSColor.white.withAlphaComponent(0.30))
            }
        }
    }

    // Nothing else may paint over it: `.none` stops the standard highlight,
    // and these stop the separator and the alternating background that a
    // table draws underneath rows on its own.
    override func drawSelection(in dirtyRect: NSRect) {}
    override func drawSeparator(in dirtyRect: NSRect) {}
    override func drawBackground(in dirtyRect: NSRect) {}
}

extension PickerView: NSTableViewDataSource, NSTableViewDelegate {
    func numberOfRows(in tableView: NSTableView) -> Int { shown.count }

    func tableView(_ tableView: NSTableView, rowViewForRow row: Int) -> NSTableRowView? {
        PickerRow()
    }

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
            let font = NSFont.systemFont(ofSize: 13)
            title.font = font
            // Which letters put this row here. The query is matched against
            // the title first and the path second — the same order the score
            // tries them in — so what is lit is what was actually matched on,
            // not a guess made afterwards.
            let needle = query.lowercased().filter { !$0.isWhitespace }
            if !needle.isEmpty, let marks = Self.marks(item.title.lowercased(), needle) {
                title.attributedStringValue = Self.lit(
                    item.title, marks: marks, font: font, colour: .controlAccentColor)
            }
        }
        title.lineBreakMode = .byTruncatingTail
        title.maximumNumberOfLines = 1
        title.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(title)

        let detailText = matches[item.id].map { "\($0.group):\(item.detail)" } ?? item.detail
        let detail = NSTextField(labelWithString: detailText)
        let detailFont = NSFont.systemFont(ofSize: 11)
        detail.font = detailFont
        detail.textColor = .secondaryLabelColor
        // And in the path, when that is where the match was found — a row
        // that is here because of its directory says so there.
        let needle = query.lowercased().filter { !$0.isWhitespace }
        if !needle.isEmpty, Self.marks(item.title.lowercased(), needle) == nil,
           let marks = Self.marks(detailText.lowercased(), needle) {
            detail.attributedStringValue = Self.lit(
                detailText, marks: marks, font: detailFont, colour: .controlAccentColor)
        }
        detail.lineBreakMode = .byTruncatingHead
        // One line, and the head is what gives: a long path is identified by
        // its end, and a row that wraps is a row that no longer fits between
        // the two beside it.
        detail.maximumNumberOfLines = 1
        detail.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(detail)

        // Two kinds of row, and you should not have to read the text to tell
        // them apart: a terminal you are going back to, or a folder you are
        // starting something in. Busy terminals wear the accent colour, which
        // is the same thing the sidebar dot says.
        let badge = NSImageView()
        switch item.kind {
        case .running:
            // Filled means busy, and that is all it means. Painted with the
            // accent colour it was a blue square at this size — read as a
            // swatch, or as something bleeding through from behind, rather
            // than as "there is a program running in here".
            badge.image = NSImage(
                systemSymbolName: item.busy ? "terminal.fill" : "terminal",
                accessibilityDescription: item.busy ? "running" : "terminal")
            badge.contentTintColor = item.busy ? .labelColor : .secondaryLabelColor
        case .destination:
            badge.image = NSImage(
                systemSymbolName: "folder", accessibilityDescription: "folder")
            badge.contentTintColor = .tertiaryLabelColor
        case .hit:
            badge.image = NSImage(
                systemSymbolName: "text.magnifyingglass", accessibilityDescription: "match")
            badge.contentTintColor = .secondaryLabelColor
        }
        // Bigger than the text beside it, not smaller: this is the one thing
        // in the row you read without reading — terminal or folder, decided
        // before the eye reaches the name — and at twelve points it was
        // punctuation next to a thirteen-point title.
        badge.symbolConfiguration = .init(pointSize: 15, weight: .regular)
        badge.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(badge)

        NSLayoutConstraint.activate([
            badge.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 18),
            badge.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            badge.widthAnchor.constraint(equalToConstant: 20),
            title.leadingAnchor.constraint(equalTo: badge.trailingAnchor, constant: 8),
            title.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            title.trailingAnchor.constraint(
                lessThanOrEqualTo: detail.leadingAnchor, constant: -10),
            detail.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -18),
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
