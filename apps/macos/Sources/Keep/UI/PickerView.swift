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
    /// Anything the actions panel offers, on the row it was opened over.
    var onAction: ((String, PickerAction) -> Void)?
    /// Escape, in a catalog the palette opened: back one step.
    var onBack: (() -> Void)?
    var onCancel: (() -> Void)?

    private let field = NSTextField()
    private let scroll = NSScrollView()
    /// The list's frame, which is what fades. A scroll view manages its own
    /// layer and quietly loses a mask put on it.
    private let listBox = FadingBox()
    private let table = PickerTable()
    private let preview = NSTextView()
    private let previewScroll = NSScrollView()
    /// What the card holds; the card itself is glass around it.
    private let cardContent = NSView()
    /// The lit rim around the card.
    ///
    /// The material path draws a flat hairline — the same white at every
    /// point of the rounding — which is a rectangle somebody outlined, not a
    /// piece of glass. Glass has a light coming from somewhere: the top edge
    /// catches it, the sides fall away, and the bottom picks up a little back
    /// off whatever is under it. Since the card cannot be real glass here
    /// (that is what fetched the wallpaper), the rim is drawn.
    private let edge = EdgeView(cornerRadius: 24)
    /// The card's own ground, in the terminal's colour.
    ///
    /// Glass refracts whatever the window server has under it, and under a
    /// window whose `background-opacity` is less than one that is the
    /// desktop: the card came out wearing the wallpaper. `KeepWindow` already
    /// meets this in the titlebar strip, where the Metal surface has not
    /// started yet and the clear window shows through — and answers it the
    /// same way, by painting the colour the terminal would have painted.
    ///
    /// Opaque, unlike that strip. The strip is meant to read as continuous
    /// with a terminal that is itself see-through; this is the floor of an
    /// overlay, and a floor you can see the desktop through is not one.
    private let backdrop = NSView()
    /// Everything above the two halves: the field's row and the line under
    /// it. Named because the halves are sized as "the card, less this" —
    /// which is what keeps their height something the card hands down rather
    /// than something they ask it for.
    private static let headerHeight: CGFloat = 20 + 24 + 18
    /// Where the list's first row sits when nothing is scrolled — just under
    /// the field, not a header's height below it. The fade reaches full
    /// strength at exactly this line, so the first row is whole and the one
    /// behind the field is not.
    private static let listTopInset: CGFloat = 20 + 24 + 24
    private var card: NSView!
    /// "3 of 47", the way a browser counts.
    private let counter = NSTextField(labelWithString: "")
    /// What Return does, and where the rest of it is.
    private let footer = PickerFooter()
    /// Up only while it is open: it takes the keyboard for as long as it
    /// exists, and a hidden view that has taken the keyboard is a picker that
    /// will not accept typing.
    private var actions: PickerActionsPanel?

    private var all: [PickerModel.Item] = []
    /// What the table draws, headings included.
    ///
    /// Headings live here rather than in the model because they are a fact
    /// about this drawing of the list and not about the list: search mode has
    /// none, and one whose whole section has been filtered away must go with
    /// it. Both fall out of rebuilding them after every filter.
    private var shown: [Row] = []
    private var query = ""
    private var mode: PickerModel.Mode = .goTo
    private var matches: [String: PickerModel.Match] = [:]
    /// Which column each row was matched on, decided while filtering.
    ///
    /// Kept rather than worked out again in the cell: with three columns to
    /// try, a cell that re-runs the walk can light a column the score never
    /// looked at, and claim the row is here for a reason it is not.
    private var litColumns: [String: Lit] = [:]
    private var isSearching: Bool {
        if case .search = mode { return true }
        return false
    }

    enum Row: Equatable {
        case heading(String)
        case item(PickerModel.Item)

        var item: PickerModel.Item? {
            if case .item(let item) = self { return item }
            return nil
        }
    }

    struct Lit: Equatable {
        enum Column { case context, title, detail }
        let column: Column
        let marks: [Int]
    }

    /// How far the card may grow before it stops being a panel.
    ///
    /// Wide enough for the two halves — a list of paths on the left and a
    /// screen's worth of terminal on the right — and no wider. Two thirds of
    /// an ultra-wide display is thirteen hundred points of card holding a
    /// list of forty-character rows: a wall with a list painted on one end.
    static let widest: CGFloat = 860
    static let tallest: CGFloat = 560

    /// Dense, per the house ladder, and one line of text per row. Not the
    /// sidebar's 24: these rows carry a selection lozenge inset by two and
    /// are aimed at with a pointer as often as with the arrows.
    static let rowHeight: CGFloat = 30
    /// The extra four points are the gap above the word, not around it.
    static let headingHeight: CGFloat = 34
    /// Wide enough for `777leads/api` in the terminal face, narrow enough to
    /// leave a title room. What overflows truncates rather than pushing the
    /// column along and taking every row's alignment with it.
    static let contextWidth: CGFloat = 132

    /// How long ago, in as few characters as will carry it.
    ///
    /// No words and no formatter: this is a column that has to align, and it
    /// sits beside identifiers rather than inside a sentence.
    static func recency(_ date: Date?, now: Date = Date()) -> String {
        guard let date else { return "" }
        let seconds = now.timeIntervalSince(date)
        guard seconds >= 0 else { return "now" }
        if seconds < 60 { return "now" }
        if seconds < 3600 { return "\(Int(seconds / 60))m" }
        if seconds < 86_400 { return "\(Int(seconds / 3600))h" }
        if seconds < 604_800 { return "\(Int(seconds / 86_400))d" }
        return "\(Int(seconds / 604_800))w"
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
        // Built in layers rather than handed to Liquid Glass, which was
        // tried and settles the question: `NSGlassEffectView` samples what
        // the window server has *behind the window*, not what this app has
        // put inside it. Holding the window opaque does not change that —
        // what is behind the window is the desktop either way, and the card
        // came back wearing it. So the stack is ours, bottom to top: a plate
        // that holds the colour and casts the shadow, a material that blurs
        // the terminal, a tint that says which colour the blur is under, and
        // a rim.
        //
        // The blur is the window server's rather than a `CIGaussianBlur` in
        // `backgroundFilters`. Same picture, and the filter is redrawn every
        // frame the terminal paints — which for a terminal is most of them.
        card = Glass.panel(cardContent, cornerRadius: 24, sampling: .window)
        card.translatesAutoresizingMaskIntoConstraints = false
        backdrop.wantsLayer = true
        backdrop.layer?.cornerCurve = .continuous
        backdrop.layer?.cornerRadius = 24
        backdrop.translatesAutoresizingMaskIntoConstraints = false
        // The card lifts off the terminal rather than lying on it. Cast from
        // the plate, which is the one view in the stack whose frame is the
        // card's and whose layer nothing else is masking.
        backdrop.layer?.shadowColor = NSColor.black.cgColor
        backdrop.layer?.shadowOpacity = 0.5
        backdrop.layer?.shadowRadius = 36
        backdrop.layer?.shadowOffset = CGSize(width: 0, height: -14)
        addSubview(backdrop)
        adoptTerminalBackground()
        // A theme change repaints this the way it repaints the chrome.
        NotificationCenter.default.addObserver(
            forName: GhosttyApp.backgroundDidChange, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.adoptTerminalBackground() }
        }
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
        edge.translatesAutoresizingMaskIntoConstraints = false
        addSubview(edge)
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
        field.font = .systemFont(ofSize: 16)
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.delegate = self
        field.translatesAutoresizingMaskIntoConstraints = false
        cardContent.addSubview(field)

        counter.font = .systemFont(ofSize: 12)
        counter.textColor = .tertiaryLabelColor
        counter.alignment = .right
        counter.translatesAutoresizingMaskIntoConstraints = false
        cardContent.addSubview(counter)

        table.headerView = nil
        table.rowHeight = 38
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
        // The right button selects what it is over and then asks the same
        // question ⌘K asks. Selecting first is the whole of it: a menu that
        // acts on a row other than the one under the pointer is a menu that
        // does something else than it was asked to.
        table.onRightClick = { [weak self] row in
            guard let self else { return }
            if row >= 0, row < self.shown.count, self.shown[row].item != nil {
                self.select(row: row)
            }
            self.openActions()
        }
        scroll.documentView = table
        // Gone by the field's own line and fully back by the time the list
        // proper begins: the clear end of the gradient sits at a little over
        // half of this, which is just below where the field's text sits.
        listBox.fadeTop = Self.listTopInset
        listBox.fadeBottom = 28
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        // The list occupies the whole card and is held clear of the header by
        // an inset, rather than starting below it. That is what lets a row
        // scroll *under* the field instead of stopping at it — and what is
        // over it then blurs it, which is the whole effect.
        scroll.automaticallyAdjustsContentInsets = false
        scroll.contentInsets = NSEdgeInsets(
            top: Self.listTopInset, left: 0, bottom: 0, right: 0)
        // The scroller is not inset with it: given the same top, its track
        // starts a third of the way down the card and the knob is a stub.
        scroll.scrollerInsets = NSEdgeInsets(top: 0, left: 0, bottom: 0, right: 0)
        scroll.translatesAutoresizingMaskIntoConstraints = false
        listBox.translatesAutoresizingMaskIntoConstraints = false
        listBox.addSubview(scroll)
        cardContent.addSubview(listBox)

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

        footer.translatesAutoresizingMaskIntoConstraints = false
        footer.onOpen = { [weak self] in
            guard let id = self?.selectedItemID else { return }
            self?.onChoose?(id)
        }
        footer.onActions = { [weak self] in self?.toggleActions() }
        // On the card, not in it. Glass refracts what is *behind* it, and
        // inside the card's own glass there is nothing behind it but the
        // card's content view — so a pane laid there came out flat, a grey
        // rectangle with a corner radius. Out here its backdrop is the card
        // itself, which is what the reference has beneath its chip.
        addSubview(footer)

        // Above the list, which fades out beneath them.
        cardContent.addSubview(field)
        cardContent.addSubview(counter)

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
            scroll.heightAnchor.constraint(equalTo: heightAnchor, multiplier: 0.62),
        ]
        // Required, unlike the shares above: a cap that a window could argue
        // with is not a cap. They are inequalities, so there is nothing for
        // the window to argue with — it stops growing and nothing else moves.
        let cardCaps = [
            card.widthAnchor.constraint(lessThanOrEqualToConstant: Self.widest),
            card.heightAnchor.constraint(lessThanOrEqualToConstant: Self.tallest),
            scroll.heightAnchor.constraint(lessThanOrEqualToConstant: Self.tallest),
        ]
        for constraint in cardSize { constraint.priority = NSLayoutConstraint.Priority(499) }

        // The field's row is what the two halves start under; there is no
        // line between them any more, and a rule drawn edge to edge inside a
        // card is a seam in something that is meant to read as one surface.
        let header = field

        NSLayoutConstraint.activate(cardSize + cardCaps + [
            card.centerXAnchor.constraint(equalTo: centerXAnchor),
            card.topAnchor.constraint(equalTo: topAnchor, constant: 90),

            // Exactly the card, so nothing of it is ever visible on its own.
            backdrop.leadingAnchor.constraint(equalTo: card.leadingAnchor),
            backdrop.trailingAnchor.constraint(equalTo: card.trailingAnchor),
            backdrop.topAnchor.constraint(equalTo: card.topAnchor),
            backdrop.bottomAnchor.constraint(equalTo: card.bottomAnchor),
            edge.leadingAnchor.constraint(equalTo: card.leadingAnchor),
            edge.trailingAnchor.constraint(equalTo: card.trailingAnchor),
            edge.topAnchor.constraint(equalTo: card.topAnchor),
            edge.bottomAnchor.constraint(equalTo: card.bottomAnchor),

            field.topAnchor.constraint(equalTo: cardContent.topAnchor, constant: 20),
            field.heightAnchor.constraint(equalToConstant: 24),
            field.leadingAnchor.constraint(equalTo: cardContent.leadingAnchor, constant: 20),
            field.trailingAnchor.constraint(equalTo: counter.leadingAnchor, constant: -10),
            counter.centerYAnchor.constraint(equalTo: field.centerYAnchor),
            counter.trailingAnchor.constraint(equalTo: cardContent.trailingAnchor, constant: -20),

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
            listBox.topAnchor.constraint(equalTo: cardContent.topAnchor),
            listBox.leadingAnchor.constraint(equalTo: cardContent.leadingAnchor),
            listBox.widthAnchor.constraint(equalTo: cardContent.widthAnchor, multiplier: 0.42),
            listBox.heightAnchor.constraint(equalTo: scroll.heightAnchor),

            scroll.topAnchor.constraint(equalTo: listBox.topAnchor),
            scroll.leadingAnchor.constraint(equalTo: listBox.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: listBox.trailingAnchor),
            // Measured against the window, not against the card. Against the
            // card it is circular — the card is as tall as its insides, and
            // its insides are as tall as the card — and a circle with a
            // smallest answer settles on the smallest answer: a card the
            // height of its own header. Against the window there is nothing
            // to solve: the window is a size already, and everything here is
            // a share of it.
            scroll.heightAnchor.constraint(greaterThanOrEqualToConstant: 240),

            previewScroll.topAnchor.constraint(equalTo: header.bottomAnchor, constant: 18),
            previewScroll.leadingAnchor.constraint(equalTo: listBox.trailingAnchor, constant: 8),
            previewScroll.trailingAnchor.constraint(equalTo: cardContent.trailingAnchor),
            previewScroll.bottomAnchor.constraint(
                equalTo: footer.topAnchor, constant: -6),

            // In the corner, floating, sized by what is in it. Pinned to two
            // edges and nothing else: the card's height is a share of the
            // window's, and a footer that also pushed on the card would be
            // the one thing in here deciding how tall it is.
            footer.trailingAnchor.constraint(
                equalTo: card.trailingAnchor, constant: -PickerFooter.margin),
            footer.bottomAnchor.constraint(
                equalTo: card.bottomAnchor, constant: -PickerFooter.margin),
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

    override func layout() {
        super.layout()
        // Given explicitly, so the shadow is cast by the card's shape rather
        // than worked out from the plate's contents — which are half
        // transparent, and would cast half a shadow.
        backdrop.layer?.shadowPath = CGPath(
            roundedRect: backdrop.bounds, cornerWidth: 24, cornerHeight: 24,
            transform: nil)
    }

    /// Clicking the dimmed ground outside the card dismisses.
    override func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        // The panel is dismissed by the click that lands outside it, and that
        // click does nothing else: a stray tap should not also close the
        // picker underneath.
        if let panel = actions {
            let inPanel = panel.convert(event.locationInWindow, from: nil)
            if !panel.bounds.contains(inPanel) { closeActions() }
            return
        }
        if !card.frame.contains(point) { onCancel?() }
    }

    /// The colour the terminal would have painted here, at full strength.
    ///
    /// Falls back to the window's own ground rather than to a named grey: a
    /// terminal that has not reported its background yet is a terminal we
    /// know nothing about, and a guess would be a rectangle of the wrong
    /// colour behind glass rather than no rectangle at all.
    private func adoptTerminalBackground() {
        let ground = (GhosttyApp.shared.terminalBackground ?? .windowBackgroundColor)
            .withAlphaComponent(1)
        // Short of opaque, or there is nothing left to blur. The material
        // over this plate samples what is under it, and a plate at full
        // strength gives it a flat colour to blur — which is a card that
        // looks painted rather than one you can see the terminal moving
        // behind. What gets through with it is the window's own see-through
        // fraction of the desktop, which at this strength is two parts in a
        // hundred and behind a blur.
        //
        // Decided from the colour rather than from the system's appearance:
        // a light theme in a dark system is a light card, and it is the
        // theme this is a card of.
        let light = (ground.usingColorSpace(.sRGB)?.brightnessComponent ?? 0) >= 0.5
        let colour = Self.deepened(ground)
        // Barely there. The plate used to be the only thing standing between
        // the card and the desktop, and had to be thick enough to hide it;
        // now the window itself is opaque for as long as the overlay is up,
        // so the plate is only a bed for the colour and a shape for the
        // shadow — and every point of it is a point of blurred terminal that
        // does not reach the eye.
        backdrop.layer?.backgroundColor = colour
            .withAlphaComponent(light ? 0.45 : 0.22).cgColor
        // And the same colour again inside the card, over the material's
        // blur. The material paints a light grey of its own on top of
        // whatever it blurred, which is what made the card read as a pale
        // panel on a black terminal — this is the colour the card is
        // supposed to be, laid over that.
        Glass.tint(card, colour.withAlphaComponent(light ? 0.72 : 0.55))
    }

    /// The terminal's own colour, a step deeper than it.
    ///
    /// The same arithmetic `KeepWindow.recessed` uses to put the sidebar a
    /// step behind the terminal — a factor on the channels, so the hue is
    /// the theme's and only the level moves. A panel over a terminal reads
    /// as floating when the light comes from its edge rather than from its
    /// face; lifted toward white instead, it was a pale card on a black
    /// terminal.
    ///
    /// This is the whole of the card following the theme. It was once mostly
    /// a fixed grey with the terminal's colour showing through at a fifth,
    /// which is a card that changes by a fifth of a theme.
    private static func deepened(_ color: NSColor) -> NSColor {
        guard let rgb = color.usingColorSpace(.sRGB) else { return color }
        // A light theme has less room to go down before it stops being the
        // same colour, so it takes a smaller step.
        let factor: CGFloat = rgb.brightnessComponent >= 0.5 ? 0.94 : 0.78
        return NSColor(
            srgbRed: rgb.redComponent * factor,
            green: rgb.greenComponent * factor,
            blue: rgb.blueComponent * factor,
            alpha: 1)
    }

    /// The plate is painted per appearance, so it has to be repainted when
    /// the appearance changes under it.
    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        adoptTerminalBackground()
    }

    /// The terminal's font may change with a config reload, and the preview
    /// shows terminal output — including glyphs only a Nerd Font supplies.
    private func adoptTerminalFont() {
        let font = GhosttyApp.shared.terminalFont(size: 10)
        if preview.font != font { preview.font = font }
    }

    func apply(_ model: PickerModel) {
        adoptTerminalFont()
        // Said every time, not only when the mode changes. The view starts
        // out believing it is in `.goTo`, which is the mode the first ⌘P
        // arrives in — so the one that mattered most was the one nobody ever
        // set, and the field opened blank.
        switch model.mode {
        case .goTo:
            field.placeholderString = "go to a terminal, or a folder to start one in…"
        case .palette(.root):
            field.placeholderString = "what would you like to do…"
        case .palette(.themes):
            field.placeholderString = "a theme to wear…"
        case .palette(.fonts):
            field.placeholderString = "a face to set it in…"
        case .search(let global):
            field.placeholderString = global
                ? "find everywhere…"
                : "find in \(model.scopeLabel ?? "this pane")…"
        }
        if mode != model.mode {
            mode = model.mode
            field.stringValue = ""
            query = ""
        }
        matches = model.matches
        if all != model.items {
            all = model.items
            refilter(preservingSelection: true)
        }
        // A theme previews as a screen in it, and a face as a screen set in
        // it. Neither is a question for the daemon: the answer is a file on
        // this machine, and the sample is made here.
        if let id = model.previewOf,
           let item = model.items.first(where: { $0.id == id }),
           Self.isAppearance(item.kind) {
            showSample(of: item)
            return
        }
        preview.drawsBackground = false
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
                    font: GhosttyApp.shared.terminalFont(size: 11),
                    palette: terminal.colors,
                    foreground: terminal.foreground.withAlphaComponent(0.85)))
            preview.scrollToBeginningOfDocument(nil)
        }
        // Next turn: the card is measured after the window has laid it out,
        // not while the model is still being applied to it.
        if Trace.enabled { DispatchQueue.main.async { [weak self] in self?.traceShape() } }
    }

    private static func isAppearance(_ kind: PickerModel.Item.Kind) -> Bool {
        switch kind {
        case .theme, .fontFamily: return true
        case .running, .destination, .hit, .command: return false
        }
    }

    /// A few lines of a terminal, in the theme or the face being considered.
    ///
    /// The same lines every time, on purpose: what is being compared is the
    /// colours, and a sample whose text changed between two themes would be
    /// asking you to compare two different things.
    ///
    /// The background is painted as an attribute rather than on the view.
    /// The text view is only as tall as its text and the pane is taller, so a
    /// background set on the view is a block that stops halfway down the
    /// card; padded lines carrying their own put the colour exactly where the
    /// screen would be.
    private func showSample(of item: PickerModel.Item) {
        var family: String?
        var colors: ThemeCatalog.Colors?
        switch item.kind {
        case .theme(let name):
            colors = ThemeCatalog.colors(of: name)
        case .fontFamily(let name):
            family = name
        default: return
        }
        let sampleKey = "sample:\(item.id)"
        guard lastPreview != sampleKey else { return }
        lastPreview = sampleKey

        let font = family.flatMap { NSFont(name: $0, size: 11) }
            ?? GhosttyApp.shared.terminalFont(size: 11)
        let palette = GhosttyApp.shared.terminalPalette()
        let ink = colors?.foreground ?? palette.foreground
        func colour(_ slot: Int) -> NSColor {
            colors.map { $0.ansi[slot] } ?? palette.colors[slot]
        }
        // Wide enough that the painted background reads as a screen rather
        // than as a ragged label.
        let width = 46
        let lines: [[(String, NSColor)]] = [
            [("~/Projects/keep", colour(4)), ("  main", colour(5))],
            [("$ ", colour(2)), ("git status", ink)],
            [(" M ", colour(3)), ("crates/keepd/src/tab.rs", ink)],
            [("?? ", colour(1)), ("apps/macos/Sources/Keep/UI", ink)],
            [("$ ", colour(2)), ("cargo build --release", ink)],
            [("   Compiling ", colour(2)), ("keepd v0.1.0", ink)],
            [("    Finished ", colour(6)), ("in 4.21s", ink)],
            [("", ink)],
            [("████", colour(1)), ("████", colour(2)), ("████", colour(3)),
             ("████", colour(4)), ("████", colour(5)), ("████", colour(6))],
        ]
        let text = NSMutableAttributedString()
        for line in lines {
            var drawn = 0
            for (run, tint) in line {
                text.append(NSAttributedString(
                    string: run, attributes: [.font: font, .foregroundColor: tint]))
                drawn += run.count
            }
            // Padded, so the background runs to the edge of the sample.
            let pad = max(width - drawn, 1)
            text.append(NSAttributedString(
                string: String(repeating: " ", count: pad) + "\n",
                attributes: [.font: font, .foregroundColor: ink]))
        }
        if let background = colors?.background {
            text.addAttribute(
                .backgroundColor, value: background,
                range: NSRange(location: 0, length: text.length))
        }
        preview.textStorage?.setAttributedString(text)
        preview.scrollToBeginningOfDocument(nil)
    }

    /// The field owns the keyboard for as long as the picker is up.
    func takeFocus() {
        window?.makeFirstResponder(field)
        // The caret is the accent colour by default, which in a card that has
        // just had every other blue taken out of it is the only blue left.
        // The field editor is shared and handed round, so it is set here,
        // each time this field takes it.
        if let editor = window?.fieldEditor(true, for: field) as? NSTextView {
            editor.insertionPointColor = .white
        }
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
        closeActions()
        guard !query.isEmpty else { return }
        query = ""
        field.stringValue = ""
        refilter(preservingSelection: false)
    }

    var selectedItemID: String? { selectedItem?.id }

    private var selectedItem: PickerModel.Item? {
        guard table.selectedRow >= 0, table.selectedRow < shown.count else { return nil }
        return shown[table.selectedRow].item
    }

    // MARK: - actions

    /// ⌘K, the footer's chip and the right button all arrive here.
    private func toggleActions() {
        if actions == nil { openActions() } else { closeActions() }
    }

    private func openActions() {
        guard let item = selectedItem else { return }
        if actions != nil { closeActions() }
        let panel = PickerActionsPanel()
        panel.translatesAutoresizingMaskIntoConstraints = false
        panel.onRun = { [weak self] action in
            guard let self else { return }
            // Closed before the action runs: several of these take the app
            // somewhere else, and a panel still up when Finder comes forward
            // is a panel that comes back with it.
            let id = item.id
            self.closeActions()
            self.onAction?(id, action)
        }
        panel.onClose = { [weak self] in self?.closeActions() }
        // Over the card rather than inside it, for the reason the chip is —
        // and it must be the panel's own glass that the chip's edge meets.
        addSubview(panel)
        NSLayoutConstraint.activate([
            // Out of the corner it was named in — the footer's chip is
            // directly beneath it, which is what makes the panel read as that
            // chip opening rather than as something arriving from elsewhere.
            panel.trailingAnchor.constraint(
                equalTo: card.trailingAnchor, constant: -PickerFooter.margin),
            panel.bottomAnchor.constraint(
                equalTo: card.bottomAnchor, constant: -PickerFooter.margin),
        ])
        panel.present(item)
        actions = panel
        panel.takeFocus()
    }

    private func closeActions() {
        guard let panel = actions else { return }
        actions = nil
        panel.removeFromSuperview()
        // The field gets the keyboard back, or the picker is up with nothing
        // listening to it.
        takeFocus()
    }

    // MARK: - filtering

    private func refilter(preservingSelection: Bool) {
        let previous = preservingSelection ? selectedItemID : nil
        // In search the daemon has already decided what matches; filtering
        // its answer again with a different rule would hide real hits.
        if isSearching {
            litColumns = [:]
            shown = all.map(Row.item)
        } else if case .palette(let catalog) = mode {
            let (kept, marks) = Self.matches(all, query: query)
            litColumns = marks
            // Headings only on the untyped list. Scoring mixes the groups
            // together, and a heading standing over one row of its own kind
            // and four of another is worse than no heading at all.
            shown = catalog == .root && query.isEmpty
                ? Self.grouped(kept)
                : kept.map(Row.item)
        } else {
            let (kept, marks) = Self.matches(all, query: query)
            litColumns = marks
            shown = Self.sectioned(kept)
        }
        table.reloadData()
        let index = previous.flatMap { id in
            shown.firstIndex { $0.item?.id == id }
        } ?? firstSelectableRow()
        select(row: index)
    }

    /// The two headings, around the two groups, and neither when its group is
    /// empty — which is what makes a heading disappear as its section is
    /// typed away rather than stand over nothing.
    ///
    /// The kinds are gathered rather than assumed to arrive together. Unfiltered
    /// they do, because the list is built as `running + new`; but the score
    /// sorts the whole list at once, so one letter typed can put a folder
    /// between two terminals — and a heading standing over rows of the other
    /// kind is worse than no heading at all. Order within each group is left
    /// exactly as the score left it.
    private static func sectioned(_ items: [PickerModel.Item]) -> [Row] {
        var terminals: [PickerModel.Item] = []
        var folders: [PickerModel.Item] = []
        var loose: [PickerModel.Item] = []
        for item in items {
            switch item.kind {
            case .running: terminals.append(item)
            case .destination: folders.append(item)
            // Neither group, and neither heading. A palette's rows never
            // reach here — `grouped` takes those — but a kind with nowhere
            // to go must still land somewhere rather than vanish.
            case .hit, .command, .theme, .fontFamily: loose.append(item)
            }
        }
        var rows: [Row] = loose.map(Row.item)
        if !terminals.isEmpty {
            rows.append(.heading("terminals"))
            rows += terminals.map(Row.item)
        }
        if !folders.isEmpty {
            rows.append(.heading("open in"))
            rows += folders.map(Row.item)
        }
        return rows
    }

    /// Runs of one group, each under its name.
    ///
    /// The order is the list's own — the palette hands its commands over in
    /// the order they should be read — so this only has to notice where one
    /// group stops and the next begins.
    private static func grouped(_ items: [PickerModel.Item]) -> [Row] {
        var rows: [Row] = []
        var current: String?
        for item in items {
            if item.context != current {
                current = item.context
                if !item.context.isEmpty { rows.append(.heading(item.context)) }
            }
            rows.append(.item(item))
        }
        return rows
    }

    /// The first row that can hold the selection, or -1 in an empty list.
    private func firstSelectableRow() -> Int {
        shown.firstIndex { $0.item != nil } ?? -1
    }

    /// The next row the selection may rest on, stepping over headings.
    ///
    /// Clamps rather than wraps, and returns where it started when there is
    /// nowhere to go, so holding ↓ at the bottom of the list does nothing
    /// instead of jumping back to the top.
    private func nextSelectable(from row: Int, step: Int) -> Int {
        var next = row + step
        while next >= 0 && next < shown.count {
            if shown[next].item != nil { return next }
            next += step
        }
        return row >= 0 && row < shown.count && shown[row].item != nil
            ? row : firstSelectableRow()
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
        let key = event.charactersIgnoringModifiers?.lowercased() ?? ""
        // Both halves of the toggle, and the panel's escape hatch, in one
        // place: the panel is a subview of this one, so a ⌘K it answered
        // itself would only ever be the closing half.
        if held == .command, key == "k" {
            toggleActions()
            return true
        }
        // The two the panel advertises, so that what it says about them is
        // true without opening it. ⇧⌘F is not among them: it is Find
        // Everywhere, and the picker is the last thing that should take it.
        if held == [.command, .shift], key == "r" || key == "c" {
            guard let item = selectedItem else { return true }
            // The panel, if it is up, has just been answered without it.
            closeActions()
            switch (key, item.kind) {
            case ("r", _) where !item.path.isEmpty: onAction?(item.id, .reveal)
            case ("c", .hit): onAction?(item.id, .copyText)
            case ("c", _) where !item.path.isEmpty: onAction?(item.id, .copyPath)
            default: break
            }
            return true
        }
        // While the panel is up it owns the arrows; moving the list behind it
        // would change what the actions are about, under the panel naming it.
        guard held == .control, actions == nil else {
            return super.performKeyEquivalent(with: event)
        }
        switch event.charactersIgnoringModifiers {
        case "j":
            select(row: nextSelectable(from: table.selectedRow, step: 1))
            return true
        case "k":
            select(row: nextSelectable(from: table.selectedRow, step: -1))
            return true
        default:
            return super.performKeyEquivalent(with: event)
        }
    }

    private func select(row: Int) {
        defer { updateCounter() }
        guard row >= 0, row < shown.count, shown[row].item != nil else {
            table.deselectAll(nil)
            footer.show(nil)
            onHighlight?(nil)
            return
        }
        // The table's own delegate reports the change; announcing it here as
        // well asked the daemon for the same preview twice.
        table.selectRowIndexes([row], byExtendingSelection: false)
        table.scrollRowToVisible(row)
        // Said here as well as from the delegate: a refilter that lands on the
        // same row number holding a different item changes the selection
        // without changing the selected row, and the table reports nothing.
        footer.show(selectedItem)
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
    /// The rows that match, best first, and which column put each one there.
    ///
    /// Three columns are tried in reading order, and the one that answers is
    /// remembered. The context column goes first on purpose: it is where a
    /// workspace's name lives, so typing one gathers that project rather than
    /// scattering its tabs behind whichever titles happened to score better.
    static func matches(
        _ items: [PickerModel.Item], query: String
    ) -> ([PickerModel.Item], [String: Lit]) {
        let needle = query.lowercased().filter { !$0.isWhitespace }
        guard !needle.isEmpty else { return (items, [:]) }
        let scored = items.enumerated().compactMap {
            (position, item) -> (PickerModel.Item, Int, Int, Lit)? in
            let columns: [(Lit.Column, String)] = [
                (.context, item.context), (.title, item.title), (.detail, item.detail),
            ]
            for (column, text) in columns {
                guard let landed = Self.marks(text.lowercased(), needle),
                      let first = landed.first, let last = landed.last
                else { continue }
                return (
                    item, (last - first) + first / 2, position,
                    Lit(column: column, marks: landed)
                )
            }
            return nil
        }
        // Position is the tiebreak, and it has to be written down: Swift's
        // sort is not documented stable, so rows of equal score kept the
        // list's own order — where you have been, most recent first — only by
        // luck, and lost it the moment the sort changed its mind.
        .sorted { ($0.1, $0.2) < ($1.1, $1.2) }
        var kept: [PickerModel.Item] = []
        var columns: [String: Lit] = [:]
        for (item, _, _, hit) in scored {
            kept.append(item)
            columns[item.id] = hit
        }
        return (kept, columns)
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
    ///
    /// Lit by weight and brightness rather than by a colour. The accent this
    /// used was the system's fixed blue — the one thing in the card that had
    /// not chosen its own colour, in an app whose rule is that colour carries
    /// information or does not appear. What is left says the same thing with
    /// the ladder the rest of the chrome is built on: the letters that
    /// matched come forward, the ones around them step back.
    static func lit(
        _ text: String, marks: [Int], font: NSFont,
        base: NSColor = .secondaryLabelColor, hit: NSColor = .labelColor
    ) -> NSAttributedString {
        let attributed = NSMutableAttributedString(
            string: text, attributes: [.font: font, .foregroundColor: base])
        let colour = hit
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
            select(row: nextSelectable(from: table.selectedRow, step: 1))
            return true
        case #selector(NSResponder.moveUp(_:)):
            select(row: nextSelectable(from: table.selectedRow, step: -1))
            return true
        case #selector(NSResponder.insertNewline(_:)):
            if let id = selectedItemID { onChoose?(id) }
            return true
        case #selector(NSResponder.cancelOperation(_:)):
            // A catalog was opened from the palette, so escape goes back to
            // it rather than out of the overlay: you came here to do
            // something, and changing your mind about *which* theme is not
            // changing your mind about the palette.
            if case .palette(let catalog) = mode, catalog != .root {
                onBack?()
                return true
            }
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

/// A box whose top and bottom edges fade out.
///
/// A fade rather than a blur, and not for want of trying. Blurring what is
/// behind something, inside a window, has two native forms and this card can
/// afford neither: `NSVisualEffectView` is the window server's own blur and
/// costs nothing to scroll, but a material paints as well as blurs, and over
/// glass its edge is a band across the card whatever material is chosen. A
/// `CIGaussianBlur` in `backgroundFilters` paints nothing and is redrawn on
/// every frame the list moves, which is a blur you can count. What is left is
/// this, and it costs neither.
///
/// A row travelling up under the field thins out and is gone before it
/// reaches it, rather than sliding behind a lid; the same at the bottom, so
/// the list ends by running out rather than by being cut.
///
/// Around the scroll view rather than on it: a scroll view manages its own
/// layer and a mask put there is quietly lost. And not on the clip view
/// either, whose bounds travel with the scroll — laid there, the fade rode
/// along with the content and came to rest against the last row.
///
/// A material laid over the top was the first try, since a material blurs
/// what is behind it inside the window and that is the native way to get
/// this. But a material also paints: with nothing scrolled up there it is a
/// band across the card, and what it does to the row underneath is hide it
/// rather than soften it.
private final class FadingBox: NSView {
    var fadeTop: CGFloat = 0 { didSet { needsLayout = true } }
    var fadeBottom: CGFloat = 0 { didSet { needsLayout = true } }
    private let fade = CAGradientLayer()

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        fade.colors = [
            NSColor.clear.cgColor, NSColor.black.cgColor,
            NSColor.black.cgColor, NSColor.clear.cgColor,
        ]
        // Top to bottom along the layer's own axis, which grows upwards.
        fade.startPoint = CGPoint(x: 0.5, y: 1)
        fade.endPoint = CGPoint(x: 0.5, y: 0)
        layer?.mask = fade
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    override func layout() {
        super.layout()
        guard bounds.height > 0 else { return }
        let top = min(0.6, fadeTop / bounds.height)
        let bottom = min(0.5, fadeBottom / bounds.height)
        // No implicit animation: this runs on every resize, and a
        // quarter-second cross-fade of a mask is a shimmer.
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        fade.frame = bounds
        fade.locations = [
            // A long ramp: a short one lets a row's own edge — the top of a
            // selected lozenge especially — cross it in a few points and read
            // as a line drawn across the card.
            NSNumber(value: top * 0.28), NSNumber(value: top),
            NSNumber(value: 1 - bottom), NSNumber(value: 1.0),
        ]
        CATransaction.commit()
    }
}

/// A hairline whose brightness travels around the shape.
///
/// The border is drawn at full strength and then *masked* by a gradient of
/// alphas, rather than being drawn in a gradient of colours. That is what
/// keeps the corner: a stroked path has to say which curve it is stroking,
/// and the continuous corner AppKit rounds these views with is not a curve
/// it will hand out. A layer with `cornerCurve = .continuous` draws its own
/// border on the right shape, and a mask over it decides how much of that
/// border reaches the eye at each point.
private final class EdgeView: NSView {
    private let rim = CALayer()
    private let fall = CAGradientLayer()

    init(cornerRadius: CGFloat) {
        super.init(frame: .zero)
        wantsLayer = true
        rim.cornerRadius = cornerRadius
        rim.cornerCurve = .continuous
        rim.borderWidth = 1
        rim.borderColor = NSColor.white.cgColor
        // Light from above. The top edge catches most of it, the sides fall
        // away to almost nothing, and the bottom takes a little back off
        // whatever the card is lying on — which is the reading that makes an
        // edge look like a thickness rather than a line.
        //
        // Quietly. A rim that swings from a third of white to almost nothing
        // reads as a border with a bright part, which is a different thing
        // from an edge catching light — the swing has to be small enough
        // that you notice the shape, not the gradient.
        fall.colors = [
            NSColor.white.withAlphaComponent(0.22).cgColor,
            NSColor.white.withAlphaComponent(0.12).cgColor,
            NSColor.white.withAlphaComponent(0.17).cgColor,
        ]
        fall.locations = [0, 0.45, 1]
        // Along the layer's own axis, which grows upwards.
        fall.startPoint = CGPoint(x: 0.5, y: 1)
        fall.endPoint = CGPoint(x: 0.5, y: 0)
        rim.mask = fall
        layer?.addSublayer(rim)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    override func layout() {
        super.layout()
        // No implicit animation: this runs on every window resize, and a
        // quarter-second cross-fade of a rim is a shimmer around the card.
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        rim.frame = bounds
        fall.frame = bounds
        CATransaction.commit()
    }

    /// It is a decoration lying over the whole card. Every click through it
    /// belongs to whatever is underneath.
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}

/// A scrap of a terminal in a theme's own colours.
///
/// Its background, five of its sixteen, and a hairline so that a theme whose
/// background is the same as the card's does not read as a hole. Five rather
/// than sixteen because at this size sixteen is a smear.
private final class ThemeSwatchView: NSView {
    init(colors: ThemeCatalog.Colors?) {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        wantsLayer = true
        layer?.cornerCurve = .continuous
        layer?.cornerRadius = 5
        layer?.borderWidth = 1
        layer?.borderColor = NSColor.separatorColor.withAlphaComponent(0.4).cgColor
        NSLayoutConstraint.activate([
            widthAnchor.constraint(equalToConstant: 46),
            heightAnchor.constraint(equalToConstant: 18),
        ])
        guard let colors else {
            // A theme whose file would not parse: an empty frame, which is
            // honest, rather than somebody else's colours in its name.
            layer?.backgroundColor = NSColor.clear.cgColor
            return
        }
        layer?.backgroundColor = colors.background.cgColor
        // The four that carry a theme's character — red, green, yellow, blue
        // — and its foreground last, which is what most of the screen is.
        let shown = [colors.ansi[1], colors.ansi[2], colors.ansi[3], colors.ansi[4],
                     colors.foreground]
        let strip = NSStackView(views: shown.map { colour in
            let dot = NSView()
            dot.wantsLayer = true
            dot.layer?.cornerRadius = 1.5
            dot.layer?.backgroundColor = colour.cgColor
            dot.translatesAutoresizingMaskIntoConstraints = false
            NSLayoutConstraint.activate([
                dot.widthAnchor.constraint(equalToConstant: 5),
                dot.heightAnchor.constraint(equalToConstant: 9),
            ])
            return dot
        })
        strip.orientation = .horizontal
        strip.spacing = 2
        strip.translatesAutoresizingMaskIntoConstraints = false
        addSubview(strip)
        NSLayoutConstraint.activate([
            strip.centerXAnchor.constraint(equalTo: centerXAnchor),
            strip.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }
}

/// A row that lights up the way the sidebar's do.
///
/// The same shape, the same inset, and the same glass — a list of places to go
/// inside a window whose other list of places to go looks like this should not
/// have to be told twice what a chosen row looks like.
private final class PickerRow: NSTableRowView {
    // Adaptive: Tahoe glass draws its own depth, and on a light ground that
    // depth is a pill floating off the list with a shadow under it.
    private let lozenge = AdaptiveLozengeView(
        cornerRadius: 12, lightFill: NSColor.black.withAlphaComponent(0.10))
    /// A breath of white under the pointer, the same one the sidebar uses.
    /// Flat rather than glass: it appears and disappears as the pointer
    /// travels, and a pane of glass per row for that is a lot of glass.
    private let hover = NSView()
    private var hovered = false { didSet { showHover() } }

    override init(frame: NSRect) {
        super.init(frame: frame)
        lozenge.isHidden = true
        hover.wantsLayer = true
        hover.layer?.cornerCurve = .continuous
        hover.layer?.cornerRadius = 12
        hover.layer?.backgroundColor = NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
                ? NSColor.white.withAlphaComponent(0.06)
                : NSColor.black.withAlphaComponent(0.05)
        }.cgColor
        hover.isHidden = true
        addSubview(hover, positioned: .below, relativeTo: nil)
        addSubview(lozenge, positioned: .below, relativeTo: nil)
    }

    /// The chosen row already says so; two marks on one row says nothing.
    private func showHover() { hover.isHidden = !hovered || isSelected }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for area in trackingAreas { removeTrackingArea(area) }
        addTrackingArea(NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .activeInActiveApp, .inVisibleRect],
            owner: self))
        // A row that is scrolled out from under the pointer is never sent
        // `mouseExited`; asking where the pointer actually is settles it.
        hovered = window.map { bounds.contains(convert($0.mouseLocationOutsideOfEventStream, from: nil)) }
            ?? false
    }

    override func mouseEntered(with event: NSEvent) { hovered = true }
    override func mouseExited(with event: NSEvent) { hovered = false }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    override func layout() {
        super.layout()
        lozenge.frame = bounds.insetBy(dx: 6, dy: 2)
        hover.frame = lozenge.frame
    }

    override var isSelected: Bool {
        didSet {
            guard isSelected != oldValue else { return }
            lozenge.isHidden = !isSelected
            showHover()
            if isSelected {
                // Quieter than the sidebar's, which is one row among five;
                // this is one row among forty, and a bright fill scanning
                // down a long list is a light being flashed at you.
                lozenge.set(
                    cornerRadius: 12,
                    tint: NSColor.white.withAlphaComponent(0.20),
                    lightFill: NSColor.black.withAlphaComponent(0.10))
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
        // A heading takes the plain row: `PickerRow` washes under the pointer,
        // and a wash on something that cannot be chosen is an invitation the
        // list will not honour.
        shown[row].item == nil ? NSTableRowView() : PickerRow()
    }

    func tableView(_ tableView: NSTableView, heightOfRow row: Int) -> CGFloat {
        // The list breathes between its groups rather than everywhere: a
        // heading is taller than a row because the gap is above it, which is
        // what makes it read as the start of something instead of as a row
        // that lost its text.
        shown[row].item == nil ? Self.headingHeight : Self.rowHeight
    }

    func tableView(_ tableView: NSTableView, shouldSelectRow row: Int) -> Bool {
        shown[row].item != nil
    }

    func tableView(
        _ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int
    ) -> NSView? {
        switch shown[row] {
        case .heading(let text): return headingCell(text)
        case .item(let item): return itemCell(item)
        }
    }

    /// A heading: the app's own voice, so the system face, and quiet enough
    /// that the eye takes it as a label on the way past rather than as a row.
    private func headingCell(_ text: String) -> NSView {
        let cell = NSView()
        let label = NSTextField(labelWithString: text)
        label.font = .systemFont(ofSize: 11, weight: .medium)
        label.textColor = .tertiaryLabelColor
        label.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(label)
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 18),
            // Pinned low, so the extra height falls above it as a gap between
            // the sections rather than around the word.
            label.bottomAnchor.constraint(equalTo: cell.bottomAnchor, constant: -4),
        ])
        return cell
    }

    private func itemCell(_ item: PickerModel.Item) -> NSView {
        let cell = NSView()
        switch item.kind {
        case .command(let command): return commandCell(command, item, in: cell)
        case .theme(let name): return themeCell(name, item, in: cell)
        case .fontFamily(let name): return fontCell(name, item, in: cell)
        case .hit: return hitCell(item, in: cell)
        case .running, .destination: break
        }
        // A workspace name and the directory under it are things the terminal
        // would also print, so they are set in the terminal's own face — and
        // in a column, which is the whole repair: three tabs of one project
        // read as three of one project before a word of them is read.
        let identifierFont = GhosttyApp.shared.terminalFont(size: 12.5)
        if item.isFolder { return folderCell(item, font: identifierFont, in: cell) }

        let context = NSTextField(labelWithString: item.context)
        context.font = identifierFont
        context.textColor = .secondaryLabelColor
        context.lineBreakMode = .byTruncatingTail
        context.maximumNumberOfLines = 1
        context.translatesAutoresizingMaskIntoConstraints = false
        // The column holds its width against a long name rather than pushing
        // the title along and taking the alignment with it.
        context.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        cell.addSubview(context)

        let title = NSTextField(labelWithString: item.title)
        title.font = identifierFont
        title.textColor = .labelColor
        title.lineBreakMode = .byTruncatingTail
        title.maximumNumberOfLines = 1
        title.translatesAutoresizingMaskIntoConstraints = false
        title.setContentCompressionResistancePriority(.defaultLow - 1, for: .horizontal)
        cell.addSubview(title)

        // What the row is here for, when the query is what put it here.
        if let hit = litColumns[item.id] {
            let target = hit.column == .context ? context : title
            let text = hit.column == .context ? item.context : item.title
            if hit.column == .context || hit.column == .title {
                target.attributedStringValue = Self.lit(
                    text, marks: hit.marks, font: identifierFont)
            }
        }

        // The terminal glyph, back where it was. The section heading says
        // terminal-or-folder too, but a heading is read once at the top of a
        // group and a row is read on its own — and filled-versus-hollow is
        // still the one thing here you take in without reading.
        let badge = NSImageView()
        badge.image = NSImage(
            systemSymbolName: item.busy ? "terminal.fill" : "terminal",
            accessibilityDescription: item.busy ? "running" : "terminal")
        badge.contentTintColor = item.busy ? .labelColor : .tertiaryLabelColor
        badge.symbolConfiguration = .init(pointSize: 13, weight: .regular)
        badge.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(badge)

        // What the tab is, and how long ago it was anything. The pane count
        // keeps its place ahead of the time so the times still form a column.
        let trailingText = [item.detail, Self.recency(item.lastActive)]
            .filter { !$0.isEmpty }
            .joined(separator: "  ")
        let trailing = NSTextField(labelWithString: trailingText)
        trailing.font = GhosttyApp.shared.terminalFont(size: 11)
        trailing.textColor = .tertiaryLabelColor
        trailing.alignment = .right
        trailing.maximumNumberOfLines = 1
        trailing.translatesAutoresizingMaskIntoConstraints = false
        trailing.setContentCompressionResistancePriority(.required, for: .horizontal)
        // A row here because of its pane count should say so, or it is a row
        // with no visible reason for being in the list. The detail leads the
        // joined string, so its marks land without shifting.
        if let hit = litColumns[item.id], hit.column == .detail {
            trailing.attributedStringValue = Self.lit(
                trailingText, marks: hit.marks, font: trailing.font ?? identifierFont,
                base: .tertiaryLabelColor)
        }
        cell.addSubview(trailing)

        // What the tab actually is. A title is what a program decided to call
        // itself — it may say nothing, and six tabs running one tool say the
        // same sentence — so the name of the program goes beside it. Set
        // harder against compression than the title: when the row runs out of
        // room the sentence is what should give, not the word that identifies
        // it.
        let command = NSTextField(labelWithString: item.command.isEmpty ? "" : "— \(item.command)")
        command.font = identifierFont
        command.textColor = .tertiaryLabelColor
        command.lineBreakMode = .byTruncatingTail
        command.maximumNumberOfLines = 1
        command.translatesAutoresizingMaskIntoConstraints = false
        command.setContentCompressionResistancePriority(.defaultHigh, for: .horizontal)
        command.setContentHuggingPriority(.required, for: .horizontal)
        cell.addSubview(command)

        NSLayoutConstraint.activate([
            context.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 18),
            context.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            context.widthAnchor.constraint(lessThanOrEqualToConstant: Self.contextWidth),
            badge.leadingAnchor.constraint(
                equalTo: cell.leadingAnchor, constant: Self.contextWidth + 26),
            badge.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            badge.widthAnchor.constraint(equalToConstant: 16),
            title.leadingAnchor.constraint(equalTo: badge.trailingAnchor, constant: 8),
            title.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            command.leadingAnchor.constraint(equalTo: title.trailingAnchor, constant: 6),
            command.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            command.trailingAnchor.constraint(
                lessThanOrEqualTo: trailing.leadingAnchor, constant: -10),
            trailing.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -18),
            trailing.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
        ])
        return cell
    }

    /// A place to start something: one path, one line.
    ///
    /// The road faint and the name it would take in ink, so the eye lands on
    /// the word that becomes the workspace. One string rather than two labels
    /// because it is one path — and under a heading that already says these
    /// are folders, printing the name again on the right was the same word
    /// twice, which is what this replaces.
    private func folderCell(
        _ item: PickerModel.Item, font: NSFont, in cell: NSView
    ) -> NSView {
        // `detail` is the whole path and `title` its last component, so the
        // road to the folder is what is left when the name is taken off the
        // end — no rejoining, and no chance of the two disagreeing.
        let badge = NSImageView()
        badge.image = NSImage(systemSymbolName: "folder", accessibilityDescription: "folder")
        badge.contentTintColor = .tertiaryLabelColor
        badge.symbolConfiguration = .init(pointSize: 13, weight: .regular)
        badge.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(badge)

        let path = item.detail.isEmpty ? item.title : item.detail
        let parentLength = max(path.count - item.title.count, 0)
        let label = NSTextField(labelWithString: path)
        label.font = font
        let text = NSMutableAttributedString(
            string: path,
            attributes: [.font: font, .foregroundColor: NSColor.labelColor])
        if parentLength > 0 {
            let head = String(path.prefix(parentLength))
            text.addAttribute(
                .foregroundColor, value: NSColor.tertiaryLabelColor,
                range: NSRange(location: 0, length: (head as NSString).length))
        }
        // A row that matched on its path says where, on the letters that did.
        if let hit = litColumns[item.id] {
            // A match on the path counts from the path's start and needs no
            // shifting; one on the name counts from where the name begins.
            let shift = hit.column == .title ? parentLength : 0
            let bold = NSFontManager.shared.convert(font, toHaveTrait: .boldFontMask)
            let scalars = Array(path)
            var offsets: [Int] = []
            var cursor = 0
            for character in scalars {
                offsets.append(cursor)
                cursor += String(character).utf16.count
            }
            for mark in hit.marks {
                let index = mark + shift
                guard index < offsets.count else { continue }
                let start = offsets[index]
                let length = index + 1 < offsets.count
                    ? offsets[index + 1] - start
                    : (path as NSString).length - start
                guard length > 0, start + length <= (path as NSString).length else { continue }
                text.addAttributes(
                    [.foregroundColor: NSColor.labelColor, .font: bold],
                    range: NSRange(location: start, length: length))
            }
        }
        label.attributedStringValue = text
        // The head is what gives on a path: a long one is known by its end.
        label.lineBreakMode = .byTruncatingHead
        label.maximumNumberOfLines = 1
        label.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(label)
        NSLayoutConstraint.activate([
            badge.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 18),
            badge.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            badge.widthAnchor.constraint(equalToConstant: 16),
            label.leadingAnchor.constraint(equalTo: badge.trailingAnchor, constant: 8),
            label.trailingAnchor.constraint(
                lessThanOrEqualTo: cell.trailingAnchor, constant: -18),
            label.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
        ])
        return cell
    }

    /// Something to do: the glyph, the words, and the chord that does the
    /// same thing without coming through here.
    ///
    /// Set in the system face, not the terminal's. Everything else in this
    /// list is a thing the terminal knows about — a directory, a title, a
    /// line it printed — and is set in the terminal's face for that reason.
    /// A command is the app talking about itself.
    private func commandCell(
        _ command: Command, _ item: PickerModel.Item, in cell: NSView
    ) -> NSView {
        let badge = NSImageView()
        badge.image = NSImage(
            systemSymbolName: command.symbol, accessibilityDescription: command.title)
        badge.contentTintColor = command.isDestructive ? .systemRed : .secondaryLabelColor
        badge.symbolConfiguration = .init(pointSize: 13, weight: .regular)
        badge.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(badge)

        let font = NSFont.systemFont(ofSize: 13)
        let title = NSTextField(labelWithString: command.title)
        title.font = font
        title.textColor = command.isDestructive ? .systemRed : .labelColor
        title.lineBreakMode = .byTruncatingTail
        title.maximumNumberOfLines = 1
        title.translatesAutoresizingMaskIntoConstraints = false
        if let hit = litColumns[item.id], hit.column == .title {
            title.attributedStringValue = Self.lit(
                command.title, marks: hit.marks, font: font,
                base: command.isDestructive ? .systemRed : .secondaryLabelColor,
                hit: command.isDestructive ? .systemRed : .labelColor)
        }
        cell.addSubview(title)

        NSLayoutConstraint.activate([
            badge.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 18),
            badge.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            badge.widthAnchor.constraint(equalToConstant: 18),
            title.leadingAnchor.constraint(equalTo: badge.trailingAnchor, constant: 10),
            title.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
        ])

        guard !command.keys.isEmpty else { return cell }
        let caps = KeyCapView.row(command.keys)
        cell.addSubview(caps)
        NSLayoutConstraint.activate([
            caps.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            caps.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -16),
            title.trailingAnchor.constraint(lessThanOrEqualTo: caps.leadingAnchor, constant: -10),
        ])
        return cell
    }

    /// A theme, wearing itself.
    ///
    /// The name of a theme says nothing — there are four hundred of them and
    /// half are named after a mountain. So the row carries a scrap of the
    /// thing itself: the background it would paint, with its own colours on
    /// it. That is the whole choice, made without reading.
    private func themeCell(
        _ name: String, _ item: PickerModel.Item, in cell: NSView
    ) -> NSView {
        let swatch = ThemeSwatchView(colors: ThemeCatalog.colors(of: name))
        cell.addSubview(swatch)

        let font = GhosttyApp.shared.terminalFont(size: 12.5)
        let title = NSTextField(labelWithString: name)
        title.font = font
        title.textColor = .labelColor
        title.lineBreakMode = .byTruncatingTail
        title.maximumNumberOfLines = 1
        title.translatesAutoresizingMaskIntoConstraints = false
        if let hit = litColumns[item.id], hit.column == .title {
            title.attributedStringValue = Self.lit(name, marks: hit.marks, font: font)
        }
        cell.addSubview(title)

        // The one being worn says so, since a list of four hundred names has
        // no other way of telling you where you already are.
        let worn = NSTextField(labelWithString: item.detail)
        worn.font = .systemFont(ofSize: 11)
        worn.textColor = .tertiaryLabelColor
        worn.translatesAutoresizingMaskIntoConstraints = false
        worn.setContentCompressionResistancePriority(.required, for: .horizontal)
        cell.addSubview(worn)

        NSLayoutConstraint.activate([
            swatch.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 18),
            swatch.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            title.leadingAnchor.constraint(equalTo: swatch.trailingAnchor, constant: 12),
            title.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            title.trailingAnchor.constraint(lessThanOrEqualTo: worn.leadingAnchor, constant: -10),
            worn.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -18),
            worn.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
        ])
        return cell
    }

    /// A face, set in itself. Same argument as the theme above: the name of a
    /// monospaced family tells you almost nothing about its zero.
    private func fontCell(
        _ family: String, _ item: PickerModel.Item, in cell: NSView
    ) -> NSView {
        let face = NSFont(name: family, size: 13) ?? .monospacedSystemFont(ofSize: 13, weight: .regular)
        let title = NSTextField(labelWithString: family)
        title.font = face
        title.textColor = .labelColor
        title.lineBreakMode = .byTruncatingTail
        title.maximumNumberOfLines = 1
        title.translatesAutoresizingMaskIntoConstraints = false
        if let hit = litColumns[item.id], hit.column == .title {
            title.attributedStringValue = Self.lit(family, marks: hit.marks, font: face)
        }
        cell.addSubview(title)

        // The glyphs a terminal is actually judged on, in the face itself.
        let sample = NSTextField(labelWithString: "0O l1I {}[] =>")
        sample.font = NSFont(name: family, size: 12) ?? face
        sample.textColor = .tertiaryLabelColor
        sample.translatesAutoresizingMaskIntoConstraints = false
        sample.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        cell.addSubview(sample)

        NSLayoutConstraint.activate([
            title.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 18),
            title.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            title.trailingAnchor.constraint(lessThanOrEqualTo: sample.leadingAnchor, constant: -12),
            sample.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -18),
            sample.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
        ])
        return cell
    }

    /// A line of history, drawn as the terminal drew it — no columns, because
    /// a hit is one string and splitting it would be inventing structure that
    /// the text does not have.
    private func hitCell(_ item: PickerModel.Item, in cell: NSView) -> NSView {
        let font = GhosttyApp.shared.terminalFont(size: 12)
        let title = NSTextField(labelWithString: item.title)
        title.font = font
        if let match = matches[item.id] {
            title.attributedStringValue = Self.marked(
                item.title, range: match.range, font: font)
        }
        title.lineBreakMode = .byTruncatingTail
        title.maximumNumberOfLines = 1
        title.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(title)

        let where_ = matches[item.id].map { "\($0.group):\(item.detail)" } ?? item.detail
        let detail = NSTextField(labelWithString: where_)
        detail.font = .systemFont(ofSize: 12)
        detail.textColor = .tertiaryLabelColor
        detail.lineBreakMode = .byTruncatingHead
        detail.maximumNumberOfLines = 1
        detail.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(detail)

        NSLayoutConstraint.activate([
            title.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 18),
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
        footer.show(selectedItem)
        onHighlight?(selectedItemID)
    }

    @objc private func chooseSelected() {
        // What was clicked, not what was selected. A heading refuses the
        // selection, so a double-click on one leaves the previous row
        // selected and would otherwise open it — sending you somewhere you
        // did not click.
        let clicked = table.clickedRow
        guard clicked >= 0, clicked < shown.count, let item = shown[clicked].item
        else { return }
        onChoose?(item.id)
    }
}


/// The picker's list, which answers the right button.
///
/// A table would otherwise put up the standard contextual menu — or, having
/// none, nothing at all. This one reports the row that was clicked and lets
/// the picker decide, because what can be done to a row is the picker's
/// question and it already has a panel that answers it.
final class PickerTable: NSTableView {
    var onRightClick: ((Int) -> Void)?

    override func rightMouseDown(with event: NSEvent) {
        onRightClick?(row(at: convert(event.locationInWindow, from: nil)))
    }

    override func menu(for event: NSEvent) -> NSMenu? { nil }
}
