import SwiftUI

struct ContentView: View {
    @State private var sessions: [Daemon.Session] = []
    @State private var selectedSession: String?
    @State private var selectedTab: UInt32?
    @State private var error: String?
    @State private var newName = ""

    private let refresh = Timer.publish(every: 2, on: .main, in: .common).autoconnect()

    var body: some View {
        NavigationSplitView {
            sidebar
        } detail: {
            detail
        }
        .onAppear(perform: reload)
        .onReceive(refresh) { _ in reload() }
    }

    private var current: Daemon.Session? {
        sessions.first { $0.name == selectedSession }
    }

    // MARK: - sidebar

    private var sidebar: some View {
        VStack(spacing: 0) {
            List(selection: $selectedSession) {
                Section("Sessions") {
                    ForEach(sessions) { session in
                        HStack(spacing: 8) {
                            Circle()
                                .fill(dotColor(for: session))
                                .frame(width: 7, height: 7)
                            VStack(alignment: .leading, spacing: 1) {
                                Text(session.name)
                                Text(subtitle(for: session))
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                            }
                        }
                        .tag(session.name)
                        .contextMenu {
                            Button("New Tab") { addTab(to: session.name) }
                            Divider()
                            Button("Kill Session", role: .destructive) { kill(session.name) }
                        }
                    }
                }
            }

            Divider()
            HStack(spacing: 6) {
                TextField("new session", text: $newName)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit(createSession)
                Button(action: createSession) { Image(systemName: "plus") }
                    .disabled(newName.isEmpty)
            }
            .padding(8)
        }
        .navigationSplitViewColumnWidth(min: 200, ideal: 230)
    }

    private func dotColor(for session: Daemon.Session) -> Color {
        if session.liveTabs.isEmpty { return .secondary }
        return session.clients > 0 ? .green : .orange
    }

    private func subtitle(for session: Daemon.Session) -> String {
        let n = session.liveTabs.count
        let tabs = n == 1 ? "1 tab" : "\(n) tabs"
        return "\(tabs) · \(session.stateLabel)"
    }

    // MARK: - detail

    @ViewBuilder
    private var detail: some View {
        if let error {
            ContentUnavailableView(
                "Daemon unreachable",
                systemImage: "bolt.horizontal.circle",
                description: Text(error)
            )
        } else if let session = current {
            VStack(spacing: 0) {
                tabBar(for: session)
                Divider()
                terminal(for: session)
            }
        } else {
            ContentUnavailableView(
                "No session selected",
                systemImage: "terminal",
                description: Text("Pick a session, or create one below the list.")
            )
        }
    }

    private func tabBar(for session: Daemon.Session) -> some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(session.liveTabs) { tab in
                    let isSelected = tab.id == selectedTab
                    Button {
                        selectedTab = tab.id
                    } label: {
                        HStack(spacing: 6) {
                            Text(tab.label)
                                .font(.system(size: 11, weight: isSelected ? .semibold : .regular))
                                .lineLimit(1)
                                .truncationMode(.middle)
                                .frame(maxWidth: 170)
                                .help(tab.title.isEmpty ? "Tab \(tab.id)" : tab.title)
                            if session.liveTabs.count > 1 {
                                Button {
                                    closeTab(tab.id, in: session.name)
                                } label: {
                                    Image(systemName: "xmark")
                                        .font(.system(size: 8, weight: .bold))
                                }
                                .buttonStyle(.plain)
                                .foregroundStyle(.secondary)
                            }
                        }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 5)
                        .background(
                            RoundedRectangle(cornerRadius: 6)
                                .fill(isSelected ? Color.accentColor.opacity(0.22) : Color.clear)
                        )
                    }
                    .buttonStyle(.plain)
                }

                Button {
                    addTab(to: session.name)
                } label: {
                    Image(systemName: "plus").font(.system(size: 10, weight: .bold))
                }
                .buttonStyle(.plain)
                .padding(.horizontal, 6)
                .help("New tab")
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
        }
    }

    @ViewBuilder
    private func terminal(for session: Daemon.Session) -> some View {
        if let tab = selectedTab ?? session.liveTabs.first?.id {
            // Keyed by session and tab: switching either must build a fresh
            // surface, not reuse one still pointed at the previous terminal.
            TerminalSurface(command: "\(Self.keepBinary) \(session.name) --tab \(tab)")
                .id("\(session.name)#\(tab)")
        } else {
            ContentUnavailableView(
                "No tabs",
                systemImage: "rectangle.stack.badge.plus",
                description: Text("Press + to open one.")
            )
        }
    }

    /// The client binary a surface runs. Looked up next to the app first so a
    /// bundled copy wins over whatever happens to be on PATH.
    private static var keepBinary: String {
        if let override = ProcessInfo.processInfo.environment["KEEP_BIN"] { return override }
        // Resources, not MacOS: the app executable is "Keep" and macOS
        // filesystems are case-insensitive, so a sibling named "keep" would
        // overwrite the app itself.
        let bundled = Bundle.main.bundleURL
            .appendingPathComponent("Contents/Resources/keep").path
        if FileManager.default.isExecutableFile(atPath: bundled) { return bundled }
        return "keep"
    }

    // MARK: - actions

    private func reload() {
        do {
            try Daemon.ensureRunning()
            sessions = try Daemon.list()
            error = nil

            if selectedSession == nil { selectedSession = sessions.first?.name }
            // Keep the tab selection valid: it may have been closed or the
            // session switched underneath us.
            if let session = current {
                let live = session.liveTabs.map(\.id)
                if selectedTab == nil || !live.contains(selectedTab!) {
                    selectedTab = live.first
                }
            } else {
                selectedTab = nil
            }
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func createSession() {
        let name = newName.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else { return }
        newName = ""
        selectedSession = name
        selectedTab = nil
        // Attaching is what brings the session into being; the surface does it.
        reload()
    }

    private func addTab(to session: String) {
        if let id = try? Daemon.newTab(in: session) {
            selectedSession = session
            selectedTab = id
        }
        reload()
    }

    private func closeTab(_ tab: UInt32, in session: String) {
        try? Daemon.closeTab(tab, in: session)
        if selectedTab == tab { selectedTab = nil }
        reload()
    }

    private func kill(_ name: String) {
        try? Daemon.kill(name)
        if selectedSession == name {
            selectedSession = nil
            selectedTab = nil
        }
        reload()
    }
}
