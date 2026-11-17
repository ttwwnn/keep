import Foundation

/// The files of a Claude Code conversation, for the panel beside it: the ones
/// the person sent (a picture pasted, a file dropped or named in a message, an
/// upload) and the ones the conversation made for them: handed over with
/// SendUserFile, or named in an answer (by path or by name) and written or
/// created since it began. What it made only on the way (the HTML a PDF was
/// printed from, a script, a scratch file) and never showed is left out. Newest first.
struct ChatFiles: Equatable {
    var sent: [ChatFileItem] = []
    var made: [ChatFileItem] = []
    /// The Mac's Downloads folder, beside the conversation's own: to drag a file from it into the prompt.
    var downloads: [ChatFileItem] = []
    /// The folder could not be read (macOS asks before an app reads Downloads): not the same as empty.
    var downloadsDenied = false
}

/// A file as its row shows it: the name without its extension, the extension as its type, when it last
/// changed, how big it is.
struct ChatFileItem: Identifiable, Equatable {
    let url: URL
    var id: URL { url }
    let name: String
    let type: String
    let modified: Date
    let size: Int64

    /// Reads the disk: built off the main thread, with the lists.
    init(_ url: URL) {
        self.url = url
        let values = try? url.resourceValues(forKeys: [.contentModificationDateKey, .fileSizeKey])
        name = (url.lastPathComponent as NSString).deletingPathExtension
        type = url.pathExtension.lowercased()
        modified = values?.contentModificationDate ?? .distantPast
        size = Int64(values?.fileSize ?? 0)
    }
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
    /// In the order the conversation came to them. `own`: written by it, whatever the file's date (otherwise
    /// only if created after it began). `handed`: given to the person with SendUserFile.
    private var made: [(path: String, own: Bool, handed: Bool)] = []
    /// What its answers name, by path and by bare name: a file it made and named there is a result, not a step.
    private var answered = Set<String>()
    private var answeredNames = Set<String>()
    /// Each path once, so a read checks the disk once per file however often it was named.
    private var madeIndex: [String: Int] = [:]
    private var sentSeen = Set<String>()

    /// Read with the lists; a test points it elsewhere.
    let downloadsFolder: URL?

    init(transcript: String, session: String, imageFolder: String,
         downloadsFolder: URL? = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first) {
        self.transcript = transcript
        self.session = session
        self.imageFolder = imageFolder
        self.downloadsFolder = downloadsFolder
    }

    /// What a folder holds that one would drag: its files and folders, hidden ones and packages' insides left out.
    /// Nil when the folder cannot be read.
    static func downloads(in folder: URL?) -> [ChatFileItem]? {
        guard let folder,
            let urls = try? FileManager.default.contentsOfDirectory(
                at: folder, includingPropertiesForKeys: [.contentModificationDateKey, .fileSizeKey],
                options: [.skipsHiddenFiles])
        else { return nil }
        return urls.map(ChatFileItem.init)
    }

    /// The Downloads listing, read again only when the folder's date moves (a file added, renamed or taken out).
    /// shortcut: a file still downloading keeps the size it was listed with until something else changes the folder.
    private var downloadsCache: (changed: Date, items: [ChatFileItem]?)?
    private func downloadsNow() -> [ChatFileItem]? {
        let changed = downloadsFolder.flatMap {
            (try? FileManager.default.attributesOfItem(atPath: $0.path))?[.modificationDate] as? Date
        }
        if let changed, let cached = downloadsCache, cached.changed == changed { return cached.items }
        let items = Self.downloads(in: downloadsFolder)
        // A refusal is not kept: access given in System Settings does not move the folder's date.
        if let changed, items != nil { downloadsCache = (changed, items) }
        return items
    }

    /// Pictures a conversation pasted are not kept for good: a conversation's folder untouched for 30 days
    /// goes, once a launch. One still read gets its pictures written again from the transcript. Each window
    /// reads on a queue of its own, so the folders already gone through are kept under a lock.
    static let keptFor: TimeInterval = 30 * 86_400
    nonisolated(unsafe) private static var pruned = Set<String>()
    private static let prunedLock = NSLock()
    static func pruneOnce(_ folder: String, now: Date = Date()) {
        // Held through the whole sweep: a window that comes in meanwhile waits, and does not read pictures
        // the sweep is about to take away.
        prunedLock.withLock {
            guard pruned.insert(folder).inserted else { return }
            let fm = FileManager.default
            for name in (try? fm.contentsOfDirectory(atPath: folder)) ?? [] {
                let path = (folder as NSString).appendingPathComponent(name)
                guard let date = (try? fm.attributesOfItem(atPath: path))?[.modificationDate] as? Date,
                    now.timeIntervalSince(date) > keptFor
                else { continue }
                try? fm.removeItem(atPath: path)
            }
        }
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
        // Here and not at init: a read runs off the main thread, and a big cache takes a while to go through.
        Self.pruneOnce(imageFolder)
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
        // Its own: handed over, written by it, or created since it began (a file it only changed, a config,
        // a test, is not one it made).
        let candidates = made.filter { entry in
            guard isFile(entry.path) else { return false }
            if entry.handed || entry.own { return true }
            guard !Self.ignored(entry.path),
                let date = (try? fm.attributesOfItem(atPath: entry.path))?[.creationDate] as? Date
            else { return false }
            return date >= begun
        }
        // A bare name in an answer stands for a file only when one of its own goes by that name:
        // `index.html` named once does not bring in every other index.html it made on the way.
        var sameName: [String: Int] = [:]
        for entry in candidates { sameName[(entry.path as NSString).lastPathComponent, default: 0] += 1 }
        let madeNow = candidates.filter { entry in
            let name = (entry.path as NSString).lastPathComponent
            return entry.handed || answered.contains(entry.path) || (answeredNames.contains(name) && sameName[name] == 1)
        }.map(\.path).filter { !sentSet.contains($0) && $0 != transcript }
        let downloads = downloadsNow()
        return ChatFiles(
            sent: sentNow.reversed().map { ChatFileItem(URL(fileURLWithPath: $0)) },
            made: madeNow.reversed().map { ChatFileItem(URL(fileURLWithPath: $0)) },
            downloads: downloads ?? [],
            // Refused, not missing: a folder that is not there is only empty.
            downloadsDenied: downloads == nil
                && downloadsFolder.map { FileManager.default.fileExists(atPath: $0.path) } == true)
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
            answered = []
            answeredNames = []
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
            case "text":
                let text = block["text"] as? String ?? ""
                let paths = Self.paths(in: text)
                paths.forEach { addMade($0) }
                answered.formUnion(paths)
                answeredNames.formUnion(Self.fileNames(in: text))
            case "tool_use":
                let input = block["input"] as? [String: Any] ?? [:]
                switch block["name"] as? String {
                case "Write": if let path = input["file_path"] as? String { addMade(path, own: true) }
                case "NotebookEdit": if let path = input["notebook_path"] as? String { addMade(path, own: true) }
                case "SendUserFile": ((input["files"] as? [String]) ?? []).forEach { addMade($0, own: true, handed: true) }
                case "Bash": Self.paths(in: input["command"] as? String ?? "").forEach { addMade($0) }
                default: break
                }
            default: break
            }
        }
    }

    private func addMade(_ path: String, own: Bool = false, handed: Bool = false) {
        if let index = madeIndex[path] {
            made[index].own = made[index].own || own
            made[index].handed = made[index].handed || handed
            return
        }
        madeIndex[path] = made.count
        made.append((path, own, handed))
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

    /// The bare file names a text holds (`Resumo-2026.pdf`, `**grafico.png**`): words with an extension and no slash.
    static func fileNames(in text: String) -> [String] {
        text.split(whereSeparator: \.isWhitespace).compactMap { word in
            let name = String(word).trimmingCharacters(in: wrapping)
            guard !name.contains("/"), !(name as NSString).pathExtension.isEmpty,
                (name as NSString).pathExtension.allSatisfy({ $0.isLetter || $0.isNumber })
            else { return nil }
            return name
        }
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
