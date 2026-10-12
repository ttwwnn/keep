import Foundation

/// The folder a workspace's AI is held to.
///
/// Kept in `~/.config/keep/confinement.json` as `{"workspace": "/folder"}`.
/// It is the shell side that holds the AI to it: `claude()` and the account
/// switch start it through `~/.local/bin/keep-confine`, which runs it in a
/// macOS sandbox that reads and writes nowhere under /Users or /Volumes but
/// in that folder (and the little the AI needs to work). This is only where
/// the choice is kept.
enum WorkspaceConfinement {
    static var path: String {
        ProcessInfo.processInfo.environment["KEEP_CONFINE_CONFIG"]
            ?? (NSHomeDirectory() as NSString).appendingPathComponent(".config/keep/confinement.json")
    }

    /// Every workspace that has a folder.
    static func all() -> [String: String] {
        guard let data = FileManager.default.contents(atPath: path),
            let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return [:] }
        return json.compactMapValues { ($0 as? String).flatMap { $0.isEmpty ? nil : $0 } }
    }

    /// Choose a folder for a workspace, or nil to take it back. Written
    /// whole and renamed into place, so a reader never sees half of it.
    static func set(_ folder: String?, for workspace: String) {
        var next = all()
        if let folder, !folder.isEmpty { next[workspace] = folder } else { next.removeValue(forKey: workspace) }
        guard let data = try? JSONSerialization.data(
            withJSONObject: next, options: [.prettyPrinted, .sortedKeys])
        else { return }
        let url = URL(fileURLWithPath: path)
        try? FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? data.write(to: url, options: .atomic)
    }
}
