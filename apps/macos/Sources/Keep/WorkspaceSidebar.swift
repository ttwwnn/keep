import SwiftUI

/// The workspace list, shown inside every tab.
///
/// Native tabs are separate windows, so the sidebar lives in each one — the
/// same arrangement Finder uses. On screen it reads as one persistent sidebar
/// beside a tab bar.
struct WorkspaceSidebar: View {
    @ObservedObject var store: Store
    @State private var newName = ""

    var body: some View {
        VStack(spacing: 0) {
            // No `List(selection:)`. That state belongs to SwiftUI, which
            // writes it for reasons that have nothing to do with intent — a
            // freshly revealed window's outline view takes focus and picks a
            // row on its own. Every window carries its own sidebar, so the row
            // it picks is rarely the workspace on screen, and with selection
            // driving navigation each of those writes was a workspace switch
            // nobody asked for. Entering a workspace is a thing you do, so it
            // hangs off the tap and nothing else.
            List {
                Section("Workspaces") {
                    ForEach(store.workspaces) { workspace in
                        // A button rather than a tap gesture: a gesture on a
                        // list row competes with the list's own hit testing and
                        // offers neither keyboard activation nor accessibility.
                        Button {
                            store.select(workspace: workspace.name)
                        } label: {
                            row(for: workspace)
                                // The whole row is the target, not just its text.
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .listRowBackground(
                            workspace.name == store.selectedWorkspace
                                ? Color.accentColor.opacity(0.18)
                                : Color.clear
                        )
                    }
                }
            }
            // The AppKit host owns one flat, full-height surface. Applying
            // SwiftUI's sidebar style here would create another rounded panel
            // below the titlebar.
            .listStyle(.plain)
            // Let the terminal-tinted host show through instead of the list
            // painting its own opaque background.
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

    private func row(for workspace: Daemon.Workspace) -> some View {
        HStack(spacing: 8) {
            Circle()
                .fill(dotColor(for: workspace))
                .frame(width: 7, height: 7)
            VStack(alignment: .leading, spacing: 1) {
                Text(workspace.name)
                Text(subtitle(for: workspace))
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
        .contextMenu {
            Button("New Tab") { store.newTab(in: workspace.name) }
            Divider()
            Button("Close Workspace", role: .destructive) {
                store.kill(workspace: workspace.name)
            }
        }
    }

    private func dotColor(for workspace: Daemon.Workspace) -> Color {
        if workspace.liveTabs.isEmpty { return .secondary }
        if workspace.busy { return .yellow }
        return workspace.clients > 0 ? .green : .orange
    }

    private func subtitle(for workspace: Daemon.Workspace) -> String {
        let n = workspace.liveTabs.count
        return "\(n == 1 ? "1 tab" : "\(n) tabs") · \(workspace.stateLabel)"
    }

    private func create() {
        store.createWorkspace(named: newName)
        newName = ""
    }
}
