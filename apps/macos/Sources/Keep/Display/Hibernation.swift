import AppKit

/// Puts `HibernationPolicy` to work across every window: the limit is the
/// app's, not a window's, since it is the app that pays for the surfaces.
///
/// Asked a second after a tab is put on screen, and every fifteen seconds for
/// what that leaves out — a tab that held a selection last time, a drag that
/// was under way. Never during a drag, never in the middle of a switch.
@MainActor
final class Hibernation {
    static let shared = Hibernation()

    private let containers = NSHashTable<TabContentContainer>.weakObjects()
    private var clock: UInt64 = 0
    private var pending: DispatchWorkItem?
    private var retry: DispatchWorkItem?
    private var timer: Timer?
    private var backgroundObserver: NSObjectProtocol?
    private var appearanceObserver: NSKeyValueObservation?

    private init() {}

    func register(_ container: TabContentContainer) {
        containers.add(container)
        guard timer == nil else { return }
        timer = Timer.scheduledTimer(withTimeInterval: 15, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.settle() }
        }
        backgroundObserver = NotificationCenter.default.addObserver(
            forName: GhosttyApp.backgroundDidChange, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                Hibernation.tellSchemeToSleepers()
                self?.refreshFacts()
            }
        }
        // Light or dark, as the windows turn: a surface is told it
        // (`applyColorScheme`), and so is a kept pane's client, through the
        // facts — whether or not the theme's colours change with it.
        appearanceObserver = NSApp.observe(\.effectiveAppearance) { [weak self] _, _ in
            DispatchQueue.main.async {
                MainActor.assumeIsolated { self?.refreshFacts() }
            }
        }
    }

    /// The order tabs were put on screen in, across windows.
    func tick() -> UInt64 {
        clock += 1
        return clock
    }

    /// A tab was put on screen. Looked at a second after the last of a run
    /// of these, so that going through tabs one after the other lets go of
    /// nothing until it stops.
    func shown() {
        pending?.cancel()
        let work = DispatchWorkItem { [weak self] in
            MainActor.assumeIsolated { self?.settle() }
        }
        pending = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 1, execute: work)
    }

    /// Hibernate what the policy picks.
    func settle() {
        guard let limit = HibernationPolicy.limit else { return }
        // A window that has closed shows nothing and holds nothing to count.
        let live = containers.allObjects.filter { $0.window != nil }
        guard !live.contains(where: \.isCarrying) else { return }
        // On screen in any window is on screen: a tab one window shows is
        // kept whole in every window that has it.
        let onScreen = Set(live.compactMap(\.visibleTab))

        var hydrated: [(container: TabContentContainer, id: TabID)] = []
        var candidates: [HibernationPolicy.Candidate] = []
        for container in live {
            for (id, host) in container.hosts where host.isHydrated {
                hydrated.append((container, id))
                candidates.append(.init(
                    visible: onScreen.contains(id),
                    lastShown: container.shownAt[id] ?? 0,
                    blocked: host.holdsUnsavedState))
            }
        }
        let chosen = HibernationPolicy.sleepers(candidates, limit: limit)
        // A tab whose client is still taking a pane over is asked about
        // again once it will have: in a few seconds, not the quarter of a
        // minute the timer would take.
        if live.contains(where: { $0.hosts.values.contains(where: \.isTakingOver) }) {
            retry?.cancel()
            let work = DispatchWorkItem { [weak self] in
                MainActor.assumeIsolated { self?.settle() }
            }
            retry = work
            DispatchQueue.main.asyncAfter(deadline: .now() + 3, execute: work)
        }
        guard !chosen.isEmpty else { return }
        for index in chosen {
            hydrated[index].container.hibernate(hydrated[index].id)
        }
        Trace.log("hibernate", "let go of \(chosen.count), kept \(candidates.count - chosen.count)")
    }

    /// The theme changed, and with it what a kept pane's client answers for
    /// its surface: the colours, and dark or light as the window now is.
    /// A second later, as the window takes on the new ground.
    private func refreshFacts() {
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
            guard let self else { return }
            let windows = self.containers.allObjects.compactMap { container -> (WindowID, Bool)? in
                guard let window = container.window else { return nil }
                let dark = window.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
                return (container.windowID, dark)
            }
            SurfacePool.shared.refreshFacts { id in windows.first { $0.0 == id }?.1 }
        }
    }

    /// The colours changed: a hibernated tab whose program asked to be told
    /// (mode 2031) is told through the daemon, as a surface tells its own
    /// (`TerminalSurfaceView.tellScheme`), a second later as a surface does.
    private static func tellSchemeToSleepers() {
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
            guard let dark = GhosttyApp.shared.groundIsDark else { return }
            for pane in SurfacePool.shared.hibernatedPanes() {
                DispatchQueue.global(qos: .utility).async {
                    let screen = (try? Daemon.colouredPreview(workspace: pane.workspace, tab: pane.tab)) ?? ""
                    guard screen.contains("\u{1b}[?2031h") else { return }
                    DispatchQueue.main.async {
                        // Woken meanwhile, it tells its program itself.
                        guard SurfacePool.shared.isHibernated(workspace: pane.workspace, tab: pane.tab)
                        else { return }
                        SurfacePool.shared.typist(workspace: pane.workspace, tab: pane.tab)?
                            .perform("text:\\x1b[?997;\(dark ? 1 : 2)n")
                    }
                }
            }
        }
    }
}
