import Foundation

// Which AI and which account each tab runs on, and the menu that changes it.
//
// Foundation only, like AIUsage.swift, so it is proved with a bare `swiftc`
// and a test main (tools/usage-test.sh). The core (`keep ia`) decides and
// does everything here that touches an account or a credential; what lives
// in this file is reading what it says about the tabs, and deciding what
// the menu offers.

/// What a tab is running, as far as its AI menu is concerned.
///
/// Read from the name of the process holding its terminal (the daemon's
/// `command`). A shell is somewhere an AI can be started; Claude Code and
/// Codex can be moved to another account; anything else is somebody else's
/// program, and the menu keeps its hands off it.
enum AIProgramKind: Equatable {
    case shell
    case claude
    case codex
    /// Another program, by name: the menu says so and offers nothing.
    case other(String)
    /// Nothing said — a daemon too old to name the process. Not held against
    /// the tab: the core looks for itself before it touches anything.
    case unknown

    init(command: String) {
        // A login shell is named with a dash in front of it.
        let name = command.hasPrefix("-") ? String(command.dropFirst()) : command
        if name.isEmpty {
            self = .unknown
        } else if Self.shells.contains(name) {
            self = .shell
        } else if name == "claude" || Self.isVersionNumber(name) {
            // Claude Code's own installer keeps each release as a file named
            // after its version, and the process is called what the file is.
            self = .claude
        } else if name == "codex" {
            self = .codex
        } else {
            self = .other(name)
        }
    }

    static let shells: Set<String> = ["zsh", "bash", "fish", "sh", "dash", "ksh", "tcsh", "csh", "nu"]

    private static func isVersionNumber(_ name: String) -> Bool {
        let parts = name.split(separator: ".", omittingEmptySubsequences: false)
        return parts.count >= 2 && parts.allSatisfy { !$0.isEmpty && $0.allSatisfy(\.isNumber) }
    }

    /// Whether the menu may do anything to the tab.
    var allowsChoice: Bool {
        if case .other = self { return false }
        return true
    }

    /// Whether an AI is what runs there, so an account the core said the
    /// tab is on is still the tab's: once the tab is back at its shell, that
    /// account is history.
    var runsAI: Bool {
        switch self {
        case .claude, .codex, .unknown: return true
        case .shell, .other: return false
        }
    }
}

/// One AI running in a tab, as `keep ia abas` describes it: the account it
/// was put on, and the one it runs on now.
struct AITabAccount: Equatable {
    let workspace: String
    let tab: UInt32
    /// "claude" or "codex".
    let agent: String
    /// What the tab was put on: `claude:ordem` (following the order of
    /// priority), `claude:<name>` or `gpt:<name>`.
    let key: String
    /// The account it runs on now, when the core could tell — for a tab
    /// following the order, the account the order has it on.
    let running: String?
    /// "exato" when the core tied the process to the tab for certain.
    let link: String
}

/// The account each tab runs on, as the core last said.
///
/// `keep ia abas` reads it off each tab's processes — which program holds
/// the tab, and the account it was started on — every few seconds while the
/// app is in front, and when a tab's menu opens (`AITabWatcher`). An answer
/// counts only for the daemon the app is using: tab numbers are that
/// daemon's, and the answer says which daemon it read — by its start to the
/// millisecond when the daemon knows the third list (`exato`), and by its
/// process when it is older.
///
/// A change the app itself just asked for is shown before the core can see
/// it (`note`), and for as long as no answer asked for after it was made has
/// come.
@MainActor
final class AITabAccounts {
    private var entries: [String: AITabAccount] = [:]
    /// Accounts the core has just said yes to, by tab, and when (unix ms).
    private var notes: [String: (key: String, at: Double)] = [:]
    /// When the answer in hand was asked for (unix ms).
    private var asked: Double = 0
    /// The daemon the answers in hand are about: the birth of its socket.
    private var daemon: Date?

    init() {}

    /// The daemon the app is talking to, by the birth of its socket. Another
    /// one than before, and what was known is about tabs that are gone.
    /// True when what some tab shows has changed.
    func daemon(_ start: Date?) -> Bool {
        guard start != daemon else { return false }
        let before = shown
        daemon = start
        entries = [:]
        notes = [:]
        asked = 0
        return shown != before
    }

    /// An answer of `keep ia abas`, asked for at `askedAt` (unix ms), taken
    /// when it is about the daemon the app is using: process `daemonPid`,
    /// started at `daemonStartedMs` (unix ms, from its third list; nil from a
    /// daemon too old to say). An answer asked for before the one in hand is
    /// older news, and dropped. True when what some tab shows has changed.
    func adopt(_ data: Data, askedAt: Double, daemonPid: UInt32?, daemonStartedMs: UInt64?) -> Bool {
        guard askedAt >= asked,
              let read = Self.parse(data, daemonPid: daemonPid, daemonStartedMs: daemonStartedMs)
        else { return false }
        let before = shown
        entries = Dictionary(
            read.map { (Self.id($0.workspace, $0.tab), $0) }, uniquingKeysWith: { first, _ in first })
        asked = askedAt
        // What the core read after a change was made is the word on it now,
        // whatever it says.
        notes = notes.filter { $0.value.at > askedAt }
        return shown != before
    }

    /// The core said yes to putting this tab on `key`. Shown from now until
    /// an answer asked for after this comes.
    func note(workspace: String, tab: UInt32, key: String, at: Date = Date()) {
        notes[Self.id(workspace, tab)] = (key, at.timeIntervalSince1970 * 1000)
    }

    /// The account a tab was put on, or nil when it runs no AI.
    ///
    /// A tab running Claude that the core has not described yet — started
    /// since it last answered — is on the order, since that is what Claude
    /// started without an account of its own runs on; a Codex likewise on
    /// Codex's own login.
    func account(workspace: String, tab: UInt32, program: AIProgramKind) -> String? {
        guard program.runsAI else { return nil }
        let id = Self.id(workspace, tab)
        if let note = notes[id] { return note.key }
        if let entry = entries[id] { return entry.key }
        switch program {
        case .claude: return AIHelper.followOrder
        case .codex: return "gpt:principal"
        default: return nil
        }
    }

    /// The account the tab's AI runs on now, when the core said so and the
    /// app has asked for no change since. Nil for a tab whose account the
    /// core could not tell: the menu then marks what it marked before.
    func running(workspace: String, tab: UInt32, program: AIProgramKind) -> String? {
        guard program.runsAI else { return nil }
        let id = Self.id(workspace, tab)
        guard notes[id] == nil else { return nil }
        return entries[id]?.running
    }

    /// Only an account the core said for the program actually on screen may
    /// light the usage footer — the one the tab runs on when it said that,
    /// else the one it was put on. A stale Claude record must not label a
    /// new Codex.
    func confirmedAccount(workspace: String, tab: UInt32, program: AIProgramKind) -> String? {
        let id = Self.id(workspace, tab)
        let codex: Bool
        switch program {
        case .claude: codex = false
        case .codex: codex = true
        // An older daemon names the shell of a tab whose AI has a child of
        // its own in front (an MCP server, a php it ran); the core looked
        // inside the shell, and its word is the one to go by.
        case .shell, .unknown:
            guard let agent = entries[id]?.agent, agent == "claude" || agent == "codex" else { return nil }
            codex = agent == "codex"
        case .other: return nil
        }
        let prefix = codex ? "gpt:" : "claude:"
        if let note = notes[id]?.key { return note.hasPrefix(prefix) ? note : nil }
        guard let entry = entries[id] else { return nil }
        if let running = entry.running, running.hasPrefix(prefix) { return running }
        return entry.key.hasPrefix(prefix) ? entry.key : nil
    }

    /// What the tabs show, for telling whether an answer changed it.
    private var shown: [String: String] {
        var map = entries.mapValues { "\($0.key) \($0.running ?? "-")" }
        for (id, note) in notes { map[id] = note.key }
        return map
    }

    private static func id(_ workspace: String, _ tab: UInt32) -> String {
        "\(workspace)\u{1F}\(tab)"
    }

    /// The answer's "ia" list, or nil when it is not one to believe: not the
    /// contract's (`versao` 1, `ok`), or read from another daemon than the
    /// one the app is using. The core says which (`keepd`): read with the
    /// third list (`exato`), its start, which must be the app's to the
    /// millisecond; from an older daemon, its process, which must be the one
    /// at the other end of the app's socket. Nothing known of the app's
    /// daemon yet, nothing is believed.
    nonisolated static func parse(
        _ data: Data, daemonPid: UInt32?, daemonStartedMs: UInt64?
    ) -> [AITabAccount]? {
        guard let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              (root["versao"] as? NSNumber)?.intValue == 1,
              root["ok"] as? Bool == true
        else { return nil }
        let keepd = root["keepd"] as? [String: Any]
        let saidStart = (keepd?["inicioMs"] as? NSNumber)?.uint64Value
        let saidPid = (keepd?["pid"] as? NSNumber)?.uint32Value
        let exact = root["exato"] as? Bool ?? false
        if exact, let mine = daemonStartedMs {
            guard let saidStart, max(saidStart, mine) - min(saidStart, mine) <= 1 else { return nil }
        } else {
            guard let mine = daemonPid, let saidPid, saidPid == mine else { return nil }
        }
        var entries: [AITabAccount] = []
        for item in (root["ia"] as? [[String: Any]]) ?? [] {
            guard let workspace = item["workspace"] as? String,
                  let tab = (item["aba"] as? NSNumber)?.uint32Value,
                  let key = item["conta"] as? String, AIHelper.isValidKey(key)
            else { continue }
            let running = (item["atual"] as? String).flatMap {
                AIHelper.isValidKey($0) && $0 != AIHelper.followOrder ? $0 : nil
            }
            entries.append(AITabAccount(
                workspace: workspace, tab: tab,
                agent: item["agente"] as? String ?? "",
                key: key,
                running: running,
                link: item["vinculo"] as? String ?? ""))
        }
        return entries
    }
}

/// What a tab's AI menu offers, decided from the footer's lines — the order
/// of priority, and whether each account can take work now — the account the
/// tab is on, and what it is running. The same menu wherever it opens: the
/// strip's chevron and the sidebar's.
enum AIChoices {
    enum Mark: Equatable {
        case none
        /// The tab is on this.
        case on
        /// The tab is on the order, and the order is on this.
        case mixed
    }

    struct Row: Equatable {
        enum Kind: Equatable { case note, follow, separator, account }
        let kind: Kind
        let title: String
        /// What choosing it asks the core for; nil for what is not a choice.
        let key: String?
        let mark: Mark
        let enabled: Bool
        let help: String?
        /// What the choice is called when it is spoken of afterwards — in a
        /// question about going on with it: "Claude · reserva".
        var label: String = ""
    }

    static let followTitle = "Seguir a ordem de prioridade"

    /// The menu, top to bottom.
    ///
    /// A tab running some other program is told so first, and nothing in it
    /// can be chosen: the core will not type into a program it does not
    /// know. Then following the order, and one line per account in the
    /// order's order — the accounts that cannot take work now say so, and
    /// can still be chosen, since that is the person's call.
    ///
    /// Following the order is asked as such (`claude:ordem`), whatever
    /// account comes first in it now: the core resolves it — to a GPT
    /// account too, when every Claude one is at its limit — and keeps the tab
    /// following it. Asking for that account instead would fix the tab on it.
    ///
    /// `current` is what the tab was put on; `running`, the account it runs
    /// on now, when the core could tell. A tab following the order has a dash
    /// on that account — or, not told, on the Claude account whose login the
    /// tabs share.
    static func rows(
        lines: [AccountUsage], current: String?, running: String? = nil, program: AIProgramKind
    ) -> [Row] {
        var rows: [Row] = []
        let enabled = program.allowsChoice
        if case .other(let name) = program {
            rows.append(Row(
                kind: .note, title: "Esta aba está rodando \(name)", key: nil, mark: .none,
                enabled: false, help: nil))
        }
        let now = (lines.first(where: \.isAvailable) ?? lines.first).map { "Agora: \($0.account.name)" }
        rows.append(Row(
            kind: .follow, title: followTitle, key: AIHelper.followOrder,
            mark: current == AIHelper.followOrder ? .on : .none, enabled: enabled, help: now,
            label: "a ordem de prioridade"))
        guard !lines.isEmpty else { return rows }
        rows.append(Row(kind: .separator, title: "", key: nil, mark: .none, enabled: false, help: nil))
        for line in lines {
            let account = line.account
            var mark = Mark.none
            if let current, account.keys.contains(current) {
                mark = .on
            } else if current == AIHelper.followOrder {
                if let running {
                    if account.keys.contains(running) { mark = .mixed }
                } else if account.engine == .claude, account.isActive {
                    mark = .mixed
                }
            }
            rows.append(Row(
                kind: .account, title: title(line), key: account.engine.key(account.alias),
                mark: mark, enabled: enabled, help: nil, label: account.name))
        }
        return rows
    }

    /// "Claude · reserva · ds.lw.tm@… — no limite": the service and the
    /// order's name for it, the address the service knows it by, and — when
    /// it cannot take work now — why.
    static func title(_ line: AccountUsage) -> String {
        var title = line.account.name
        if let email = line.account.email { title += " · \(email)" }
        if !line.isAvailable { title += " — \(line.account.warning ?? "no limite")" }
        return title
    }


    /// What the sidebar writes after a tab's title: the program, or — for an
    /// AI kept on an account of its own — which one. "claude · reserva";
    /// "codex · outra" for a GPT account other than Codex's own login.
    static func programLabel(command: String, account: String?) -> String {
        guard let account, let colon = account.firstIndex(of: ":") else { return command }
        let service = account[..<colon]
        let alias = String(account[account.index(after: colon)...])
        switch service {
        case "claude" where alias != "ordem": return "claude · \(alias)"
        case "gpt" where alias != "principal": return "codex · \(alias)"
        default: return command
        }
    }
}
