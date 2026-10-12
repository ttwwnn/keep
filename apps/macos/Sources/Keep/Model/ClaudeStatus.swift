import Foundation

/// What the bar under a Claude Code conversation shows: the model and the
/// effort it is running with, and how many agents it has going.
///
/// The model and the effort come from Claude Code's own status line, which
/// calls `~/.local/bin/keep-statusline` with them as JSON; the script saves
/// that under the number of the tab's shell (`<dir>/<pid>.json`). The agents
/// are counted off the screen, where Claude Code lists them under the prompt.
struct ClaudeStatus: Hashable {
    /// The id Claude Code gave (`claude-opus-5-5[1m]`).
    var modelID: String
    /// How it is called on screen (`Opus 5.5 (1M)`).
    var modelName: String
    /// `low`, `medium`, `high`, `xhigh` or `max`; nil when Claude Code did
    /// not say (a model without an effort setting).
    var effort: String?
    var agents: Int
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

/// Claude Code's question when it opens in a folder it has not been in:
/// `Quick safety check: is this a project you created or one you trust?`,
/// with `❯ 1. Yes, I trust this folder` and `2. No, exit`. Keep answers it
/// for the person, who typed `claude` there, with Enter on the first choice.
enum ClaudeTrustDialog {
    /// Whether the question is on the screen with its first choice — the
    /// yes — the one the pointer is on, so that Enter says yes.
    static func isAsking(onScreen text: String) -> Bool {
        guard text.contains("Yes, I trust this folder") else { return false }
        return text.split(separator: "\n").contains { line in
            let t = line.trimmingCharacters(in: .whitespaces)
            return t.hasPrefix("❯") && t.contains("Yes, I trust this folder")
        }
    }
}
