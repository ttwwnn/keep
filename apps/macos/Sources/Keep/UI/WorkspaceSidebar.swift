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
                Spacer(minLength: 6)
                // How many tabs, as a number. The dot already says how the
                // workspace is doing, and a second line of prose per row made
                // three workspaces look like six things.
                if row.tabs > 0 {
                    Text("\(row.tabs)")
                        .font(counter)
                        .foregroundStyle(row.isActive ? Palette.inkResting : Palette.inkFaint)
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .background(
            RoundedRectangle(cornerRadius: 5, style: .continuous)
                .fill(background(for: row))
        )
        .animation(.easeOut(duration: 0.16), value: row.isActive)
        .help(row.subtitle)
        .listRowInsets(EdgeInsets(top: 1, leading: 6, bottom: 1, trailing: 6))
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
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(
            RoundedRectangle(cornerRadius: 5, style: .continuous)
                .fill(fieldFocused ? Color.white.opacity(0.09) : .clear)
        )
        .animation(.easeOut(duration: 0.16), value: fieldFocused)
        .listRowInsets(EdgeInsets(top: 3, leading: 6, bottom: 1, trailing: 6))
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

/// Colour named the way this project reasons about it.
///
/// Lightness and chroma are held while the hue moves, which is the whole point
/// of asking for it in this space: hues picked off an HSL wheel come out with
/// yellow blazing and blue sunk, and a set of workspace colours chosen that way
/// would have one row shouting and another invisible.
private enum OKLCH {
    static func color(
        _ lightness: Double, _ chroma: Double, _ hue: Double, opacity: Double = 1
    ) -> Color {
        let radians = hue * .pi / 180
        let a = chroma * cos(radians)
        let b = chroma * sin(radians)

        let l = pow(lightness + 0.3963377774 * a + 0.2158037573 * b, 3)
        let m = pow(lightness - 0.1055613458 * a - 0.0638541728 * b, 3)
        let s = pow(lightness - 0.0894841775 * a - 1.2914855480 * b, 3)

        let red = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s
        let green = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s
        let blue = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s

        return Color(
            .sRGB,
            red: encode(red), green: encode(green), blue: encode(blue), opacity: opacity)
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
