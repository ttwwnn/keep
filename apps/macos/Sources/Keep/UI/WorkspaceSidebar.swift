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

    /// One list row per workspace: the workspace button, and under it the
    /// tabs it holds. One row and not several, because `.onMove` counts list
    /// rows and the reorder must keep counting workspaces.
    private func rowButton(_ row: SessionSnapshot.SidebarRow) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            workspaceButton(row)
            if row.expanded {
                ForEach(row.tabRows) { tab in tabButton(tab, in: row) }
            }
        }
        .padding(.horizontal, 5)
        // Zero, and the gutter above instead: the plain list keeps insets of
        // its own that a row cannot see, and a block that stops short of both
        // edges by an amount nobody chose looks like a mistake rather than a
        // margin. On the outer stack, not the button — a list-row trait set
        // on a nested child never reaches the list.
        .listRowInsets(EdgeInsets(top: 1, leading: 0, bottom: 1, trailing: 0))
        .listRowSeparator(.hidden)
        .listRowBackground(Color.clear)
    }

    /// The workspace, as a plain header: the name, and the fold at the far
    /// end. Everything the old card said — the dot, what is running, the
    /// count, the path — now belongs to the rows beneath it or to the
    /// tooltip; a header that repeats its children is twice the reading for
    /// the same news.
    private func workspaceButton(_ row: SessionSnapshot.SidebarRow) -> some View {
        Button {
            dispatch(.activateWorkspace(row.name))
        } label: {
            HStack(spacing: 8) {
                Text(row.name)
                    .font(identifier)
                    .foregroundStyle(
                        row.isActive
                            ? Palette.ink
                            : hovered == row.name ? Palette.inkResting : Palette.inkFaint)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if row.dot == .busy {
                    // The one fact worth carrying up from the tabs: something
                    // is running in here, visible with the group folded shut.
                    Text("✳")
                        .font(counter)
                        .foregroundStyle(Palette.busy)
                }
                Spacer(minLength: 8)
                disclosure(row)
            }
            .padding(.horizontal, 12)
            .padding(.top, 10)
            .padding(.bottom, 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(tooltip(for: row))
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
            // Two names, because the two consequences are nothing alike.
            // One tidies this window's list and leaves the work running; the
            // other ends the work, everywhere.
            Button("Remove from This Window") {
                dispatch(.removeWorkspace(row.name))
            }
            Button("Close Workspace", role: .destructive) {
                dispatch(.killWorkspace(row.name))
            }
        }
    }

    /// The fold. Its own button, not part of the workspace's: a chevron that
    /// also entered the workspace would make looking cost a switch.
    ///
    /// Shown only when there is something to fold — and replaced by a spacer
    /// otherwise, so every name in the column starts at the same x.
    @ViewBuilder
    private func disclosure(_ row: SessionSnapshot.SidebarRow) -> some View {
        if row.tabRows.count > 0 {
            Button {
                dispatch(.toggleDisclosure(row.name))
            } label: {
                Image(systemName: "chevron.right")
                    .font(.system(size: 8, weight: .semibold))
                    .foregroundStyle(Palette.inkFaint)
                    .rotationEffect(.degrees(row.expanded ? 90 : 0))
                    .frame(width: 10)
                    .contentShape(Rectangle().inset(by: -6))
            }
            .buttonStyle(.plain)
            .animation(.easeOut(duration: 0.14), value: row.expanded)
        } else {
            Spacer().frame(width: 10)
        }
    }

    /// One tab, under its workspace, wearing the strip's own clothes: the
    /// chosen one is a glass capsule, the others are bare text that brightens
    /// under the pointer, ✳ while busy, ⧉ when another window is showing it.
    /// The same tab in two places should look like the same tab.
    private func tabButton(_ tab: SessionSnapshot.SidebarTab, in row: SessionSnapshot.SidebarRow)
        -> some View
    {
        let chosen = tab.isActive && row.isActive
        let hoveredHere = hovered == tabHoverKey(tab)
        return Button {
            dispatch(.activateTab(tab.id))
        } label: {
            HStack(spacing: 6) {
                if tab.busy {
                    Text("✳")
                        .font(.system(size: 10))
                        .foregroundStyle(Palette.busy)
                }
                Text(tab.title)
                    .font(.system(size: 12, weight: chosen ? .medium : .regular))
                    .foregroundStyle(
                        chosen ? Palette.ink : hoveredHere ? Palette.inkResting : Palette.inkFaint)
                    .lineLimit(1)
                    .truncationMode(.tail)
                if tab.isElsewhere {
                    Image(systemName: "macwindow.on.rectangle")
                        .font(.system(size: 9))
                        .foregroundStyle(Palette.inkFaint)
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 5)
            .contentShape(Rectangle())
            .background(capsule(chosen: chosen, hovered: hoveredHere))
        }
        .buttonStyle(.plain)
        .padding(.leading, 12)
        .padding(.trailing, 8)
        .onHover { inside in
            let key = tabHoverKey(tab)
            hovered = inside ? key : (hovered == key ? nil : hovered)
        }
        .contextMenu {
            Button("Close Tab", role: .destructive) { dispatch(.closeTab(tab.id)) }
        }
    }

    /// The strip's capsule, vertically: glass for the tab being shown, a
    /// breath of white under the pointer, nothing otherwise.
    @ViewBuilder
    private func capsule(chosen: Bool, hovered: Bool) -> some View {
        if chosen {
            GlassRow(cornerRadius: 13, tint: Palette.litRow)
        } else if hovered {
            Capsule(style: .continuous).fill(Color.white.opacity(0.055))
        }
    }

    /// Hover state shares one string field with the workspace rows; a tab's
    /// key must not collide with a workspace named like it.
    private func tabHoverKey(_ tab: SessionSnapshot.SidebarTab) -> String {
        "tab:\(tab.id.workspace)/\(tab.id.root)"
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
            GlassRow(cornerRadius: 12, tint: Palette.litRow)
        } else if hovered == row.name {
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(Color.white.opacity(0.055))
        }
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
            RoundedRectangle(cornerRadius: 12, style: .continuous)
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
    static let litRow = NSColor.white.withAlphaComponent(0.30)

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
