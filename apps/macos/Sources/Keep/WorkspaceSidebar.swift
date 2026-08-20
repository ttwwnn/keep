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
/// It is drawn to belong to the rest of the window: the chrome is glass and
/// the terminal's own colour, and the one accent-coloured thing left in the
/// app was this list's selection — a blue slab in a window with no other blue
/// in it. Selection is a quiet fill now, and the only colour in here is the
/// dot, which is the only thing colour is carrying information about.
struct WorkspaceSidebar: View {
    @ObservedObject var model: SidebarRows
    let dispatch: (Intent) -> Void
    @State private var newName = ""
    @State private var hovered: String?
    @FocusState private var fieldFocused: Bool

    private var rows: [SessionSnapshot.SidebarRow] { model.rows }

    var body: some View {
        VStack(spacing: 0) {
            List {
                ForEach(rows) { row in
                    rowButton(row)
                }
            }
            // The AppKit host owns one flat, full-height surface; the plain
            // style avoids a second rounded panel below the titlebar, and the
            // hidden scroll background lets the terminal tint show through.
            .listStyle(.plain)
            .scrollContentBackground(.hidden)

            footer
        }
    }

    private func rowButton(_ row: SessionSnapshot.SidebarRow) -> some View {
        Button {
            dispatch(.activateWorkspace(row.name))
        } label: {
            HStack(spacing: 7) {
                Circle()
                    .fill(dotColor(row.dot))
                    .frame(width: 6, height: 6)
                Text(row.name)
                    .font(.system(size: 13))
                    .fontWeight(row.isActive ? .medium : .regular)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 6)
                // How many tabs, as a number rather than as a sentence: the
                // dot already says how the workspace is doing, and two lines
                // of text per row made a list of three workspaces look like a
                // list of six things.
                if row.tabs > 0 {
                    Text("\(row.tabs)")
                        .font(.system(size: 11).monospacedDigit())
                        .foregroundStyle(.tertiary)
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .background(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .fill(background(for: row))
        )
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
            Button("Close Workspace", role: .destructive) {
                dispatch(.killWorkspace(row.name))
            }
        }
    }

    private func background(for row: SessionSnapshot.SidebarRow) -> Color {
        if row.isActive { return Color.primary.opacity(0.13) }
        if hovered == row.name { return Color.primary.opacity(0.06) }
        return .clear
    }

    /// One affordance, not a field beside a button: typing a name and
    /// pressing return is the whole gesture, and the plus is there for the
    /// people who look for a plus.
    ///
    /// It looks like a row until you are typing in it. A box drawn around an
    /// empty field all the time competes with the list for attention and is
    /// the only thing in the sidebar that is always outlined.
    private var footer: some View {
        VStack(spacing: 0) {
            Divider().opacity(0.5)
            HStack(spacing: 7) {
                Image(systemName: "plus")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(fieldFocused ? .secondary : .tertiary)
                    .frame(width: 6)
                TextField("New workspace", text: $newName)
                    .textFieldStyle(.plain)
                    .font(.system(size: 13))
                    .focused($fieldFocused)
                    .onSubmit(create)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(fieldFocused ? Color.primary.opacity(0.10) : .clear)
            )
            .padding(.horizontal, 6)
            .padding(.vertical, 6)
            .contentShape(Rectangle())
            .onTapGesture { fieldFocused = true }
        }
    }

    private func dotColor(_ dot: SessionSnapshot.SidebarRow.Dot) -> Color {
        switch dot {
        case .empty: return Color.primary.opacity(0.25)
        case .busy: return .yellow
        case .attached: return .green
        case .idle: return .orange
        }
    }

    private func create() {
        dispatch(.newWorkspace(named: newName))
        newName = ""
    }
}
