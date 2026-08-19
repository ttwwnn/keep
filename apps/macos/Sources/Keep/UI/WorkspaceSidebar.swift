import SwiftUI

/// The workspace list. Pure function of its rows: snapshot in, intents out.
///
/// Navigation deliberately does NOT hang off `List(selection:)`. That state
/// belongs to SwiftUI, which writes it for reasons that have nothing to do
/// with intent — a freshly revealed outline view takes focus and picks a row
/// on its own, and when selection drove navigation every one of those writes
/// was a workspace switch nobody asked for. Entering a workspace is a thing
/// you do, so it hangs off the button and nothing else.
struct WorkspaceSidebar: View {
    @ObservedObject var model: SidebarRows
    let dispatch: (Intent) -> Void
    @State private var newName = ""

    private var rows: [SessionSnapshot.SidebarRow] { model.rows }

    var body: some View {
        VStack(spacing: 0) {
            List {
                Section("Workspaces") {
                    ForEach(rows) { row in
                        Button {
                            dispatch(.activateWorkspace(row.name))
                        } label: {
                            rowView(row)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .listRowBackground(
                            row.isActive ? Color.accentColor.opacity(0.18) : Color.clear
                        )
                        .contextMenu {
                            Button("New Tab") { dispatch(.newTab(in: row.name)) }
                            Divider()
                            Button("Close Workspace", role: .destructive) {
                                dispatch(.killWorkspace(row.name))
                            }
                        }
                    }
                }
            }
            // The AppKit host owns one flat, full-height surface; the plain
            // style avoids a second rounded panel below the titlebar, and the
            // hidden scroll background lets the terminal tint show through.
            .listStyle(.plain)
            .scrollContentBackground(.hidden)

            Divider()
            HStack(spacing: 6) {
                TextField("new workspace", text: $newName)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit(create)
                Button(action: create) { Image(systemName: "plus") }
                    .disabled(newName.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            .padding(8)
        }
    }

    private func rowView(_ row: SessionSnapshot.SidebarRow) -> some View {
        HStack(spacing: 8) {
            Circle()
                .fill(dotColor(row.dot))
                .frame(width: 7, height: 7)
            VStack(alignment: .leading, spacing: 1) {
                Text(row.name)
                Text(row.subtitle)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
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
