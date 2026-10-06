import AppKit

/// What happens to the text under a ⌘-click: a web address or mail link goes
/// to the browser or the mail app, a file path opens in whatever the Mac
/// opens that kind of file with, and a folder opens in the Finder.
///
/// Paths arrive as they were printed, so they may be `~/x`, relative to the
/// shell's directory, or carry the `:line` or `:line:col` that tools append.
enum LinkOpener {
    /// The file or folder a printed path stands for, or nil when nothing
    /// there exists. The path is tried whole first (a file may really be
    /// called `a:1`), then without a trailing `:line[:col]`.
    static func resolve(_ text: String, from directory: String?) -> URL? {
        var candidates = [text]
        var trimmed = text
        for _ in 0..<2 {
            guard let colon = trimmed.lastIndex(of: ":"),
                !trimmed[trimmed.index(after: colon)...].isEmpty,
                trimmed[trimmed.index(after: colon)...].allSatisfy(\.isNumber)
            else { break }
            trimmed = String(trimmed[..<colon])
            candidates.append(trimmed)
        }
        for candidate in candidates {
            var path = (candidate as NSString).expandingTildeInPath
            if !path.hasPrefix("/") {
                guard let directory else { continue }
                path = (directory as NSString).appendingPathComponent(path)
            }
            path = (path as NSString).standardizingPath
            if FileManager.default.fileExists(atPath: path) {
                return URL(fileURLWithPath: path)
            }
        }
        return nil
    }

    /// A test build is not to open anything on the machine it runs on: with
    /// this set, what would have been opened is appended to that file instead.
    private static let logPath = ProcessInfo.processInfo.environment["KEEP_LINK_LOG"]

    private static func hand(_ url: URL) -> Bool {
        guard let logPath else { return NSWorkspace.shared.open(url) }
        let line = (url.isFileURL ? url.path : url.absoluteString) + "\n"
        if let handle = FileHandle(forWritingAtPath: logPath) {
            handle.seekToEndOfFile()
            handle.write(Data(line.utf8))
            try? handle.close()
        } else {
            try? line.write(toFile: logPath, atomically: false, encoding: .utf8)
        }
        return true
    }

    @discardableResult
    static func open(_ text: String, from directory: String?) -> Bool {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return false }
        // A scheme of more than one letter is an address; `C:\` is not.
        if let url = URL(string: text), let scheme = url.scheme, scheme.count > 1 {
            if url.isFileURL {
                return open(url.path, from: directory)
            }
            return hand(url)
        }
        guard let file = resolve(text, from: directory) else {
            Trace.log("link", "no such file: \(text)")
            return false
        }
        return hand(file)
    }
}
