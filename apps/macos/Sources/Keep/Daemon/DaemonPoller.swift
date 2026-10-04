import AppKit
import Foundation

/// Polls the daemon every two seconds and hands the listing to the session.
///
/// The socket work happens off the main thread: `Daemon.list()` is blocking
/// I/O, and a daemon busy in its parser must never become a UI hitch. Only
/// the reconciliation runs on the main actor.
@MainActor
final class DaemonPoller {
    private let session: Session
    private var timer: Timer?
    private var inFlight = false
    private var lastInfo: Daemon.Info?

    init(session: Session) {
        self.session = session
    }

    func start() {
        timer = Timer.scheduledTimer(withTimeInterval: 2, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.poll() }
        }
    }

    /// When the daemon last answered a poll. The chores that talk to it
    /// through the core (`AIChores`) run only while it does.
    private(set) static var lastAnswer = Date.distantPast

    private func poll() {
        guard !inFlight else { return }
        inFlight = true
        // Which listing this is: one an intent has overtaken is dropped.
        let generation = session.generation
        DispatchQueue.global(qos: .utility).async { [weak self] in
            let listing = try? Daemon.list()
            DispatchQueue.main.async {
                guard let self else { return }
                self.inFlight = false
                if let listing {
                    Self.lastAnswer = Date()
                    self.session.reconcile(listing, asOf: generation)
                    // A daemon the app had not heard say who it is: what the
                    // tabs run on is asked now, the answer having something
                    // to be checked against.
                    let info = Daemon.lastInfo
                    if info != self.lastInfo {
                        self.lastInfo = info
                        AITabWatcher.current?.refresh()
                    }
                }
            }
        }
    }
}

/// Asks the core which AI and which account each tab runs on
/// (`keep ia abas`): every five seconds while the app is in front, every
/// half minute behind other apps, when the daemon says who it is, and when a
/// tab's AI menu opens.
///
/// The core reads it off the processes behind each tab, a moment's work not
/// worth doing every few seconds with nobody looking. One question at a
/// time; one asked for while another is out runs when that one lands.
@MainActor
final class AITabWatcher {
    /// The watcher of this run, for the menus that ask it to look again.
    private(set) static weak var current: AITabWatcher?

    private let session: Session
    private var timer: Timer?
    private var inFlight = false
    private var again = false
    private var lastAsked = Date.distantPast

    nonisolated static let every: TimeInterval = 5
    nonisolated static let behind: TimeInterval = 30

    init(session: Session) {
        self.session = session
    }

    func start() {
        Self.current = self
        timer = Timer.scheduledTimer(withTimeInterval: Self.every, repeats: true) { [weak self] _ in
            Task { @MainActor in
                guard let self else { return }
                let wait = NSApp.isActive ? Self.every : Self.behind
                guard Date().timeIntervalSince(self.lastAsked) >= wait - 0.5 else { return }
                self.refresh()
            }
        }
    }

    /// Ask now, or as soon as the question out lands.
    func refresh() {
        guard !inFlight else {
            again = true
            return
        }
        guard AIHelper.path != nil else { return }
        inFlight = true
        lastAsked = Date()
        DispatchQueue.global(qos: .utility).async { [weak self] in
            let asked = Date().timeIntervalSince1970 * 1000
            let answer = AIHelper.tabs()
            DispatchQueue.main.async {
                guard let self else { return }
                self.inFlight = false
                switch answer {
                case .success(let data):
                    self.session.adoptTabAccounts(data, askedAt: asked)
                case .failure(let problem):
                    Trace.log("ia", "abas: \(problem.detail)")
                }
                if self.again {
                    self.again = false
                    self.refresh()
                }
            }
        }
    }
}

/// The core's chores that nobody asks for: the tabs that follow the order of
/// priority moved off an account that cannot take work (`keep ia
/// sincronizar`, every thirty seconds), and the births of the worktrees
/// conversations make written down while the conversations are still there
/// to be asked (`keep worktrees indexar`, every two minutes) — what tells,
/// when a tab closes, which worktrees were its.
///
/// Off the main thread, each one at a time, and only while the daemon
/// answers: both are about the tabs it holds. The core stays its own judge
/// of the rest — another manager of the accounts installed, it moves no tab
/// and says so.
@MainActor
final class AIChores {
    private var timers: [Timer] = []
    private var syncing = false
    private var indexing = false

    nonisolated static let syncEvery: TimeInterval = 30
    nonisolated static let indexEvery: TimeInterval = 120

    func start() {
        let sync = Timer(timeInterval: Self.syncEvery, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.sync() }
        }
        let index = Timer(timeInterval: Self.indexEvery, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.index() }
        }
        for timer in [sync, index] { RunLoop.main.add(timer, forMode: .common) }
        timers = [sync, index]
    }

    /// The daemon answered a poll in the last few seconds.
    private var daemonUp: Bool { Date().timeIntervalSince(DaemonPoller.lastAnswer) < 10 }

    private func sync() {
        guard !syncing, daemonUp, AIHelper.path != nil else { return }
        syncing = true
        DispatchQueue.global(qos: .utility).async { [weak self] in
            let answer = AIHelper.sync()
            DispatchQueue.main.async {
                self?.syncing = false
                switch answer {
                case .success(let round):
                    let moved = (round["alteradas"] as? [Any])?.count ?? 0
                    if moved > 0 {
                        Trace.log("ia", "sincronizar: \(moved) aba(s) para \(round["alvo"] as? String ?? "?")")
                        // What the tabs run on now, without waiting the five
                        // seconds.
                        AITabWatcher.current?.refresh()
                    }
                case .failure(let problem):
                    Trace.log("ia", "sincronizar: \(problem.detail)")
                }
            }
        }
    }

    private func index() {
        guard !indexing, daemonUp, Worktrees.helper != nil else { return }
        indexing = true
        DispatchQueue.global(qos: .background).async { [weak self] in
            let problem = Worktrees.index()
            DispatchQueue.main.async {
                self?.indexing = false
                if let problem { Trace.log("worktree", "indexar: \(problem)") }
            }
        }
    }
}

/// Reads Claude Code's permission mode off the screens of the tabs running it.
///
/// Claude Code tells nobody when shift-tab changes its mode — no hook fires
/// and no sequence reaches the terminal — but it writes the mode under its
/// prompt the moment it changes. The daemon holds every tab's screen, hidden
/// tabs included, so asking it once a second is the whole mechanism.
///
/// A second, not two: the mode is something you just changed and are looking
/// for, and the other poll's two seconds are long enough to notice. Only the
/// tabs that look like Claude Code are asked, a few small reads.
@MainActor
final class ClaudeModeWatcher {
    private let session: Session
    private var timer: Timer?
    private var inFlight = false
    /// A bound, not a target: past this many tabs the rest wait a turn.
    private static let perTick = 24

    init(session: Session) {
        self.session = session
    }

    func start() {
        timer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.poll() }
        }
    }

    private func poll() {
        guard !inFlight else { return }
        let tabs = Array(session.aiTabs().prefix(Self.perTick))
        guard !tabs.isEmpty else {
            session.noteClaudeModes([:])
            return
        }
        inFlight = true
        DispatchQueue.global(qos: .utility).async { [weak self] in
            var modes: [TabID: ClaudeMode?] = [:]
            var activities: [TabID: ClaudeActivity] = [:]
            for (id, program) in tabs {
                guard let screen = try? Daemon.preview(workspace: id.workspace, tab: id.root)
                else { continue }
                if program == "codex" {
                    if let activity = ClaudeActivity.readCodex(onScreen: screen) { activities[id] = activity }
                    continue
                }
                if let declared = ClaudeMode.declared(onScreen: screen) { modes[id] = declared }
                // A dialog in front hides the mode but is itself the answer
                // to where the turn stands.
                if let activity = ClaudeActivity.read(onScreen: screen) { activities[id] = activity }
            }
            DispatchQueue.main.async {
                guard let self else { return }
                self.inFlight = false
                self.session.noteClaudeModes(modes, activities: activities)
            }
        }
    }
}
