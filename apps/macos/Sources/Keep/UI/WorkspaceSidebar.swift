import SwiftUI

/// The workspace list.
///
/// Pure function of its rows: snapshot in, intents out. Navigation
/// deliberately does NOT hang off `List(selection:)` — that state belongs to
/// SwiftUI, which writes it for reasons that have nothing to do with intent (a
/// freshly revealed outline view takes focus and picks a row on its own), and
/// when selection drove navigation every one of those writes was a workspace
/// switch nobody asked for. Entering a workspace is a thing you do, so it
/// hangs off the button and nothing else.
///
/// Two rules decide how it looks, and both come from the app around it.
///
/// A workspace name is terminal content — it is a directory, it appears in
/// prompts and titles — so it is set in the face the terminal is set in, not
/// in the system face the app speaks its own labels in. That is the difference
/// between chrome bolted onto a terminal and chrome that belongs to one.
///
/// And colour carries information or it does not appear. The dot carries
/// state; the current row carries a wash in a hue derived from its own name,
/// so that selection is unmistakable without being the same blue every app
/// uses for it. Nothing else in here is coloured.
struct WorkspaceSidebar: View {
    @ObservedObject var model: SidebarRows
    let dispatch: (Intent) -> Void
    @State private var newName = ""
    @State private var hovered: String?
    @FocusState private var fieldFocused: Bool

    private var rows: [SessionSnapshot.SidebarRow] { model.rows }

    /// The terminal's own face, for the strings the terminal would also print.
    private var identifier: Font {
        Font(GhosttyApp.shared.terminalFont(size: 12.5))
    }

    private var counter: Font {
        Font(GhosttyApp.shared.terminalFont(size: 10.5))
    }

    var body: some View {
        List {
            ForEach(rows) { row in
                rowButton(row)
            }
            // Dragging a row rearranges the list. The order is a preference
            // about looking, so it is the app that keeps it; the daemon knows
            // which workspaces exist and nothing about which one you want at
            // the top.
            .onMove { from, to in
                dispatch(.reorderWorkspaces(from: from, to: to))
            }

            // Under the last workspace, not pinned to the floor. Starting one
            // is the next thing after the ones you have, and a row at the
            // bottom of a tall empty column reads as a different control than
            // the list it belongs to.
            newRow
        }
        // The AppKit host owns one flat, full-height surface; the plain style
        // avoids a second rounded panel below the titlebar, and the hidden
        // scroll background lets the terminal tint show through.
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
    }

    private func rowButton(_ row: SessionSnapshot.SidebarRow) -> some View {
        Button {
            dispatch(.activateWorkspace(row.name))
        } label: {
            HStack(spacing: 8) {
                StateDot(state: row.dot)
                Text(row.name)
                    .font(identifier)
                    .foregroundStyle(row.isActive ? Palette.ink : Palette.inkResting)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                // News if there is news, inventory otherwise. A workspace
                // running something is the only thing in this list that
                // changes while you are not looking at it, so that is what
                // the end of the row is for; when nothing is running it goes
                // back to saying how many tabs are waiting.
                if let first = row.running.first {
                    HStack(spacing: 4) {
                        Text(first)
                            .foregroundStyle(Palette.busy)
                            .lineLimit(1)
                            // From the front. Only a tab that is actually
                            // running something reaches this line — a shell
                            // sitting at a prompt never does — so what is
                            // here is the name of a piece of work, and a name
                            // is read from its beginning. Cutting the head
                            // off is what you do to a path, where the end is
                            // the part that identifies it, and there are no
                            // paths in this field.
                            .truncationMode(.tail)
                            .layoutPriority(-1)
                        // One name and a count, not a list: a row this wide
                        // can carry a name, and three names truncated to five
                        // characters each carry nothing. The rest are in the
                        // tooltip, which is where a list belongs.
                        if row.running.count > 1 {
                            Text("+\(row.running.count - 1)")
                                .foregroundStyle(Palette.busy.opacity(0.7))
                        }
                    }
                    .font(counter)
                } else if row.tabs > 0 {
                    Text("\(row.tabs)")
                        .font(counter)
                        .foregroundStyle(row.isActive ? Palette.inkResting : Palette.inkFaint)
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .background(rowBackground(for: row))
        .animation(.easeOut(duration: 0.16), value: row.isActive)
        .help(tooltip(for: row))
        .padding(.horizontal, 5)
        // Zero, and the gutter above instead: the plain list keeps insets of
        // its own that a row cannot see, and a block that stops short of both
        // edges by an amount nobody chose looks like a mistake rather than a
        // margin.
        .listRowInsets(EdgeInsets(top: 1, leading: 0, bottom: 1, trailing: 0))
        .listRowSeparator(.hidden)
        .listRowBackground(Color.clear)
        .onHover { inside in
            hovered = inside ? row.name : (hovered == row.name ? nil : hovered)
        }
        .contextMenu {
            Button("New Tab") { dispatch(.newTab(in: row.name)) }
            Divider()
            // Also in the menu, not only under the pointer. A list you can
            // only rearrange by dragging is a list most people never learn
            // can be rearranged.
            Button("Move Up") { move(row, by: -1) }
                .disabled(index(of: row) == 0)
            Button("Move Down") { move(row, by: 1) }
                .disabled(index(of: row) == rows.count - 1)
            Divider()
            Button("Close Workspace", role: .destructive) {
                dispatch(.killWorkspace(row.name))
            }
        }
    }

    /// Everything running here, one per line, or what the workspace is
    /// otherwise. The row shows one; this is where the rest live.
    private func tooltip(for row: SessionSnapshot.SidebarRow) -> String {
        guard !row.running.isEmpty else { return row.subtitle }
        let heading = row.running.count == 1
            ? "1 running"
            : "\(row.running.count) running"
        return ([heading] + row.running.map { "· \($0)" }).joined(separator: "\n")
    }

    private func index(of row: SessionSnapshot.SidebarRow) -> Int {
        rows.firstIndex(where: { $0.name == row.name }) ?? 0
    }

    /// One place up or down.
    ///
    /// `move(fromOffsets:toOffset:)` counts the destination in the list as it
    /// was before the row left it, so going down one lands two along.
    private func move(_ row: SessionSnapshot.SidebarRow, by step: Int) {
        let from = index(of: row)
        let to = step < 0 ? from - 1 : from + 2
        guard from + step >= 0, from + step < rows.count else { return }
        dispatch(.reorderWorkspaces(from: [from], to: to))
    }

    /// The current row is glass, lit in its own colour. Everything else is
    /// the ground it sits on, or a breath of white under the pointer.
    ///
    /// Glass rather than a painted rectangle because that is what the rest of
    /// this window's controls are made of, and a sidebar whose selection is
    /// the only flat thing in the app reads as a part that was made
    /// separately. Only the current row gets one: a pane of glass per row,
    /// appearing and disappearing on hover, is a lot of glass for a highlight
    /// that means "the pointer is here".
    @ViewBuilder
    private func rowBackground(for row: SessionSnapshot.SidebarRow) -> some View {
        if row.isActive {
            GlassRow(cornerRadius: 7, tint: Palette.lit(for: row.name))
        } else if hovered == row.name {
            RoundedRectangle(cornerRadius: 7, style: .continuous)
                .fill(Color.white.opacity(0.055))
        }
    }

    /// The current row is washed in its own colour; everything else is the
    /// ground it sits on.
    private func background(for row: SessionSnapshot.SidebarRow) -> Color {
        if row.isActive { return Palette.selection(for: row.name) }
        if hovered == row.name { return Color.white.opacity(0.055) }
        return .clear
    }

    /// Starting one: the same shape as the workspaces above it, because it
    /// does the same kind of thing.
    ///
    /// One affordance, not a field beside a button: typing a name and
    /// pressing return is the whole gesture, and the plus is there for the
    /// people who look for a plus. It stays unoutlined until you are typing
    /// in it — a box around an empty field competes with the list, and it was
    /// the only thing in here outlined all the time. What you type becomes a
    /// workspace name, so it is set in the face names are set in.
    private var newRow: some View {
        HStack(spacing: 8) {
            Image(systemName: "plus")
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(fieldFocused ? Palette.inkResting : Palette.inkFaint)
                .frame(width: 7)
            TextField(rows.isEmpty ? "Name your first workspace" : "New workspace", text: $newName)
                .textFieldStyle(.plain)
                .font(identifier)
                .foregroundStyle(Palette.inkResting)
                .focused($fieldFocused)
                .onSubmit(create)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(
            RoundedRectangle(cornerRadius: 7, style: .continuous)
                .fill(fieldFocused ? Color.white.opacity(0.09) : .clear)
        )
        .animation(.easeOut(duration: 0.16), value: fieldFocused)
        .padding(.horizontal, 5)
        .listRowInsets(EdgeInsets(top: 3, leading: 0, bottom: 1, trailing: 0))
        .listRowSeparator(.hidden)
        .listRowBackground(Color.clear)
        .contentShape(Rectangle())
        .onTapGesture { fieldFocused = true }
    }

    private func create() {
        dispatch(.newWorkspace(named: newName))
        newName = ""
    }
}

/// How a workspace is doing, in colour and in shape.
///
/// Both, deliberately: hue alone is a thing not everyone can read, so a filled
/// dot means something is watching this workspace and a hollow one means
/// nothing is.
private struct StateDot: View {
    let state: SessionSnapshot.SidebarRow.Dot

    var body: some View {
        Group {
            switch state {
            case .attached:
                Circle().fill(Palette.attached)
            case .busy:
                Circle().fill(Palette.busy)
            case .idle:
                Circle().strokeBorder(Palette.idle, lineWidth: 1.5)
            case .empty:
                Circle().strokeBorder(Palette.inkFaint, lineWidth: 1.5)
            }
        }
        .frame(width: 7, height: 7)
    }
}

/// The colours this sidebar is allowed to use, and where they come from.
private enum Palette {
    static let ink = Color.white.opacity(0.96)
    static let inkResting = Color.white.opacity(0.55)
    static let inkFaint = Color.white.opacity(0.32)

    static let attached = OKLCH.color(0.76, 0.14, 150)
    static let busy = OKLCH.color(0.82, 0.15, 85)
    static let idle = OKLCH.color(0.70, 0.05, 250)

    /// The wash behind the current workspace, in a hue that is that
    /// workspace's own.
    ///
    /// Selection is the one place the product register lets an accent go, and
    /// fixing it to a single blue is what makes every tool's sidebar the same
    /// sidebar. Deriving it from the name instead gives a workspace a colour
    /// it keeps across sessions, at no cost in information: the hue says
    /// which, the wash says current.
    ///
    /// Twelve hues rather than three hundred and sixty, because near-misses
    /// are the ugly case. Two names landing eleven degrees apart read as one
    /// colour mixed badly; two names landing on the same anchor read as the
    /// same colour, which is honest. Sharing is common with few workspaces and
    /// costs nothing: only the current row is washed, so two washes are never
    /// on screen together to be compared.
    static func selection(for name: String) -> Color {
        OKLCH.color(0.72, 0.13, hue(of: name), opacity: 0.17)
    }

    /// What glass is aimed at for a workspace. Refraction alone comes out
    /// darker than the ground it sits on; a tint is how it is made to read as
    /// lit, and this one is lit in the workspace's own colour.
    static func lit(for name: String) -> NSColor {
        OKLCH.appKitColor(0.72, 0.15, hue(of: name), alpha: 0.5)
    }

    private static func hue(of name: String) -> Double {
        15 + Double(anchor(of: name)) * 30
    }

    /// Which of the twelve a name lands on.
    ///
    /// Deliberately not `hashValue`, which is seeded per process: a workspace
    /// would be a different colour every launch. FNV-1a is stable, but its low
    /// bits carry almost none of the string — taken straight to a remainder it
    /// put ten sample names in six of the twelve. The finalizer is what makes
    /// the bits it is asked for the bits the whole name touched.
    private static func anchor(of name: String) -> UInt32 {
        var hash: UInt32 = 2_166_136_261
        for byte in name.utf8 {
            hash = (hash ^ UInt32(byte)) &* 16_777_619
        }
        hash ^= hash >> 16
        hash = hash &* 0x85eb_ca6b
        hash ^= hash >> 13
        hash = hash &* 0xc2b2_ae35
        hash ^= hash >> 16
        return hash % 12
    }
}

/// A pane of the window's own glass, behind a SwiftUI row.
///
/// SwiftUI has no glass in this SDK — the module interface has no
/// `glassEffect` in it — so the row borrows the same `NSGlassEffectView` the
/// tab strip and the chrome buttons are built from. Bridged rather than
/// imitated: an imitation would drift away from the real thing the first time
/// the system changed what glass looks like.
private struct GlassRow: NSViewRepresentable {
    let cornerRadius: CGFloat
    let tint: NSColor

    func makeNSView(context: Context) -> NSView {
        let view = Glass.lozenge(cornerRadius: cornerRadius) ?? NSView()
        view.wantsLayer = true
        view.layer?.cornerCurve = .continuous
        return view
    }

    func updateNSView(_ view: NSView, context: Context) {
        if Glass.isAvailable {
            Glass.setCornerRadius(view, cornerRadius)
            Glass.tint(view, tint)
        } else {
            // Before glass: the flat fill this used to be, in the same colour
            // glass would have been aimed at.
            view.layer?.cornerRadius = cornerRadius
            view.layer?.backgroundColor = tint.withAlphaComponent(0.34).cgColor
        }
    }
}

/// Colour named the way this project reasons about it.
///
/// Lightness and chroma are held while the hue moves, which is the whole point
/// of asking for it in this space: hues picked off an HSL wheel come out with
/// yellow blazing and blue sunk, and a set of workspace colours chosen that way
/// would have one row shouting and another invisible.
private enum OKLCH {
    /// The same colour, for the AppKit half of the window.
    static func appKitColor(
        _ lightness: Double, _ chroma: Double, _ hue: Double, alpha: Double = 1
    ) -> NSColor {
        let (red, green, blue) = components(lightness, chroma, hue)
        return NSColor(srgbRed: red, green: green, blue: blue, alpha: alpha)
    }

    static func color(
        _ lightness: Double, _ chroma: Double, _ hue: Double, opacity: Double = 1
    ) -> Color {
        let (red, green, blue) = components(lightness, chroma, hue)
        return Color(.sRGB, red: red, green: green, blue: blue, opacity: opacity)
    }

    private static func components(
        _ lightness: Double, _ chroma: Double, _ hue: Double
    ) -> (Double, Double, Double) {
        let radians = hue * .pi / 180
        let a = chroma * cos(radians)
        let b = chroma * sin(radians)

        let l = pow(lightness + 0.3963377774 * a + 0.2158037573 * b, 3)
        let m = pow(lightness - 0.1055613458 * a - 0.0638541728 * b, 3)
        let s = pow(lightness - 0.0894841775 * a - 1.2914855480 * b, 3)

        return (
            encode(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
            encode(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
            encode(-0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s)
        )
    }

    /// Linear light to sRGB, clamped: a hue and chroma that fall outside what
    /// a display can show are brought to the nearest thing it can.
    private static func encode(_ channel: Double) -> Double {
        let value = max(0, min(1, channel))
        return value <= 0.0031308
            ? value * 12.92
            : 1.055 * pow(value, 1 / 2.4) - 0.055
    }
}
