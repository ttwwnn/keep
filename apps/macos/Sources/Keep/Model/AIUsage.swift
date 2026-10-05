import Foundation

// The AI accounts this Mac is signed in to, and how much of each one's
// allowance is spent — the facts behind the sidebar's usage footer.
//
// Found and measured by the core (`keep ia uso`, crates/keep-ia), the same
// code the Windows app runs: it reads the logins Claude Code and Codex keep,
// asks each service, and keeps the readings and the services' pauses. What
// lives here is what the app holds of its answer — no token, ever — and
// what the footer and the menus decide from it.
//
// Foundation only, and no state: proved with a bare `swiftc` and a test
// main (tools/usage-test.sh). The monitor that asks the core, and the view
// that draws its answer, live in the UI layer.

/// Which service an account belongs to.
enum AIEngine: String, Equatable, Codable {
    case claude
    case codex

    /// How the footer names the service: the product, not the vendor.
    var title: String {
        switch self {
        case .claude: return "Claude"
        case .codex: return "GPT"
        }
    }

    /// How the order and the core name the service — `claude:` and `gpt:`,
    /// the product again, since Codex is only the program that runs it.
    var orderPrefix: String {
        switch self {
        case .claude: return "claude"
        case .codex: return "gpt"
        }
    }

    /// The key one of its accounts goes by in the order and to the core:
    /// `claude:reserva`, `gpt:principal`.
    func key(_ alias: String) -> String { "\(orderPrefix):\(alias)" }
}

/// An account, as the core describes it: never with its token, which does
/// not leave the core. The names are the core's (`Conta` in
/// crates/keep-ia), which it writes for this type to read as it is.
struct AIAccountSummary: Equatable, Codable {
    func isUsed(by selectedAccount: String?) -> Bool {
        guard let selectedAccount else { return false }
        if selectedAccount == "claude:ordem" { return engine == .claude && isActive }
        return ([alias] + aliases).contains { engine.key($0) == selectedAccount }
    }

    let engine: AIEngine
    let key: String
    let alias: String
    let aliases: [String]
    let email: String?
    let plan: String?
    let isActive: Bool
    let isPreferred: Bool
    let warning: String?
    /// Optional, both of them, so that a footer saved by a build that had
    /// neither still decodes: a missing key is nil, never an error that
    /// throws the whole saved footer away.
    var orderKey: String? = nil
    /// Whether the core found a token to measure it with — said apart from
    /// the token itself, which never leaves the core.
    var hasToken: Bool? = nil

    /// The line's stable identity: the same account keeps its place and its
    /// fold across reads.
    var id: String { "\(engine.rawValue):\(key)" }

    /// "Claude · reserva": which service, and the name the order knows it by.
    var name: String { "\(engine.title) · \(alias)" }

    /// The key the core is told when this account moves in the order.
    var order: String { orderKey ?? engine.key(alias) }

    /// Every key that names this account, one per slot: a tab fixed on any
    /// of them is on this account.
    var keys: [String] { aliases.map(engine.key) }
}

// MARK: - noticing a change

enum AIAccounts {
    /// What the files the logins live in look like, as cheaply as asking:
    /// the kit's vault (each slot, and `.ativa`, `.preferida`, `.ordem`
    /// beside them), each folder of a login of Keep's own
    /// (`fixas/<id>`, its `keep.json` or the kit's `conta.json`), Codex's
    /// login and each extra one, and the Jev's last reading. Different from
    /// the last time means worth
    /// asking the core again — a login added or renewed, an account switched,
    /// the order changed.
    ///
    /// Stat calls and directory listings; no file is opened. A login made
    /// with the footer's "+" is noticed by this within a couple of seconds,
    /// rather than at the next five-minute reading. The logins the Keychain
    /// holds change nothing here; they come with the next reading.
    static func signature(home: URL) -> [String] {
        let files = FileManager.default
        var parts: [String] = []
        let vault = home.appendingPathComponent(".claude/contas")
        for name in ((try? files.contentsOfDirectory(atPath: vault.path)) ?? []).sorted() {
            let slot = name.hasSuffix(".json") && !name.hasPrefix(".")
            guard slot || [".ativa", ".preferida", ".ordem"].contains(name) else { continue }
            parts.append(name + " " + stamp(vault.appendingPathComponent(name)))
        }
        let own = vault.appendingPathComponent("fixas")
        for name in ((try? files.contentsOfDirectory(atPath: own.path)) ?? []).sorted()
        where !name.hasPrefix(".") {
            let folder = own.appendingPathComponent(name)
            for file in ["keep.json", "conta.json"] {
                parts.append("fixas/\(name)/\(file) " + stamp(folder.appendingPathComponent(file)))
            }
        }
        parts.append("codex " + stamp(home.appendingPathComponent(".codex/auth.json")))
        // The Jev's reading, which the core writes when it measures and when a
        // connection to OpenRouter is made: where the person's app keeps its
        // state, or a test app's made-up home.
        for state in ["Library/Application Support/Keep/ia", ".keep-ia-estado"] {
            parts.append("jev " + stamp(home.appendingPathComponent(state + "/jev.json")))
        }
        let extras = home.appendingPathComponent(".codex-contas")
        for name in ((try? files.contentsOfDirectory(atPath: extras.path)) ?? []).sorted()
        where !name.hasPrefix(".") {
            let auth = extras.appendingPathComponent(name).appendingPathComponent("auth.json")
            parts.append("codex/\(name) " + stamp(auth))
        }
        return parts
    }

    /// A file as `stat` has it: which file (the inode — whoever writes these
    /// replaces them whole), when, and how long. "-" when it is not there.
    static func stamp(_ file: URL) -> String {
        var info = stat()
        guard stat(file.path, &info) == 0 else { return "-" }
        let modified = info.st_mtimespec
        return "\(info.st_ino) \(modified.tv_sec).\(modified.tv_nsec) \(info.st_size)"
    }
}

// MARK: - the core's answer

/// What `keep ia uso` answers: every account in the order of priority —
/// the one handed work first, first — each with what it last said, and the
/// order itself.
struct UsageAnswer: Decodable {
    let linhas: [AccountUsage]
    let ordem: [String]?
    /// Another manager of the accounts is installed (the kit), and it is
    /// the one that switches the login Claude's tabs share.
    let gerenteExterno: Bool?
    let medidoEm: Date?

    /// The answer, or nil when what was printed is not one. Its dates are
    /// seconds since 1970, as the core writes them; the footer's own saved
    /// copy keeps Foundation's default, which the kit reads it with.
    static func decode(_ data: Data) -> UsageAnswer? {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .secondsSince1970
        return try? decoder.decode(UsageAnswer.self, from: data)
    }
}

// MARK: - one account's line

/// One account's place in the footer: who it is, and the last it said.
///
/// Here rather than beside the view, so that what the menus decide from it
/// (`AIChoices`) can be proved without an app.
struct AccountUsage: Identifiable, Equatable, Codable {
    let account: AIAccountSummary
    /// The last reading that came back, kept through later failures: an old
    /// figure marked as old says more than a blank.
    var reading: UsageReading?
    var measuredAt: Date?
    /// Why the latest attempt brought nothing back, when it did not.
    var problem: String?

    init(
        account: AIAccountSummary, reading: UsageReading? = nil, measuredAt: Date? = nil,
        problem: String? = nil
    ) {
        self.account = account
        self.reading = reading
        self.measuredAt = measuredAt
        self.problem = problem
    }

    var id: String { account.id }

    /// Whether this account can take work now, as the core judges it: nothing
    /// known to be wrong with its login, and not at its limit. The service
    /// refuses at 100% of the five-hour or the weekly window, and only there
    /// does an account leave the order — in the kit as in the Clínica's
    /// panel. At 95% the kit moves on ahead of the limit, but only to an
    /// account with room; one past 95% still takes work, and taking it for
    /// one at its limit sent new tabs to GPT while a Claude account still
    /// answered. One not measured yet is given the benefit of the doubt.
    var isAvailable: Bool {
        guard account.warning == nil else { return false }
        guard let reading else { return true }
        if reading.limitReached { return false }
        return !reading.windows.contains { ($0.label == "5h" || $0.label == "7d") && $0.percent >= 100 }
    }
}

// MARK: - a reading

/// One allowance window: a bar in the footer.
struct UsageWindow: Equatable, Codable {
    /// The short name beside the bar ("5h", "7d", "Fable").
    let label: String
    /// The long name, for the tooltip ("Sessão (5h)").
    let title: String
    /// Spent, from 0 to 100 — both services answer in percent.
    let percent: Double
    let resetsAt: Date?
}

/// Everything one answer says about one account.
struct UsageReading: Equatable, Codable {
    let windows: [UsageWindow]
    /// The service says the account is at its limit now — by its general
    /// windows. A per-model window, extra credits or an additional limit at
    /// 100% show red on their own bar; the account itself still works.
    let limitReached: Bool
}

// MARK: - the Jev's credit

/// What OpenRouter says of the credit the Jev spends, in dollars (`keep ia
/// jev`): what the account bought and spent, and the ceiling and spending of
/// the Jev's own key when the key has a ceiling.
struct JevCredit: Equatable, Codable {
    let total: Double
    let used: Double
    let keyLimit: Double?
    let keyUsed: Double?
    let usedToday: Double?
    let usedThisMonth: Double?

    var accountLeft: Double { max(0, total - used) }

    var keyLeft: Double? { keyLimit.map { max(0, $0 - (keyUsed ?? 0)) } }

    /// What the Jev can still spend: the smaller of the two.
    var available: Double { keyLeft.map { min($0, accountLeft) } ?? accountLeft }

    /// Spent, from 0 to 100 — the figure a bar draws.
    var accountPercent: Double { total > 0 ? min(100, used / total * 100) : 100 }

    var keyPercent: Double? {
        guard let keyLimit else { return nil }
        return keyLimit > 0 ? min(100, (keyUsed ?? 0) / keyLimit * 100) : 100
    }

    /// What the Jev's key has spent, as a share of all the credit the account
    /// bought — the bar of a key with no ceiling of its own, which spends
    /// from the whole credit.
    var keySharePercent: Double { total > 0 ? min(100, (keyUsed ?? 0) / total * 100) : 0 }
}

/// The Jev's line of the footer: the last credit that came back, kept through
/// later failures, and why the latest attempt brought nothing when it did not.
struct JevLine: Equatable, Codable {
    var credit: JevCredit?
    var measuredAt: Date?
    var problem: String?
}

/// What `keep ia jev` answers. `jev` is null when this Mac has no key of the
/// Jev's: there is then nothing to show.
struct JevAnswer: Decodable {
    let jev: JevLine?

    static func decode(_ data: Data) -> JevAnswer? {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .secondsSince1970
        return try? decoder.decode(JevAnswer.self, from: data)
    }
}

// MARK: - saying it

enum UsageText {
    /// "17%", whole numbers: a tenth of a percent of a weekly allowance is
    /// not something anyone acts on.
    static func percent(_ value: Double) -> String {
        "\(Int(max(0, min(999, value)).rounded()))%"
    }

    /// "US$ 9,98": a dollar figure the way the footer says it — two places,
    /// a comma, and "menos de" for what rounds to nothing.
    static func dollars(_ value: Double) -> String {
        let value = max(0, value)
        if value > 0, value < 0.01 { return "< US$ 0,01" }
        return "US$ " + String(format: "%.2f", value).replacingOccurrences(of: ".", with: ",")
    }

    /// How long until a window starts over, in the fewest characters that
    /// still say it: "41min", "4h10", "1d13h".
    static func until(_ date: Date?, now: Date) -> String? {
        guard let date else { return nil }
        let minutes = Int((date.timeIntervalSince(now) / 60).rounded(.up))
        if minutes <= 0 { return "agora" }
        if minutes < 60 { return "\(minutes)min" }
        let hours = minutes / 60
        if hours < 24 {
            let rest = minutes % 60
            return rest == 0 ? "\(hours)h" : "\(hours)h" + String(format: "%02d", rest)
        }
        let days = hours / 24
        let restHours = hours % 24
        return restHours == 0 ? "\(days)d" : "\(days)d\(restHours)h"
    }

    /// The reset as a moment, for the tooltip: "hoje às 01:50", "amanhã às
    /// 07:00", "em 28/09 às 07:00".
    static func moment(_ date: Date, now: Date, calendar: Calendar = .current) -> String {
        let clock = DateFormatter()
        clock.calendar = calendar
        clock.timeZone = calendar.timeZone
        clock.dateFormat = "HH:mm"
        let time = clock.string(from: date)
        if calendar.isDate(date, inSameDayAs: now) { return "hoje às \(time)" }
        if let tomorrow = calendar.date(byAdding: .day, value: 1, to: now),
           calendar.isDate(date, inSameDayAs: tomorrow) { return "amanhã às \(time)" }
        clock.dateFormat = "dd/MM"
        return "em \(clock.string(from: date)) às \(time)"
    }

    /// "agora", "há 3 min", "há 2h".
    static func ago(_ date: Date, now: Date) -> String {
        let minutes = Int(now.timeIntervalSince(date) / 60)
        if minutes < 1 { return "agora" }
        if minutes < 60 { return "há \(minutes) min" }
        return "há \(minutes / 60)h"
    }

    /// The same thresholds the Clínica's panel uses: a warning from 75, and
    /// critical from 90. The kit moves the tabs to an account with room from
    /// 95, and off an account at its limit — 100 — to the next one in the
    /// order that still answers.
    enum Level: Equatable { case normal, attention, critical }

    static func level(_ percent: Double) -> Level {
        if percent >= 90 { return .critical }
        if percent >= 75 { return .attention }
        return .normal
    }
}
