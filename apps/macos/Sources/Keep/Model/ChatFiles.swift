import Foundation

/// The files of a Claude Code conversation, for the panel beside it: the ones
/// the person sent (a picture pasted, a file dropped or named in a message, an
/// upload) and the ones the conversation made (written, handed over with
/// SendUserFile, or named by it and created since it began). Newest first.
struct ChatFiles: Equatable {
    var sent: [URL] = []
    var made: [URL] = []
}

/// Reads one conversation's transcript as it grows: each read takes only the
/// lines added since the last, so a transcript of a hundred megabytes is read
/// in full once. Not thread-safe: one serial queue reads it.
final class ChatFilesReader: @unchecked Sendable {
    let transcript: String
    let session: String
    /// Where a pasted picture is written out, so it can be opened: the
    /// transcript holds it only as base64.
    let imageFolder: String
    private var offset: UInt64 = 0
    private var start: Date?
    private var sent: [String] = []
    /// In the order the conversation came to them. Written or handed over (`own`): its files, whatever their
    /// date. Named in a command or an answer: its only if created after it began.
    private var made: [(path: String, own: Bool)] = []
    /// Each path once, so a read checks the disk once per file however often it was named.
    private var madeIndex: [String: Int] = [:]
    private var sentSeen = Set<String>()

    init(transcript: String, session: String, imageFolder: String) {
        self.transcript = transcript
        self.session = session
        self.imageFolder = imageFolder
    }

    /// The transcript and the conversation a Claude Code status line file
    /// names (`transcript_path`, `session_id`).
    static func source(statusFile data: Data) -> (transcript: String, session: String)? {
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let transcript = json["transcript_path"] as? String, !transcript.isEmpty,
            let session = json["session_id"] as? String, !session.isEmpty
        else { return nil }
        return (transcript, session)
    }

    static var cacheFolder: String {
        let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first?.path
            ?? NSTemporaryDirectory()
        return (caches as NSString).appendingPathComponent("Keep/conversas")
    }

    func read() -> ChatFiles {
        readNewLines()
        let fm = FileManager.default
        func isFile(_ path: String) -> Bool {
            var isDirectory: ObjCBool = false
            return fm.fileExists(atPath: path, isDirectory: &isDirectory) && !isDirectory.boolValue
        }
        let uploads = (NSHomeDirectory() as NSString).appendingPathComponent(".claude/uploads/\(session)")
        let uploaded = ((try? fm.contentsOfDirectory(atPath: uploads)) ?? []).map { (uploads as NSString).appendingPathComponent($0) }
        let sentNow = unique(sent + uploaded).filter(isFile)
        let sentSet = Set(sentNow)
        let begun = start ?? .distantPast
        // Created since it began: a file it only changed (a config, a test) is not one it made.
        let madeNow = made.filter { entry in
            guard isFile(entry.path) else { return false }
            if entry.own { return true }
            guard !Self.ignored(entry.path),
                let date = (try? fm.attributesOfItem(atPath: entry.path))?[.creationDate] as? Date
            else { return false }
            return date >= begun
        }.map(\.path).filter { !sentSet.contains($0) && $0 != transcript }
        return ChatFiles(
            sent: sentNow.reversed().map { URL(fileURLWithPath: $0) },
            made: madeNow.reversed().map { URL(fileURLWithPath: $0) })
    }

    // MARK: - the transcript

    private func readNewLines() {
        guard let handle = FileHandle(forReadingAtPath: transcript) else { return }
        defer { try? handle.close() }
        let size = (try? handle.seekToEnd()) ?? 0
        if size < offset {
            // Written over: read again from the top.
            offset = 0
            start = nil
            sent = []
            made = []
            madeIndex = [:]
            sentSeen = []
        }
        guard size > offset else { return }
        try? handle.seek(toOffset: offset)
        guard let data = try? handle.readToEnd(), let lastNewline = data.lastIndex(of: 0x0A) else { return }
        // A line still being written waits for the next read.
        let complete = data[data.startIndex...lastNewline]
        offset += UInt64(complete.count)
        for line in complete.split(separator: 0x0A) { take(Data(line)) }
    }

    private static let userMark = Data(#""type":"user""#.utf8)
    private static let assistantMark = Data(#""type":"assistant""#.utf8)

    private func take(_ line: Data) {
        if start == nil, let json = try? JSONSerialization.jsonObject(with: line) as? [String: Any],
            let stamp = json["timestamp"] as? String
        {
            start = Self.date(stamp)
        }
        guard line.range(of: Self.userMark) != nil || line.range(of: Self.assistantMark) != nil,
            let json = try? JSONSerialization.jsonObject(with: line) as? [String: Any],
            json["isSidechain"] as? Bool != true, json["isMeta"] as? Bool != true,
            let message = json["message"] as? [String: Any]
        else { return }
        switch json["type"] as? String {
        case "user": takeUser(message, json)
        case "assistant": takeAssistant(message)
        default: break
        }
    }

    private func takeUser(_ message: [String: Any], _ json: [String: Any]) {
        // The summary of a compaction, a message from another tab or a hook: not the person's.
        guard json["isCompactSummary"] as? Bool != true,
            (json["origin"] as? [String: Any])?["kind"].map({ $0 as? String == "human" }) ?? true
        else { return }
        if let text = message["content"] as? String {
            takeTyped(text)
            return
        }
        guard let blocks = message["content"] as? [[String: Any]] else { return }
        var ids = (json["imagePasteIds"] as? [Int]) ?? []
        for (index, block) in blocks.enumerated() {
            switch block["type"] as? String {
            case "text": takeTyped(block["text"] as? String ?? "")
            case "image":
                let id = ids.isEmpty ? nil : ids.removeFirst()
                let uuid = json["uuid"] as? String ?? UUID().uuidString
                if let path = writeImage(block, id: id, fallback: "\(uuid.prefix(8))-\(index)") { addSent(path) }
            default: break
            }
        }
    }

    /// What the person typed or dropped, not what came in their name (a task's
    /// notice, a command's output: those open with a tag).
    private func takeTyped(_ text: String) {
        guard !text.trimmingCharacters(in: .whitespaces).hasPrefix("<") else { return }
        Self.paths(in: text).forEach(addSent)
    }

    private func takeAssistant(_ message: [String: Any]) {
        guard let blocks = message["content"] as? [[String: Any]] else { return }
        for block in blocks {
            switch block["type"] as? String {
            case "text": Self.paths(in: block["text"] as? String ?? "").forEach { addMade($0, own: false) }
            case "tool_use":
                let input = block["input"] as? [String: Any] ?? [:]
                switch block["name"] as? String {
                case "Write": if let path = input["file_path"] as? String { addMade(path, own: true) }
                case "NotebookEdit": if let path = input["notebook_path"] as? String { addMade(path, own: true) }
                case "SendUserFile": ((input["files"] as? [String]) ?? []).forEach { addMade($0, own: true) }
                case "Bash": Self.paths(in: input["command"] as? String ?? "").forEach { addMade($0, own: false) }
                default: break
                }
            default: break
            }
        }
    }

    private func addMade(_ path: String, own: Bool) {
        if let index = madeIndex[path] {
            if own { made[index].own = true }
            return
        }
        madeIndex[path] = made.count
        made.append((path, own))
    }

    private func addSent(_ path: String) {
        if sentSeen.insert(path).inserted { sent.append(path) }
    }

    private func writeImage(_ block: [String: Any], id: Int?, fallback: String) -> String? {
        guard let source = block["source"] as? [String: Any], source["type"] as? String == "base64",
            let base64 = source["data"] as? String
        else { return nil }
        let type = (source["media_type"] as? String)?.split(separator: "/").last.map(String.init) ?? "png"
        let ext = type == "jpeg" ? "jpg" : type
        let folder = (imageFolder as NSString).appendingPathComponent(session)
        // Each picture its own name: a message without paste ids may hold several.
        let name = "Imagem \(id.map(String.init) ?? fallback).\(ext)"
        let path = (folder as NSString).appendingPathComponent(name)
        if FileManager.default.fileExists(atPath: path) { return path }
        guard let data = Data(base64Encoded: base64) else { return nil }
        do {
            // A picture may hold anything on the person's screen: theirs alone to read.
            try FileManager.default.createDirectory(
                atPath: folder, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
            try data.write(to: URL(fileURLWithPath: path), options: .atomic)
            try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: path)
        } catch {
            return nil
        }
        return path
    }

    // MARK: - paths in text

    /// The absolute or `~/` paths a text holds: as words (spaces escaped by a
    /// backslash kept, as a dropped file arrives), between quotes, and with
    /// the sentence's punctuation, a `file://` or a `:line` taken off.
    static func paths(in text: String) -> [String] {
        var words: [String] = []
        var current = ""
        var escaping = false
        for char in text {
            if escaping { current.append(char); escaping = false }
            else if char == "\\" { escaping = true }
            else if char.isWhitespace { words.append(current); current = "" }
            else { current.append(char) }
        }
        words.append(current)
        // A word that opens with a quote is read whole between the quotes, below.
        words.removeAll { $0.first.map { "\"'`".contains($0) } ?? false }
        for quote in ["\"", "'", "`"] {
            let parts = text.components(separatedBy: quote)
            words += stride(from: 1, to: parts.count, by: 2).map { parts[$0] }
        }
        var found: [String] = []
        for word in words {
            var path = word.trimmingCharacters(in: wrapping)
            if path.hasPrefix("file://") { path = URL(string: path)?.path ?? path }
            if let suffix = path.range(of: #":\d+(:\d+)?$"#, options: .regularExpression) {
                path.removeSubrange(suffix)
            }
            guard path.hasPrefix("/") || path.hasPrefix("~/"), path.count > 1 else { continue }
            let full = ((path as NSString).expandingTildeInPath as NSString).standardizingPath
            if !found.contains(full) { found.append(full) }
        }
        return found
    }

    private static let wrapping = CharacterSet(charactersIn: "`'\"()<>[]{}.,;:!?*")

    /// Named often, never anybody's work: the system's, Claude Code's own
    /// records and a background task's output.
    private static func ignored(_ path: String) -> Bool {
        let claude = (NSHomeDirectory() as NSString).appendingPathComponent(".claude/")
        return ["/usr/", "/bin/", "/sbin/", "/opt/", "/System/", "/Library/", "/Applications/", "/dev/", "/etc/",
                "/private/etc/", claude].contains { path.hasPrefix($0) }
            || path.hasSuffix(".output") && path.contains("/tasks/")
            || ["-wal", "-shm", "-journal"].contains { path.hasSuffix($0) }
    }

    private func unique(_ paths: [String]) -> [String] {
        var seen = Set<String>()
        return paths.filter { seen.insert($0).inserted }
    }

    private static func date(_ stamp: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter.date(from: stamp) ?? ISO8601DateFormatter().date(from: stamp)
    }
}
