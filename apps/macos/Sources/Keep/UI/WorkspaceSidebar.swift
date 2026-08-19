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
                Section {
                    ForEach(rows) { row in
                        rowButton(row)
                    }
                } header: {
                    Text("Workspaces")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(.tertiary)
                        .padding(.leading, 4)
                        .padding(.bottom, 2)
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
            HStack(spacing: 8) {
                Circle()
                    .fill(dotColor(row.dot))
                    .frame(width: 7, height: 7)
                VStack(alignment: .leading, spacing: 1) {
                    Text(row.name)
                        .font(.system(size: 13))
                        .fontWeight(row.isActive ? .medium : .regular)
                        .lineLimit(1)
                    Text(row.subtitle)
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        // Native sidebars inset their selection and round it; a bar running
        // edge to edge reads as a divider between two halves rather than as
        // one row being chosen.
        .background(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .fill(background(for: row))
        )
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
        if row.isActive { return Color.accentColor.opacity(0.22) }
        if hovered == row.name { return Color.primary.opacity(0.06) }
        return .clear
    }

    /// One affordance, not a field beside a button: typing a name and
    /// pressing return is the whole gesture, and the plus is there for the
    /// people who look for a plus.
    private var footer: some View {
        HStack(spacing: 6) {
            Image(systemName: "plus")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.tertiary)
            TextField("New workspace", text: $newName)
                .textFieldStyle(.plain)
                .font(.system(size: 12))
                .focused($fieldFocused)
                .onSubmit(create)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
        .background(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .fill(Color.primary.opacity(fieldFocused ? 0.09 : 0.05))
        )
        .padding(.horizontal, 8)
        .padding(.bottom, 8)
        .contentShape(Rectangle())
        .onTapGesture { fieldFocused = true }
    }

    private func dotColor(_ dot: SessionSnapshot.SidebarRow.Dot) -> Color {
        switch dot {
        case .empty: return .secondary
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
