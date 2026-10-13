import Foundation

/// What the bar under an AI conversation shows: the model and the effort it
/// is running with, and how many agents it has going.
///
/// For Claude Code, the model and the effort come from its own status line,
/// which calls `~/.local/bin/keep-statusline` with them as JSON; the script
/// saves that under the number of the tab's shell (`<dir>/<pid>.json`). The
/// agents are counted off the screen, where Claude Code lists them under the
/// prompt. For Codex, all of it is read off the screen (`CodexStatusReader`).
struct ClaudeStatus: Hashable {
    /// `claude` or `codex`: which program the bar speaks to.
    var program: String = "claude"
    /// The id the program goes by (`claude-opus-5-5[1m]`, `gpt-6-astra`).
    var modelID: String
    /// How it is called on screen (`Opus 5.5 (1M)`, `GPT-6-Astra`).
    var modelName: String
    /// `low`, `medium`, `high`, `xhigh`, `max` (and `ultra` in Codex); nil
    /// when the program did not say (a model without an effort setting).
    var effort: String?
    /// Nil where the program does not list its agents (Codex): the bar then
    /// says nothing about them.
    var agents: Int?
    /// Whether a command can be typed into the prompt now: the turn is over
    /// and nothing — a draft, a dialog — is in the way.
    var canType: Bool
}

enum ClaudeStatusReader {
    /// Where the status line command leaves its files.
    static var directory: String {
        ProcessInfo.processInfo.environment["KEEP_STATUS_DIR"]
            ?? (NSHomeDirectory() as NSString).appendingPathComponent(".local/state/keep/claude")
    }

    /// The model and the effort the tab whose shell is `shellPid` last
    /// reported, or nil when it has not (Claude Code not started yet, an old
    /// version, or the status line not set up).
    static func read(shellPid: UInt32) -> (modelID: String, modelName: String, effort: String?)? {
        guard shellPid > 0 else { return nil }
        let path = (directory as NSString).appendingPathComponent("\(shellPid).json")
        guard let data = FileManager.default.contents(atPath: path) else { return nil }
        return parse(data)
    }

    static func parse(_ data: Data) -> (modelID: String, modelName: String, effort: String?)? {
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let model = json["model"] as? [String: Any]
        else { return nil }
        let id = (model["id"] as? String) ?? ""
        let display = (model["display_name"] as? String) ?? id
        guard !id.isEmpty || !display.isEmpty else { return nil }
        let effort = (json["effort"] as? [String: Any])?["level"] as? String
        return (id, shortName(display), effort)
    }

    /// `Opus 5.5 (1M context)` as `Opus 5.5 (1M)`.
    static func shortName(_ display: String) -> String {
        display.replacingOccurrences(of: " context)", with: ")")
    }

    /// How many agents Claude Code has running, as it lists them under the
    /// prompt (`◯ general-purpose …` for each) or counts them when the list
    /// is folded (`· 2 local agents`).
    static func agents(onScreen text: String) -> Int {
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
        guard let rule = lines.lastIndex(where: isRule) else { return 0 }
        let footer = lines[(rule + 1)...].map { $0.trimmingCharacters(in: .whitespaces) }
        let listed = footer.filter { $0.hasPrefix("◯") }.count
        var counted = 0
        for line in footer {
            guard let match = line.range(of: #"(\d+) local agents?"#, options: .regularExpression)
            else { continue }
            counted = max(counted, Int(line[match].prefix(while: \.isNumber)) ?? 0)
        }
        return max(listed, counted)
    }

    /// Whether the prompt box holds nothing: `❯` and no text after it.
    static func promptIsEmpty(onScreen text: String) -> Bool {
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
        guard let rule = lines.lastIndex(where: isRule),
            let opening = lines[..<rule].lastIndex(where: isRule),
            let first = lines[(opening + 1)..<rule].first(where: {
                !$0.trimmingCharacters(in: .whitespaces).isEmpty
            })
        else { return false }
        let line = first.trimmingCharacters(in: .whitespaces)
        return line == "❯" || line == ">"
    }

    private static func isRule(_ line: Substring) -> Bool {
        line.trimmingCharacters(in: .whitespaces).hasPrefix(String(repeating: "─", count: 12))
    }

    /// The efforts `/effort` takes, lowest first.
    static let efforts = ["low", "medium", "high", "xhigh", "max"]

    /// The models offered for `/model`: what to show, what to type.
    static let models: [(name: String, id: String)] = [
        ("Fable 5.1", "claude-fable-5-1"),
        ("Opus 5.5", "claude-opus-5-5"),
        ("Opus 5.5 (1M)", "claude-opus-5-5[1m]"),
        ("Sonnet 5.5", "claude-sonnet-5-5"),
        ("Haiku 5.5", "claude-haiku-5-5"),
    ]
}

/// One of the models Codex offers, as its cache lists them
/// (`~/.codex/models_cache.json`): the slug it goes by, the name on screen,
/// the efforts it takes and the one it starts with.
struct CodexModel: Hashable {
    let slug: String
    let name: String
    let efforts: [String]
    let defaultEffort: String?
}

/// Codex's model and effort, read off its screen: the line under its prompt
/// (`GPT-6-Astra max · ~/Developer/keep`, Codex 0.162) or, with that out of
/// sight, the last `• Model changed to gpt-6-astra max` above it.
enum CodexStatusReader {
    struct Said: Hashable {
        var model: String
        var effort: String?
    }

    static let efforts = ["low", "medium", "high", "xhigh", "max", "ultra"]
    private static let effortPattern = "minimal|low|medium|high|xhigh|max|ultra|none"
    private static let modelPattern = #"[A-Za-z][\w.:/-]*"#

    static func read(onScreen text: String) -> Said? {
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
            .map { $0.trimmingCharacters(in: .whitespaces) }
        let prompt = lines.lastIndex(where: isPrompt)
        if let prompt {
            for line in lines[(prompt + 1)...] {
                if let said = match(line, #"^("# + modelPattern + #")(?:\s+("# + effortPattern
                    + #"))?(?:\s+fast)?\s+·\s"#) { return said }
            }
        }
        let above = prompt.map { Array(lines[..<$0]) } ?? lines
        for line in above.reversed() {
            if let said = match(line, #"^[•●◦∙*]?\s*Model changed to\s+("# + modelPattern
                + #")(?:\s+(?:with\s+)?("# + effortPattern + #")\b)?"#) { return said }
        }
        return nil
    }

    private static func match(_ line: String, _ pattern: String) -> Said? {
        guard let regex = try? NSRegularExpression(pattern: pattern, options: [.caseInsensitive]),
            let m = regex.firstMatch(in: line, range: NSRange(line.startIndex..., in: line)),
            let modelRange = Range(m.range(at: 1), in: line)
        else { return nil }
        let model = String(line[modelRange])
        // A model's name has a digit or a hyphen in it; a word does not.
        guard model.contains(where: { $0.isNumber || $0 == "-" }) else { return nil }
        let effort = Range(m.range(at: 2), in: line).map { String(line[$0]).lowercased() }
        return Said(model: model, effort: effort)
    }

    /// Codex's prompt: `›` (or `»`), and not a numbered choice of a picker.
    static func isPrompt(_ line: String) -> Bool {
        (line.hasPrefix("›") || line.hasPrefix("»"))
            && line.range(of: #"^[❯›»]\s*\d+[.)]\s+"#, options: .regularExpression) == nil
    }

    /// The hints Codex shows, dim, in an empty prompt — in the screen's text
    /// like anything typed. A hint not listed reads as a draft, which keeps
    /// the menu shut rather than typing into somebody's text.
    private static let hints: Set<String> = [
        "ask codex to do anything", "explain this codebase", "summarize recent commits",
        "implement {feature}", "find and fix a bug in @filename", "write tests for @filename",
        "improve documentation in @filename", "run /review on my current changes",
        "use /skills to list available skills",
    ]

    /// Whether Codex's prompt holds nothing: bare, or with one of its hints.
    static func promptIsEmpty(onScreen text: String) -> Bool {
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
            .map { $0.trimmingCharacters(in: .whitespaces) }
        guard let prompt = lines.lastIndex(where: isPrompt) else { return false }
        let typed = lines[prompt].dropFirst().trimmingCharacters(in: .whitespaces)
        return typed.isEmpty || hints.contains(typed.lowercased())
    }

    /// The models Codex offers, in its order, from its cache.
    static func models(cache: String? = nil) -> [CodexModel] {
        let path = cache ?? {
            let home = ProcessInfo.processInfo.environment["CODEX_HOME"]
                ?? (NSHomeDirectory() as NSString).appendingPathComponent(".codex")
            return (home as NSString).appendingPathComponent("models_cache.json")
        }()
        guard let data = FileManager.default.contents(atPath: path),
            let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let list = json["models"] as? [[String: Any]]
        else { return [] }
        return list.compactMap { m -> (Int, CodexModel)? in
            guard let slug = m["slug"] as? String, (m["visibility"] as? String) == "list" else { return nil }
            let efforts = (m["supported_reasoning_levels"] as? [Any] ?? []).compactMap { level -> String? in
                (level as? [String: Any])?["effort"] as? String ?? level as? String
            }
            let model = CodexModel(
                slug: slug, name: m["display_name"] as? String ?? slug, efforts: efforts,
                defaultEffort: m["default_reasoning_level"] as? String)
            return ((m["priority"] as? Int) ?? 1000, model)
        }.sorted { $0.0 < $1.0 }.map(\.1)
    }

    /// The model a name on screen stands for: its slug or its display name.
    static func model(named name: String, in models: [CodexModel]) -> CodexModel? {
        models.first { $0.slug.lowercased() == name.lowercased() || $0.name.lowercased() == name.lowercased() }
    }
}

/// Codex's own picker, which `/model` opens: a numbered list of models, then
/// one of reasoning levels, with Max and Ultra one step further under "More
/// reasoning…". A number key chooses at once. These read where a choice is.
enum CodexPicker {
    static let modelsTitle = "Select Model and Effort"
    static let levelsTitle = "Select Reasoning Level"
    static let advancedTitle = "Advanced Reasoning"
    static let moreReasoning = "More reasoning"

    /// How a level is called in the picker.
    static func label(ofEffort effort: String) -> String {
        switch effort {
        case "xhigh": return "Extra high"
        default: return effort.prefix(1).uppercased() + effort.dropFirst()
        }
    }

    /// The number of the choice called `label` (`2. GPT-6-Astra (current)`
    /// is GPT-6-Astra's 2), or nil when no choice on the screen is called
    /// that. The name must end where the label does: `High` is not
    /// `Extra high`, and `GPT-6-Sol` is not `GPT-6.1-Sol`.
    static func number(of label: String, onScreen text: String) -> Int? {
        let wanted = label.lowercased()
        for raw in text.split(separator: "\n") {
            var line = raw.trimmingCharacters(in: .whitespaces)
            if let first = line.first, "›>❯»".contains(first) {
                line = String(line.dropFirst()).trimmingCharacters(in: .whitespaces)
            }
            guard let dot = line.firstIndex(of: "."), let n = Int(line[..<dot]) else { continue }
            let name = line[line.index(after: dot)...].trimmingCharacters(in: .whitespaces).lowercased()
            guard name.hasPrefix(wanted) else { continue }
            let rest = name.dropFirst(wanted.count)
            if rest.isEmpty || rest.first == " " || rest.first == "(" || rest.first == "…" { return n }
        }
        return nil
    }
}

/// Claude Code's question when it opens in a folder it has not been in:
/// `Quick safety check: Is this a project you created or one you trust?`,
/// with the choices `No, exit` — the one the pointer starts on — and `Yes, I
/// trust this folder`. Keep answers it for the person, who typed `claude`
/// there: the pointer down to the yes, then Enter.
enum ClaudeTrustDialog {
    enum Pointer { case onYes, onNo }

    /// Whether the question is on the screen, and which choice the pointer
    /// (`❯`) is on. Nil when it is not the question — the words quoted in a
    /// conversation, say, have no pointer in front of them.
    static func pointer(onScreen text: String) -> Pointer? {
        guard text.contains("Yes, I trust this folder"), text.contains("No, exit") else { return nil }
        for line in text.split(separator: "\n") {
            let t = line.trimmingCharacters(in: .whitespaces)
            guard t.hasPrefix("❯") else { continue }
            if t.contains("Yes, I trust this folder") { return .onYes }
            if t.contains("No, exit") { return .onNo }
        }
        return nil
    }
}
