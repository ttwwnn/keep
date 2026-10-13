import AppKit

/// Every terminal surface the app has opened, kept alive across the windows
/// that show them.
///
/// A surface is not a piece of a window's layout — it owns a libghostty
/// renderer and a `keep` client process attached to a daemon tab, and building
/// one costs a process spawn, a socket, a shell and a repaint before it has
/// anything to draw. Tying its lifetime to whichever window happens to be
/// displaying it means paying that price every time the arrangement changes.
///
/// So windows borrow surfaces from here. A surface outlives being taken out of
/// one window and put into another; it goes away only when its daemon tab
/// does, when the window that borrowed it closes — or when its tab has been
/// out of sight long enough to be hibernated (`hibernate`; which tabs, is
/// `Hibernation`'s business).
///
/// **Keyed by window as well as by tab.** A surface is an `NSView`, and a view
/// has one superview: two windows showing one tab cannot share it, and trying
/// would silently tear the terminal out of whichever window asked first. So
/// each window gets its own, which means its own `keep` client, which means
/// its own subscriber on the daemon's tab — two clients on one shell, the way
/// two people attached to one tmux session are two clients. The daemon has
/// accepted that since before this app existed.
///
/// Main thread only: this holds views.
final class SurfacePool {
    static let shared = SurfacePool()

    private struct Key: Hashable {
        let window: WindowID
        let workspace: String
        let tab: UInt32
    }

    private var surfaces: [Key: TerminalSurfaceView] = [:]

    /// A surface let go of by `hibernate`, while its tab lives on.
    private struct Sleeper {
        /// The directory the shell last said it was in.
        var directory: String?
        /// Where the pane's client keeps it, until a surface comes back for
        /// it (`Keeper`); nil when it could not be kept, and the daemon's
        /// repaint is all a new surface will get.
        let keeper: URL?
        /// What the kept program says about itself, carried to the app.
        var watch: KeeperWatch?
        /// The size of a cell the surface had, for the facts.
        let cell: (width: UInt32, height: UInt32)?
    }

    /// The surfaces let go of by `hibernate`. Kept by window, as the
    /// surfaces are: an entry goes when that window makes the surface again,
    /// closes, or loses the tab.
    private var asleep: [Key: Sleeper] = [:]

    private init() {}

    /// The surface a window shows a tab through, made on first use.
    ///
    /// Named with the window rather than defaulting to one, so that adding
    /// windows made every caller of the old name fail to compile. A caller
    /// missed here does not misbehave visibly — it hands one window's live
    /// terminal to another, and the first goes blank.
    func surface(window: WindowID, workspace: String, tab: UInt32) -> TerminalSurfaceView {
        let key = Key(window: window, workspace: workspace, tab: tab)
        if let existing = surfaces[key] { return existing }
        let surface = TerminalSurfaceView(workspace: workspace, tab: tab)
        surface.translatesAutoresizingMaskIntoConstraints = false
        if let sleeper = asleep.removeValue(forKey: key) {
            sleeper.watch?.stop()
            // Woken from hibernation: the shell says where it is only at its
            // next prompt, and the screen handed over does not say it at all.
            if let directory = sleeper.directory {
                surface.noteDirectory(directory)
            }
            // Its client takes the pane over from the one that kept it, which
            // then goes, its directory with it. One that is somehow still
            // there a while later is told to go.
            if let keeper = sleeper.keeper {
                surface.takeOver = keeper
                DispatchQueue.main.asyncAfter(deadline: .now() + 30) { Keeper.release(keeper) }
            }
        }
        surfaces[key] = surface
        Trace.log("pool", "made \(window) \(workspace)/\(tab) total=\(surfaces.count)")
        traceScreen(of: surface)
        return surface
    }

    /// One window's surface for a pane, only where it already exists.
    ///
    /// Distinct from `surface(window:workspace:tab:)`, which makes one: asking
    /// a question about a pane must not start a client for it.
    func existing(window: WindowID, workspace: String, tab: UInt32) -> TerminalSurfaceView? {
        surfaces[Key(window: window, workspace: workspace, tab: tab)]
    }

    /// Any window's surface for a pane.
    ///
    /// For questions whose answer cannot differ between windows — what
    /// directory the shell is in, say. They are all watching one shell, so
    /// whichever answers first is right. Anything that acts on a particular
    /// window's view of a tab, like scrolling it to a search hit, must ask for
    /// that window's instead.
    func anyExisting(workspace: String, tab: UInt32) -> TerminalSurfaceView? {
        surfaces.first { $0.key.workspace == workspace && $0.key.tab == tab }?.value
    }

    // MARK: - hibernation

    /// Let go of one window's surface for a tab whose shell lives on.
    ///
    /// Its renderer goes — three render targets of tens of megabytes, which a
    /// surface keeps for as long as it exists, on screen or not — and so does
    /// its `keep` client. The shell is the daemon's and is not touched, nor is
    /// the daemon told anything. The next `surface(window:workspace:tab:)`
    /// makes a new one, whose client the daemon repaints with the screen as it
    /// stands, as when the app is opened onto a tab that kept running.
    ///
    /// The caller takes the surface out of its layout: this only lets go.
    func hibernate(
        window: WindowID, workspace: String, tab: UInt32, onTitle: @escaping (String) -> Void
    ) {
        let key = Key(window: window, workspace: workspace, tab: tab)
        guard let surface = surfaces[key] else { return }
        // Kept by its client, which follows it into a mirror, when it does:
        // told what to answer for the surface, and that a surface will come
        // back for the pane, before this one goes (`Keeper`).
        var keeper: URL?
        let cell = surface.cellPixels
        if surface.isMirrored, let directory = surface.clientDirectory {
            Keeper.writeFacts(to: directory, cell: cell, dark: surface.schemeIsDark)
            let flag = directory.appendingPathComponent(Keeper.flag)
            if FileManager.default.createFile(atPath: flag.path, contents: Data("1\n".utf8)) {
                surface.keepsDirectory = true
                keeper = directory
            }
        }
        var sleeper = Sleeper(directory: surface.currentDirectory, keeper: keeper, watch: nil, cell: cell)
        if let keeper {
            sleeper.watch = KeeperWatch(
                directory: keeper,
                onTitle: onTitle,
                onDirectory: { [weak self] raw in
                    let path = raw.hasPrefix("file://") ? (URL(string: raw)?.path ?? raw) : raw
                    guard !path.isEmpty else { return }
                    self?.asleep[key]?.directory = path
                })
        }
        asleep[key] = sleeper
        Trace.log("pool", "hibernating \(window) \(workspace)/\(tab) kept=\(keeper != nil)")
        drop(key)
    }

    /// Tell every kept pane again what to answer for its surface: the theme
    /// or the appearance changed. `dark` says, per window, what its surfaces
    /// would now say.
    func refreshFacts(dark: (WindowID) -> Bool?) {
        for (key, sleeper) in asleep {
            guard let keeper = sleeper.keeper, let isDark = dark(key.window) else { continue }
            Keeper.writeFacts(to: keeper, cell: sleeper.cell, dark: isDark)
        }
    }

    /// The app is going: no surface will come back for any kept pane.
    func releaseKeepers() {
        for (_, sleeper) in asleep {
            sleeper.watch?.stop()
            if let keeper = sleeper.keeper {
                try? FileManager.default.removeItem(at: keeper.appendingPathComponent(Keeper.flag))
            }
        }
    }

    private func release(_ sleeper: Sleeper) {
        sleeper.watch?.stop()
        if let keeper = sleeper.keeper { Keeper.release(keeper) }
    }

    /// Whether this app showed the pane, and let go of its surface in every
    /// window that did. Not the same as never having had one: a tab nobody
    /// has opened here is not one somebody here is using.
    func isHibernated(workspace: String, tab: UInt32) -> Bool {
        anyExisting(workspace: workspace, tab: tab) == nil
            && asleep.keys.contains { $0.workspace == workspace && $0.tab == tab }
    }

    /// The panes `isHibernated` is true of, each once.
    func hibernatedPanes() -> [(workspace: String, tab: UInt32)] {
        var seen = Set<String>()
        return asleep.keys.compactMap { key in
            guard seen.insert("\(key.workspace)/\(key.tab)").inserted,
                isHibernated(workspace: key.workspace, tab: key.tab)
            else { return nil }
            return (key.workspace, key.tab)
        }
    }

    /// The directory a pane's shell is in: as a surface heard it, or as it
    /// last heard it before being let go of.
    func currentDirectory(workspace: String, tab: UInt32) -> String? {
        if let live = anyExisting(workspace: workspace, tab: tab) { return live.currentDirectory }
        return asleep.first { $0.key.workspace == workspace && $0.key.tab == tab }?.value.directory
    }

    /// Whether a pane's surface was let go of with its client keeping it,
    /// for the tests: `isHibernated` alone says nothing of how.
    func isKept(workspace: String, tab: UInt32) -> Bool {
        asleep.contains { $0.key.workspace == workspace && $0.key.tab == tab && $0.value.keeper != nil }
    }

    /// Where to type into a pane: the surface of a window that has one, or,
    /// for a pane hibernated in every window, the daemon. Nil for a pane this
    /// app never showed, as `anyExisting` is.
    func typist(workspace: String, tab: UInt32) -> PaneTypist? {
        if let live = anyExisting(workspace: workspace, tab: tab) { return PaneTypist(live) }
        guard isHibernated(workspace: workspace, tab: tab) else { return nil }
        return PaneTypist(workspace: workspace, tab: tab)
    }

    /// For the test of hibernation (tools/hibernate-test.sh): what a new
    /// surface shows a few seconds after it was made — after a hibernation,
    /// the screen the daemon repainted. Terminal content is not something a
    /// trace records, so it takes `KEEP_TRACE_SCREEN` as well as the trace.
    private func traceScreen(of surface: TerminalSurfaceView) {
        guard Trace.enabled, ProcessInfo.processInfo.environment["KEEP_TRACE_SCREEN"] != nil
        else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + 3) { [weak surface] in
            guard let surface else { return }
            let rows = surface.visibleText().lines
                .map { $0.trimmingCharacters(in: .whitespaces) }
                .filter { !$0.isEmpty }
            Trace.log("screen", "\(surface.workspace)/\(surface.tab) \(rows.joined(separator: "|"))")
        }
        // The whole screen, history first, a second at a time, into a file
        // the test reads: `KEEP_SCREEN_DUMP=<directory>`.
        guard let dump = ProcessInfo.processInfo.environment["KEEP_SCREEN_DUMP"] else { return }
        let file = URL(fileURLWithPath: dump)
            .appendingPathComponent("\(surface.workspace)-\(surface.tab).txt")
        Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak surface] timer in
            guard let surface, surface.window != nil else {
                timer.invalidate()
                return
            }
            guard let text = surface.screenText() else { return }
            try? text.write(to: file, atomically: true, encoding: .utf8)
        }
    }

    /// Let go of a tab's surfaces, in every window, which tears down their
    /// clients.
    ///
    /// Only for a tab the daemon no longer has. A surface dropped while its
    /// tab is alive would be rebuilt from nothing the next time the tab is
    /// shown, which is the cost this pool exists to avoid.
    func discard(workspace: String, tab: UInt32) {
        forget { $0.workspace == workspace && $0.tab == tab }
        for key in surfaces.keys where key.workspace == workspace && key.tab == tab {
            drop(key)
        }
    }

    /// Drop every surface of a workspace, for a workspace that is being killed.
    func discardAll(workspace: String) {
        forget { $0.workspace == workspace }
        for key in surfaces.keys where key.workspace == workspace { drop(key) }
    }

    /// Drop everything one window was showing, for a window that has closed.
    ///
    /// Only that window's: the others are still on screen, and a tab mirrored
    /// into two windows keeps the surface belonging to the one that stayed.
    func discardAll(window: WindowID) {
        forget { $0.window == window }
        for key in surfaces.keys where key.window == window { drop(key) }
    }

    /// Hibernated panes no surface will come back for: their keepers are told.
    private func forget(where matches: (Key) -> Bool) {
        for (key, sleeper) in asleep where matches(key) {
            release(sleeper)
            asleep[key] = nil
        }
    }

    private func drop(_ key: Key) {
        guard let surface = surfaces.removeValue(forKey: key) else { return }
        surface.removeFromSuperview()
        Trace.log(
            "pool", "discarded \(key.window) \(key.workspace)/\(key.tab) total=\(surfaces.count)")
    }

    /// Tabs any window holds a surface for, or hibernated one for, so callers
    /// can reconcile: a tab the daemon lost is forgotten either way. A tab
    /// mirrored into two windows appears once.
    func tabs(in workspace: String) -> Set<UInt32> {
        Set((Array(surfaces.keys) + Array(asleep.keys)).filter { $0.workspace == workspace }.map(\.tab))
    }
}

/// Typing into a pane, whether or not this app still holds a surface for it.
///
/// Through the surface where there is one, as the terminal's own `text:`
/// binding action. Through the daemon where the surface was hibernated: the
/// action's text, as bytes, written to the shell (`Daemon.sendInput`) — what
/// the surface's client would have forwarded. Only `text:` actions go that
/// way; anything else needs a terminal, and is refused. Main thread only,
/// like the pool.
struct PaneTypist {
    private let surface: TerminalSurfaceView?
    private let workspace: String
    private let tab: UInt32

    init(_ surface: TerminalSurfaceView) {
        self.surface = surface
        workspace = surface.workspace
        tab = surface.tab
    }

    init(workspace: String, tab: UInt32) {
        surface = nil
        self.workspace = workspace
        self.tab = tab
    }

    @discardableResult
    func perform(_ action: String) -> Bool {
        if let surface { return surface.perform(action) }
        guard let bytes = TextAction.bytes(of: action) else {
            Trace.log("pool", "\(workspace)/\(tab) is hibernated; cannot \(action)")
            return false
        }
        let workspace = workspace, tab = tab
        Trace.log("pool", "\(workspace)/\(tab) is hibernated; typed through the daemon: \(action)")
        DispatchQueue.global(qos: .utility).async {
            do {
                try Daemon.sendInput(bytes, workspace: workspace, tab: tab)
            } catch {
                Trace.log("pool", "\(workspace)/\(tab) could not be typed into: \(error)")
            }
        }
        return true
    }
}
