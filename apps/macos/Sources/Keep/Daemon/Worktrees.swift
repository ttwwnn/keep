import Foundation

/// The git worktrees a tab's conversation made, which go to the Trash when
/// the tab is closed.
///
/// A Claude Code session spins up worktrees for its work and leaves them
/// behind: closing its tab ended the work, and the folders — each a full
/// checkout, vendor and all — stayed until somebody went through `~/projetos`
/// by hand deciding which were whose. The person's rule for that sweep is the
/// rule here: a worktree belongs to the tab whose conversation created it,
/// an open tab is in use, and when in doubt it is not the tab's.
///
/// Which worktrees those are is not something this app can see. The process
/// in the tab sits in the directory it was started in while its work happens
/// elsewhere; what connects a tab to its conversation, and a conversation to
/// the worktrees it made, is the conversation's own record and the moment
/// each worktree was born. The core works that out — `keep worktrees`, the
/// `keep` inside the app, the same code the Windows app runs — and answers
/// in JSON; it writes the births down as they happen (`indexar`, every two
/// minutes, `AIChores`). Without a `keep` to ask nothing here happens: the
/// question before closing stays as it was.
///
/// The app keeps the two steps that must happen inside a long-lived process
/// or with the person watching: saying it in the question before anything
/// ends, and the move itself, `FileManager.trashItem` — Finder's "Put Back"
/// is written down asynchronously by the process that trashes, and a helper
/// that exited straight after would lose it.
///
/// Blocking throughout; call it off the main thread.
enum Worktrees {
    /// What to ask about: a daemon workspace and the daemon tabs in it that
    /// are being closed — a tab's root and its panes, or a lone pane.
    struct Target: Equatable {
        let workspace: String
        let tabs: [UInt32]

        var argument: String {
            "\(workspace):" + tabs.map(String.init).joined(separator: ",")
        }
    }

    /// The core's answer to `listar`.
    struct Listing: Decodable {
        struct Tab: Decodable {
            let alvo: String
            let pids: [Int32]?
        }

        struct Running: Decodable {
            let pid: Int32
            let nome: String?
        }

        struct Item: Decodable {
            let caminho: String
            let ramo: String?
            let head: String?
            let alteracoes: Int?
            let commits_so_aqui: Int?
            let processos: [Running]?
            /// When the worktree was born, so `preparar` can refuse a
            /// different one made at the same path while the question was up.
            let nasceu_ns: Int64?
        }

        struct Kept: Decodable {
            let caminho: String
            let motivo: String
        }

        let versao: Int
        let abas: [Tab]?
        let lixeira: [Item]?
        let mantidas: [Kept]?
        let avisos: [String]?

        /// The processes that end with the tabs, to be waited for before
        /// anything moves: a folder still being written to follows nobody to
        /// the Trash.
        var pids: [Int32] { (abas ?? []).flatMap { $0.pids ?? [] } }
    }

    enum Answer {
        /// No `keep` to ask: nothing to say, nothing to move.
        case off
        case listing(Listing)
        /// The core is there and did not answer. Said in the question, and
        /// nothing moves: a guess is not consent.
        case failed(String)
    }

    /// The `keep` inside the app, or `KEEP_WORKTREES_BIN` when it is set —
    /// then the only place looked at, so a test that points it at nothing
    /// gets nothing. Whatever answers is called as the app's `keep` is:
    /// `worktrees` first.
    static var helper: String? {
        KeepCLI.path(named: ProcessInfo.processInfo.environment["KEEP_WORKTREES_BIN"])
    }

    // MARK: - asking

    /// The worktrees that go with these tabs.
    ///
    /// Asked before the question, because after the tab closes the
    /// conversation in it is gone and so is the only live link between the
    /// tab and what it made. `deadline` is the helper's; the app allows it a
    /// moment more to be heard.
    static func list(_ targets: [Target], deadline: TimeInterval = 3) -> Answer {
        guard let helper else { return .off }
        guard !targets.isEmpty else { return .listing(Listing(versao: 1, abas: [], lixeira: [], mantidas: [], avisos: [])) }
        let arguments = ["listar", "--prazo", String(format: "%.1f", deadline)] + targets.map(\.argument)
        switch run(helper, arguments, within: deadline + 1.5) {
        case .failure(let why):
            return .failed(why.description)
        case .success(let data):
            // A refusal in the contract's words is a failure to list, said
            // as the core says it — not a list with nothing in it.
            let answer = object(data)
            if answer["ok"] as? Bool == false {
                return .failed(answer["motivo"] as? String ?? "o keep worktrees recusou")
            }
            guard let listing = try? JSONDecoder().decode(Listing.self, from: data), listing.versao == 1
            else { return .failed("resposta ilegível do keep worktrees") }
            return .listing(listing)
        }
    }

    /// Write down the births of the worktrees the running conversations are
    /// making (`indexar`) — what `listar` goes by when their tab closes,
    /// since by then the conversation that made one may be long past it.
    /// Why it did not, or nil when it did.
    static func index(within seconds: TimeInterval = 120) -> String? {
        guard let helper else { return nil }
        switch run(helper, ["indexar", "--json"], within: seconds) {
        case .failure(let why): return why.description
        case .success: return nil
        }
    }

    /// What the question adds: which folders go, which stay and why, or that
    /// nothing could be checked. Empty when there is nothing to say.
    ///
    /// `subject` is what is being closed, as the sentence says it: "desta
    /// aba", "deste painel", "deste workspace".
    static func note(for answer: Answer, about subject: String = "desta aba") -> String {
        switch answer {
        case .off:
            return ""
        case .failed(let why):
            return "\n\nNão consegui verificar as worktrees \(subject) (\(why)); nenhuma será movida."
        case .listing(let listing):
            var parts: [String] = []
            let going = listing.lixeira ?? []
            if !going.isEmpty {
                var text = going.count == 1
                    ? "A worktree \(subject) vai para a lixeira:"
                    : "As \(going.count) worktrees \(subject) vão para a lixeira:"
                for item in going { text += "\n• " + describe(item) }
                // Put Back brings the files, not the worktree: its entry in
                // the repository is dropped so the branch is free again.
                text += "\nOs arquivos voltam pelo “Colocar de volta” da Lixeira, já fora do git; "
                    + "o ramo continua no repositório e os commits e alterações ficam guardados em refs/keep-lixeira."
                parts.append(text)
            }
            let kept = listing.mantidas ?? []
            if !kept.isEmpty {
                var text = kept.count == 1 ? "Fica onde está:" : "Ficam onde estão:"
                for item in kept { text += "\n• \(tilde(item.caminho)) — \(item.motivo)" }
                parts.append(text)
            }
            parts += listing.avisos ?? []
            return parts.isEmpty ? "" : "\n\n" + parts.joined(separator: "\n\n")
        }
    }

    /// One folder's line: where, and what in it is not anywhere else.
    static func describe(_ item: Listing.Item) -> String {
        var facts: [String] = []
        if let branch = item.ramo, !branch.isEmpty {
            facts.append("ramo \(branch)")
        } else if let head = item.head, !head.isEmpty {
            facts.append("sem ramo, em \(head)")
        }
        if let changed = item.alteracoes, changed > 0 {
            facts.append(changed == 1 ? "1 arquivo alterado" : "\(changed) arquivos alterados")
        }
        if let only = item.commits_so_aqui, only > 0 {
            facts.append(only == 1 ? "1 commit só nela" : "\(only) commits só nela")
        }
        if let running = item.processos, !running.isEmpty {
            let names = running.map { $0.nome ?? "pid \($0.pid)" }
            facts.append("ainda rodando dentro: " + names.joined(separator: ", "))
        }
        return tilde(item.caminho) + (facts.isEmpty ? "" : " (" + facts.joined(separator: "; ") + ")")
    }

    static func tilde(_ path: String) -> String {
        let home = NSHomeDirectory()
        guard path == home || path.hasPrefix(home + "/") else { return path }
        return "~" + path.dropFirst(home.count)
    }

    // MARK: - moving

    /// Move the listed folders to the Trash once the tabs' processes are gone.
    ///
    /// Only what the question showed: consent was given for that list, and a
    /// worktree born since is not in it. Each folder is prepared by the
    /// helper first — its HEAD, and any change not yet committed, saved in
    /// `refs/keep-lixeira` so no commit depends on the folder surviving the
    /// Trash — then moved here, then its registration in the repository
    /// dropped by the helper, so its branch and its name are free again. A
    /// folder something still runs in is retried for a minute, the time a
    /// test runner the conversation left behind takes to notice; after that
    /// it stays, and says so.
    ///
    /// Returns what did not go, one line each; empty when everything did.
    static func trash(_ listing: Listing) -> [String] {
        guard let helper else { return [] }
        let going = listing.lixeira ?? []
        guard !going.isEmpty else { return [] }
        // The tabs' own processes gone, or nothing moves: a conversation
        // still running is still using its worktrees, whatever the close
        // reported. Twenty seconds covers Claude Code's own grace on a hang-up.
        let running = waitForExit(listing.pids, upTo: 20)
        guard running.isEmpty else {
            let pids = running.map(String.init).joined(separator: ", ")
            return going.map {
                "\(tilde($0.caminho)): a conversa da aba ainda está rodando (pid \(pids)); nada foi movido"
            }
        }

        var problems: [String] = []
        for item in going {
            let place = tilde(item.caminho)
            var ready = false
            var reason = "sem resposta do keep worktrees"
            let giveUp = Date().addingTimeInterval(60)
            let identity = item.nasceu_ns.map { ["--nasceu", String($0)] } ?? []
            while true {
                switch run(helper, ["preparar", item.caminho] + identity, within: 30) {
                case .failure(let why):
                    reason = why.description
                case .success(let data):
                    let answer = object(data)
                    if answer["ok"] as? Bool == true {
                        ready = true
                    } else {
                        reason = answer["motivo"] as? String ?? "recusada pelo keep worktrees"
                        // Only something still running is worth waiting out.
                        let running = (answer["processos"] as? [Any]) ?? []
                        if !running.isEmpty, Date() < giveUp {
                            Thread.sleep(forTimeInterval: 3)
                            continue
                        }
                    }
                }
                break
            }
            guard ready else {
                problems.append("\(place): \(reason)")
                continue
            }

            var moved: NSURL?
            do {
                try FileManager.default.trashItem(
                    at: URL(fileURLWithPath: item.caminho), resultingItemURL: &moved)
            } catch {
                problems.append("\(place): \(error.localizedDescription)")
                continue
            }
            let destination = (moved as URL?)?.path ?? ""
            Trace.log("worktree", "trashed \(place)")
            switch run(helper, ["concluir", item.caminho, destination], within: 30) {
            case .failure(let why):
                problems.append("\(place) foi para a lixeira, mas o git ainda a registra: \(why)")
            case .success(let data):
                let answer = object(data)
                if answer["ok"] as? Bool != true {
                    let why = answer["motivo"] as? String ?? "recusado"
                    problems.append("\(place) foi para a lixeira, mas o git ainda a registra: \(why)")
                }
            }
        }
        return problems
    }

    /// Until every one of these is gone, or the time is up; the ones still
    /// running then.
    ///
    /// A zombie counts as gone: it runs nothing, and a shell killed under
    /// its parent can stay one until the parent reaps it — which `kill(pid,
    /// 0)` alone would read as alive for ever.
    static func waitForExit(_ pids: [Int32], upTo seconds: TimeInterval) -> [Int32] {
        let until = Date().addingTimeInterval(seconds)
        func alive(_ pid: Int32) -> Bool {
            guard pid > 0, kill(pid, 0) == 0 || errno == EPERM else { return false }
            var info = kinfo_proc()
            var size = MemoryLayout<kinfo_proc>.stride
            var name: [Int32] = [CTL_KERN, KERN_PROC, KERN_PROC_PID, pid]
            guard sysctl(&name, 4, &info, &size, nil, 0) == 0, size > 0 else { return true }
            return Int32(info.kp_proc.p_stat) != SZOMB
        }
        while pids.contains(where: alive), Date() < until {
            Thread.sleep(forTimeInterval: 0.1)
        }
        return pids.filter(alive)
    }

    // MARK: - running the helper

    private static func object(_ data: Data) -> [String: Any] {
        (try? JSONSerialization.jsonObject(with: data) as? [String: Any]) ?? [:]
    }

    /// Run `keep worktrees` and hand back what it printed, or why there is
    /// nothing (`KeepCLI`). A clean exit is an answer, and so is a refusal
    /// that says so in the contract's words (exit 1 with a `versao` 1
    /// object): `preparar` says no that way, with the reason and what still
    /// runs in the folder. Anything else is a failure.
    static func run(_ helper: String, _ arguments: [String], within seconds: TimeInterval)
        -> Result<Data, RunFailure>
    {
        KeepCLI.run(helper, ["worktrees"] + arguments, within: seconds, name: "keep worktrees").flatMap {
            if $0.status == 0 { return .success($0.data) }
            if $0.status == 1, (object($0.data)["versao"] as? NSNumber)?.intValue == 1 {
                return .success($0.data)
            }
            return .failure(RunFailure("o keep worktrees saiu com \($0.status)"))
        }
    }

    typealias RunFailure = KeepCLI.Failure
}

