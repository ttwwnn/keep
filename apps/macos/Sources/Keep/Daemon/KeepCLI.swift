import Foundation

/// Running the `keep` that goes inside the app for what it answers in JSON —
/// `keep ia …` (the AI accounts, how much of each is spent, which AI each tab
/// runs on and putting it on another) and `keep worktrees …` (the worktrees a
/// tab's conversation made) — and hearing what it printed.
///
/// The core knows things this app should not have to: where every login is
/// kept, whose each one is, which process holds each tab and which account
/// it runs on, how to put a conversation on another account. It is the same
/// code in the Windows app, and it answers here in JSON on its standard
/// output. What is common to asking it is here: told which daemon the app is
/// using, since a test app runs against one of its own, and which home it
/// reads, since a test app reads a made-up one; read while it runs, so a long
/// answer cannot fill the pipe and stall the process writing it; and killed
/// when it outlives its time, since an answer held up by a hung process
/// would be a button that does nothing.
///
/// Blocking; call it off the main thread.
enum KeepCLI {
    /// What came back: how it exited, and everything it printed.
    struct Output {
        let status: Int32
        let data: Data
    }

    struct Failure: Error, CustomStringConvertible {
        let description: String
        /// How the process exited, when that is what went wrong.
        var status: Int32?
        init(_ description: String, status: Int32? = nil) {
            self.description = description
            self.status = status
        }
    }

    /// `name` is how the helper is called in what goes wrong: "o keep ia
    /// passou de 40 s".
    static func run(
        _ helper: String, _ arguments: [String], within seconds: TimeInterval, name: String
    ) -> Result<Output, Failure> {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: helper)
        process.arguments = arguments
        let environment = environment(ProcessInfo.processInfo.environment)
        prepare(environment)
        process.environment = environment
        let output = Pipe()
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice

        let finished = DispatchSemaphore(value: 0)
        process.terminationHandler = { _ in finished.signal() }
        do {
            try process.run()
        } catch {
            return .failure(Failure("\(name) não abriu: \(error.localizedDescription)"))
        }
        // Read while it runs, so a large answer cannot fill the pipe and
        // stall the process that is writing it.
        var data = Data()
        let reading = DispatchGroup()
        reading.enter()
        DispatchQueue.global(qos: .userInitiated).async {
            data = output.fileHandleForReading.readDataToEndOfFile()
            reading.leave()
        }
        guard finished.wait(timeout: .now() + seconds) == .success else {
            process.terminate()
            _ = finished.wait(timeout: .now() + 1)
            return .failure(Failure("o \(name) passou de \(Int(seconds)) s"))
        }
        _ = reading.wait(timeout: .now() + 2)
        return .success(Output(status: process.terminationStatus, data: data))
    }

    // MARK: - what it runs with

    /// What the core is told, on top of the app's own environment: the daemon
    /// the app uses (`KEEP_SOCKET`), and — for an app that reads a made-up
    /// home (`KEEP_AI_USAGE_HOME`, which every test build sets) — that home
    /// and a state folder inside it (`KEEP_IA_HOME`, `KEEP_IA_ESTADO`), so the
    /// core reads and writes there and nowhere else.
    ///
    /// The home alone is not enough. Where the home does not reach — the
    /// login Claude Code keeps in the Keychain, the services a token is shown
    /// to — a test app is kept off the real ones too, unless its test names
    /// stand-ins of its own: the Keychain is answered by a stand-in that has
    /// nothing in it (`KEEP_IA_SECURITY`, written by `prepare`), and the
    /// services' addresses point at a port on this machine where nothing
    /// listens. A test build must never show, measure or send a real login.
    static func environment(_ base: [String: String]) -> [String: String] {
        var environment = base
        environment["KEEP_SOCKET"] = Daemon.socketPath
        guard let home = madeUpHome(base) else { return environment }
        let state = (home as NSString).appendingPathComponent(".keep-ia-estado")
        environment["KEEP_IA_HOME"] = home
        environment["KEEP_IA_ESTADO"] = state
        if base["KEEP_IA_SECURITY"] == nil {
            environment["KEEP_IA_SECURITY"] = (state as NSString).appendingPathComponent(noKeychainName)
        }
        // The core's own names for the stand-ins, and the older ones it still
        // takes: a test that named either keeps it.
        let services: [(String, [String])] = [
            ("KEEP_IA_PERFIL_URL", ["KEEP_IA_PERFIL_URL", "CLAUDE_KIT_PERFIL_URL"]),
            ("KEEP_IA_CLAUDE_URL", ["KEEP_IA_CLAUDE_URL", "KEEP_AI_USAGE_CLAUDE_URL"]),
            ("KEEP_IA_CODEX_URL", ["KEEP_IA_CODEX_URL", "KEEP_AI_USAGE_CODEX_URL"]),
            ("KEEP_IA_OPENROUTER_URL", ["KEEP_IA_OPENROUTER_URL"]),
        ]
        for (name, names) in services where !names.contains(where: { base[$0] != nil }) {
            environment[name] = nowhere
        }
        return environment
    }

    /// The home a test app reads instead of the person's, if it reads one.
    static func madeUpHome(_ environment: [String: String]) -> String? {
        environment["KEEP_AI_USAGE_HOME"].flatMap { $0.isEmpty ? nil : $0 }
    }

    /// A port on this machine where nothing listens: a request made there
    /// fails at once, and no token leaves the machine.
    static let nowhere = "http://127.0.0.1:9"

    /// The Keychain of a test app: `security` as the core calls it, answering
    /// that there is no such item (44, `errSecItemNotFound`) to every question.
    static let noKeychainName = "sem-chaveiro"
    static let noKeychain = "#!/bin/sh\n# Keychain de um app de teste: nenhum item.\nexit 44\n"

    /// The stand-in Keychain in place before the core looks for it. Written
    /// into the made-up home's state folder, never anywhere else; a home that
    /// cannot be written to leaves the core unable to read any login, which
    /// is still not the real one.
    static func prepare(_ environment: [String: String]) {
        guard let state = environment["KEEP_IA_ESTADO"],
              let security = environment["KEEP_IA_SECURITY"],
              security == (state as NSString).appendingPathComponent(noKeychainName),
              !FileManager.default.isExecutableFile(atPath: security)
        else { return }
        try? FileManager.default.createDirectory(atPath: state, withIntermediateDirectories: true)
        guard FileManager.default.createFile(atPath: security, contents: Data(noKeychain.utf8)) else { return }
        chmod(security, 0o755)
    }

    /// An executable file. A directory passes `isExecutableFile` — it can be
    /// searched — and `/var/empty` is what the tests point the helpers at to
    /// say there is none.
    static func isRunnable(_ path: String) -> Bool {
        var directory: ObjCBool = false
        return !path.isEmpty
            && FileManager.default.fileExists(atPath: path, isDirectory: &directory)
            && !directory.boolValue
            && FileManager.default.isExecutableFile(atPath: path)
    }

    /// The `keep` inside the app: `Contents/Resources/keep`, the one every
    /// tab runs.
    static var bundledPath: String {
        Bundle.main.bundleURL.appendingPathComponent("Contents/Resources/keep").path
    }

    /// Where the `keep` to ask is, or nil when there is none: `named`, when
    /// the environment names one — then the only place looked at, so a test
    /// that points it at nothing gets nothing rather than the app's own —
    /// and otherwise the app's. Whatever is named is called exactly as the
    /// app's `keep` would be: the command's group first (`ia …`,
    /// `worktrees …`).
    static func path(named: String?, bundled: String? = nil) -> String? {
        if let named { return isRunnable(named) ? named : nil }
        let bundled = bundled ?? bundledPath
        return isRunnable(bundled) ? bundled : nil
    }
}

/// `keep ia`: the order of priority among the AI accounts, what each has
/// spent, the logins, and which account each tab runs on.
///
/// Every decision about an account or a credential is the core's — it reads
/// the logins, the order and the tabs' processes. The app shows what the core
/// says and asks it for changes, in its words: always `--json`, one object
/// printed, `"versao": 1`; exit 0 for `ok: true`, 1 for `ok: false` (the
/// object printed all the same, with a `motivo` and a `detalhe` in
/// Portuguese to show), 2 for a question asked wrong. A value travels inside
/// its option (`--ws=<name>`), so a workspace whose name starts with a dash
/// is still a workspace and not an option.
///
/// No `keep` to ask, no feature: the footer's arrows and "+" and the tabs'
/// chevrons are only there when there is one.
enum AIHelper {
    /// The key for Claude following the order of priority: whichever account
    /// of it can take work first.
    static let followOrder = "claude:ordem"

    /// The deadlines below are the contract's; a test that waits them out
    /// takes them down to a fraction.
    nonisolated(unsafe) static var timeScale: Double = 1

    /// Where the `keep` to ask is, or nil when there is none to ask.
    ///
    /// `KEEP_IA_BIN`, when it is set, is the only place looked at (a test
    /// that points it at nothing gets no helper, rather than the app's own),
    /// and it is called as the app's `keep` is: `ia` first. Otherwise the
    /// `keep` inside the app.
    static var path: String? { path(environment: ProcessInfo.processInfo.environment) }

    static func path(environment: [String: String], bundled: String? = nil) -> String? {
        KeepCLI.path(named: environment["KEEP_IA_BIN"], bundled: bundled)
    }

    /// A key the core takes: `claude:<name>`, `gpt:<name>`, or
    /// `claude:ordem`. Refused here before anything runs, so nothing that
    /// arrives from a file can become an option or a path on the way out.
    static func isValidKey(_ key: String) -> Bool {
        key.range(of: #"^(claude|gpt):[^\s/:]+$"#, options: .regularExpression) != nil
    }

    /// The two services a login can be opened for.
    enum Service: String {
        case claude
        case gpt
    }

    /// Why the core did not do it: its `motivo` (`ocupada`,
    /// `outro-programa`, …) when it gave one, and what to tell the person.
    ///
    /// `precisa-login` is a step rather than a failure: the account has no
    /// login of its own for a tab yet, and the core has opened one in a tab —
    /// `aba_login`, in `ws_login` — that puts the tab on the account itself
    /// once it is through. The conversation has not been touched.
    struct Problem: Error, Equatable {
        let reason: String?
        let detail: String
        var login: (workspace: String?, tab: UInt32)? = nil

        static func == (a: Problem, b: Problem) -> Bool {
            a.reason == b.reason && a.detail == b.detail
                && a.login?.workspace == b.login?.workspace && a.login?.tab == b.login?.tab
        }
    }

    // MARK: - asking

    /// One place up or down the order. The answer is the order as the core
    /// now has it.
    static func moveOrder(_ key: String, up: Bool) -> Result<[String], Problem> {
        guard isValidKey(key), key != followOrder else { return .failure(invalid(key)) }
        return ask(["ordem", "mover", key, up ? "cima" : "baixo", "--json"], within: 10 * timeScale)
            .map { ($0["ordem"] as? [String]) ?? [] }
    }

    /// A new tab in `workspace` where a login to another account is made;
    /// the answer is where the core opened it.
    static func signIn(
        _ service: Service, workspace: String
    ) -> Result<(workspace: String, tab: UInt32), Problem> {
        ask(["entrar", service.rawValue, "--ws=\(workspace)", "--json"], within: 20 * timeScale)
            .flatMap { answer in
                guard let opened = answer["ws"] as? String,
                      let tab = (answer["aba"] as? NSNumber)?.uint32Value
                else {
                    return .failure(Problem(reason: "erro", detail: "O keep ia não disse qual aba abriu."))
                }
                return .success((opened, tab))
            }
    }

    /// Put a tab on another AI or account. `interrupt` is the person's yes to
    /// stopping what the tab is doing, asked for after the core said it was
    /// busy.
    static func switchAccount(
        workspace: String, tab: UInt32, to key: String, interrupt: Bool
    ) -> Result<String, Problem> {
        guard isValidKey(key) else { return .failure(invalid(key)) }
        var arguments = ["trocar", "--ws=\(workspace)", "--aba=\(tab)", "--para=\(key)"]
        if interrupt { arguments.append("--interromper") }
        arguments.append("--json")
        return ask(arguments, within: 40 * timeScale).map { ($0["feito"] as? String) ?? "" }
    }

    /// Which readings a question about usage is after.
    enum Measure: Equatable {
        /// The accounts whose reading is due — five minutes between two,
        /// sooner after a failure — and what the others last said.
        case due
        /// "Medir agora": every account, within the core's own limits (one
        /// such round in 15 s, never through a service's "too many requests").
        case now
        /// What the core last measured, without asking any service: which
        /// accounts there are, a moment after a login changes.
        case cached
        /// This account now — a login just made — and the others as they are.
        case only(String)

        var arguments: [String] {
            switch self {
            case .due: return []
            case .now: return ["--agora"]
            case .cached: return ["--cache"]
            case .only(let key): return ["--conta=\(key)"]
            }
        }

        /// How the trace names it.
        var label: String {
            switch self {
            case .due: return "devidas"
            case .now: return "agora"
            case .cached: return "cache"
            case .only(let key): return "conta \(key)"
            }
        }

        /// A round asks the services, each within 15 s, and a login whose
        /// owner is not known yet asks once more; the cache asks nobody.
        var deadline: TimeInterval {
            switch self {
            case .cached: return 15
            case .due, .now, .only: return 60
            }
        }
    }

    /// Every account, in the order of priority, and what each last said
    /// about its allowance (`keep ia uso`).
    static func usage(_ measure: Measure = .due) -> Result<UsageAnswer, Problem> {
        if case .only(let key) = measure, !isValidKey(key) || key == followOrder {
            return .failure(invalid(key))
        }
        return askRaw(["uso"] + measure.arguments + ["--json"], within: measure.deadline * timeScale)
            .flatMap { data in
                guard let answer = UsageAnswer.decode(data) else {
                    return .failure(Problem(reason: "erro", detail: "O keep ia uso respondeu algo que não dá para ler."))
                }
                return .success(answer)
            }
    }

    /// What the Jev can still spend of OpenRouter's credit (`keep ia jev`).
    /// The core keeps the cadence, as it does for the accounts; `jev` in the
    /// answer is nil when this Mac has no key of the Jev's.
    static func jev(_ measure: Measure = .due) -> Result<JevAnswer, Problem> {
        let arguments: [String]
        switch measure {
        case .now: arguments = ["--agora"]
        case .cached: arguments = ["--cache"]
        case .due, .only: arguments = []
        }
        return askRaw(["jev"] + arguments + ["--json"], within: 30 * timeScale)
            .flatMap { data in
                guard let answer = JevAnswer.decode(data) else {
                    return .failure(Problem(reason: "erro", detail: "O keep ia jev respondeu algo que não dá para ler."))
                }
                return .success(answer)
            }
    }

    /// Which AI and which account each tab runs on (`keep ia abas`), as
    /// printed: `AITabAccounts` reads it, and checks it is about the daemon
    /// the app is using.
    static func tabs() -> Result<Data, Problem> {
        askRaw(["abas", "--json"], within: 15 * timeScale)
    }

    /// One round of the order being followed (`keep ia sincronizar`): the
    /// tabs on an account that cannot take work moved to the first one that
    /// can, when they are free. The core does nothing while another manager
    /// of the accounts is installed, and says so.
    ///
    /// A round can switch three tabs, each the seconds a switch takes; it is
    /// given time for that and more, since a switch cut off half way leaves
    /// a tab at its shell.
    static func sync() -> Result<[String: Any], Problem> {
        ask(["sincronizar", "--json"], within: 300 * timeScale)
    }

    private static func invalid(_ key: String) -> Problem {
        Problem(reason: "uso", detail: "“\(key)” não é uma conta que o keep ia conheça.")
    }

    /// One question, and its answer when the answer is yes.
    private static func ask(
        _ arguments: [String], within seconds: TimeInterval
    ) -> Result<[String: Any], Problem> {
        askRaw(arguments, within: seconds).flatMap { data in
            guard let answer = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                return .failure(Problem(reason: "erro", detail: "O keep ia respondeu algo que não dá para ler."))
            }
            return .success(answer)
        }
    }

    /// One question, and what was printed when the answer is yes — checked
    /// to be an answer of the contract, but not otherwise read.
    private static func askRaw(
        _ arguments: [String], within seconds: TimeInterval
    ) -> Result<Data, Problem> {
        guard let helper = path else {
            return .failure(Problem(reason: "sem-ajudante", detail: "Não há um keep para perguntar."))
        }
        Trace.log("ia", "keep ia \(arguments.joined(separator: " "))")
        switch KeepCLI.run(helper, ["ia"] + arguments, within: seconds, name: "keep ia") {
        case .failure(let why):
            return .failure(Problem(reason: "erro", detail: why.description))
        case .success(let output):
            guard let answer = try? JSONSerialization.jsonObject(with: output.data) as? [String: Any],
                  (answer["versao"] as? NSNumber)?.intValue == 1
            else {
                return .failure(Problem(
                    reason: "erro",
                    detail: output.status == 2
                        ? "O keep ia não entendeu o pedido."
                        : "O keep ia respondeu algo que não dá para ler (saiu com \(output.status))."))
            }
            if output.status == 0, answer["ok"] as? Bool == true { return .success(output.data) }
            let detail = (answer["detalhe"] as? String).flatMap { $0.isEmpty ? nil : $0 }
                ?? "O keep ia saiu com \(output.status)."
            let login = (answer["aba_login"] as? NSNumber).map {
                (workspace: answer["ws_login"] as? String, tab: $0.uint32Value)
            }
            return .failure(Problem(reason: answer["motivo"] as? String, detail: detail, login: login))
        }
    }
}
