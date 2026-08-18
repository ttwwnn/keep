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
            List(selection: Binding(
                get: { store.selectedWorkspace },
                set: { if let name = $0 { store.select(workspace: name) } }
            )) {
                Section("Workspaces") {
                    ForEach(store.workspaces) { workspace in
                        row(for: workspace).tag(workspace.name)
                    }
                }
            }
            .listStyle(.sidebar)
            // Let the split view's sidebar material show through instead of
            // the list painting its own opaque background.
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
