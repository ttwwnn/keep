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

    /// The file a ⌘-click meant when the runtime handed over only part of
    /// its path. Two ways that happens, both seen in Claude Code:
    ///   - a path longer than the row is broken across rows by the program
    ///     itself, the rest indented on the next one, so to the terminal they
    ///     are two words on two lines;
    ///   - a file dropped into the tab arrives with its spaces escaped
    ///     (`Captura\ de\ Tela.png`), and the runtime stops at the backslash.
    /// So the word around what was found is taken whole, escaped spaces and
    /// all, joined with the row above when it starts its row and with the row
    /// below when it ends it, and the longest piece that names a file wins.
    /// `lines` is the screen, `row` roughly where the pointer was on it.
    static func recover(
        _ text: String, in lines: [String], near row: Int?, from directory: String?
    ) -> URL? {
        for hit in hits(of: text, in: lines, near: row) {
            for candidate in candidates(lines, hit.line, hit.start, hit.end) {
                if let url = resolve(candidate, from: directory) { return url }
            }
        }
        return nil
    }

    /// The pieces to try for a hit on `lines[line]` between `start` and
    /// `end`, longest first.
    private static func candidates(_ lines: [String], _ line: Int, _ start: Int, _ end: Int) -> [String] {
        let chars = Array(lines[line])
        let (low, high) = word(chars, start, end)
        let middle = String(chars[low..<high])
        var before: [String] = []
        var index = line
        // Up the screen while the word starts its row and the row above
        // ends in one; past the first, only rows that are that word alone.
        if chars[..<low].allSatisfy(\.isWhitespace) {
            while index > 0, before.count < 3 {
                index -= 1
                let above = Array(lines[index].trimmingTrailingWhitespace)
                guard !above.isEmpty else { break }
                let (l, h) = word(above, above.count - 1, above.count)
                before.insert(String(above[l..<h]), at: 0)
                guard above[..<l].allSatisfy(\.isWhitespace) else { break }
            }
        }
        var after: [String] = []
        index = line
        if chars[high...].allSatisfy(\.isWhitespace) {
            while index < lines.count - 1, after.count < 3 {
                index += 1
                let below = Array(lines[index])
                guard let first = below.firstIndex(where: { !$0.isWhitespace }) else { break }
                let (l, h) = word(below, first, first + 1)
                after.append(String(below[l..<h]))
                guard below[h...].allSatisfy(\.isWhitespace) else { break }
            }
        }
        var found: [String] = []
        for b in (0...before.count).reversed() {
            for a in (0...after.count).reversed() {
                let joined = before.suffix(b).joined() + middle + after.prefix(a).joined()
                for piece in [unescaped(joined), joined] {
                    for trimmed in [piece, piece.trimmingCharacters(in: wrapping)]
                    where !trimmed.isEmpty && !found.contains(trimmed) {
                        found.append(trimmed)
                    }
                }
            }
        }
        return found
    }

    /// The run of text around `start..<end` that a shell would read as one
    /// word: no blanks in it, except a blank a backslash escapes.
    private static func word(_ chars: [Character], _ start: Int, _ end: Int) -> (Int, Int) {
        var low = start
        while low > 0, !chars[low - 1].isWhitespace || (low >= 2 && chars[low - 2] == "\\") {
            low -= 1
        }
        var high = end
        while high < chars.count, !chars[high].isWhitespace || (high > 0 && chars[high - 1] == "\\") {
            high += 1
        }
        return (low, high)
    }

    /// What a word dropped into a shell stands for: `\x` is `x`.
    private static func unescaped(_ text: String) -> String {
        var out = ""
        var escaping = false
        for char in text {
            if escaping { out.append(char); escaping = false }
            else if char == "\\" { escaping = true }
            else { out.append(char) }
        }
        return out
    }

    /// Quotes, brackets and the sentence's own punctuation around a path.
    private static let wrapping = CharacterSet(charactersIn: "`'\"()<>[]{}.,;:!?")

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

    /// The file or folder the word under the pointer stands for, pieced back
    /// together from the screen as for a click, or nil when it names nothing.
    static func file(
        for word: String, from directory: String?, screen: [String], near row: Int?
    ) -> URL? {
        recover(word, in: screen, near: row, from: directory)
            ?? resolve(word, from: directory)
    }

    /// The word at a cell of the screen, as a shell would read it (blanks
    /// escaped by a backslash stay inside it), or nil on a blank or off the
    /// screen.
    static func wordAt(_ lines: [String], row: Int, column: Int) -> String? {
        guard lines.indices.contains(row) else { return nil }
        let chars = Array(lines[row])
        guard chars.indices.contains(column),
            !chars[column].isWhitespace || (column > 0 && chars[column - 1] == "\\")
        else { return nil }
        let (low, high) = word(chars, column, column + 1)
        return String(chars[low..<high])
    }

    /// Shows a file in the Finder, selected in its folder. Under test the
    /// request is appended to the log, as an opening is.
    @discardableResult
    static func reveal(_ url: URL) -> Bool {
        guard let logPath else {
            NSWorkspace.shared.activateFileViewerSelecting([url])
            return true
        }
        let line = "reveal " + url.path + "\n"
        if let handle = FileHandle(forWritingAtPath: logPath) {
            handle.seekToEndOfFile()
            handle.write(Data(line.utf8))
            try? handle.close()
        } else {
            try? line.write(toFile: logPath, atomically: false, encoding: .utf8)
        }
        return true
    }

    /// Opens a file already found, the same way a click does.
    @discardableResult
    static func open(_ url: URL) -> Bool { hand(url) }

    /// A word under a ⌘-click the runtime saw no link in: opened only when
    /// the screen around it names a file, and silently left alone otherwise.
    @discardableResult
    static func openRecovered(
        _ word: String, from directory: String?, screen: [String], near row: Int?
    ) -> Bool {
        guard let file = recover(word, in: screen, near: row, from: directory) else { return false }
        return hand(file)
    }

    /// What the runtime found under a click, opened: an address goes to the
    /// app for it, anything else is a path looked for as above.
    ///
    /// An address the runtime read off the screen may be only the part of it
    /// on the clicked row, Claude Code having carried the rest onto the next
    /// one, so it is pieced back together from the screen first (`columns`
    /// says how wide a full row is). One the program put behind its text
    /// (OSC 8) arrives `whole` and is opened as it is.
    @discardableResult
    static func open(
        _ text: String, from directory: String?, screen: [String] = [], near row: Int? = nil,
        columns: Int = 0, whole: Bool = false
    ) -> Bool {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return false }
        // A scheme of more than one letter is an address; `C:\` is not.
        if let url = URL(string: text), let scheme = url.scheme, scheme.count > 1 {
            if url.isFileURL {
                return open(url.path, from: directory)
            }
            if !whole, let longer = webAddress(around: text, in: screen, near: row, columns: columns) {
                return hand(longer)
            }
            return hand(url)
        }
        guard let file = recover(text, in: screen, near: row, from: directory)
            ?? resolve(text, from: directory)
        else {
            Trace.log("link", "no such file: \(text)")
            return false
        }
        return hand(file)
    }

    // MARK: - web addresses

    /// Whether a plain click may open what the runtime found: a path, a web
    /// or mail address. A program can put any address behind its text, and a
    /// click meant to put the cursor somewhere should not start an app; that
    /// takes ⌘-click, or the right button's "Abrir link".
    static func plainClickMayOpen(_ text: String) -> Bool {
        guard let url = URL(string: text), let scheme = url.scheme?.lowercased(), scheme.count > 1
        else { return true }
        return ["http", "https", "mailto", "ftp", "file"].contains(scheme)
    }

    /// Where a web address starts inside a word. Only these: a word read off
    /// the screen as `Nota:` would otherwise pass for one.
    private static let addressStarts = ["https://", "http://", "ftp://", "mailto:", "www."]

    /// The web address a word holds, or nil when it holds none: from where
    /// the address starts to where the sentence takes over again — a full
    /// stop, a comma, a closing quote, or the bracket of a markdown link.
    static func webAddress(in word: String) -> URL? {
        let lowered = word.lowercased()
        guard let start = addressStarts.compactMap({ lowered.range(of: $0)?.lowerBound }).min()
        else { return nil }
        var text = String(word[start...])
        let trailing = CharacterSet(charactersIn: "`'\".,;:!?>]}*")
        while let last = text.unicodeScalars.last {
            if trailing.contains(last) {
                text.removeLast()
            } else if last == ")",
                text.filter({ $0 == ")" }).count > text.filter({ $0 == "(" }).count {
                text.removeLast()
            } else {
                break
            }
        }
        if text.lowercased().hasPrefix("www.") { text = "https://" + text }
        guard let url = URL(string: text), let scheme = url.scheme?.lowercased() else { return nil }
        if scheme == "mailto" { return text.count > "mailto:".count ? url : nil }
        guard let host = url.host, host.contains(".") || host == "localhost" else { return nil }
        return url
    }

    /// The web address around a word on the screen, joined across rows when
    /// the program broke it there: Claude Code fills a row to its right edge
    /// with an address too long for it and goes on, indented, on the next.
    /// A row only continues on the next one when it is full — reaches within
    /// a few columns of `columns` — so an address that merely ends a row
    /// keeps clear of the sentence below it. `row` is roughly where the
    /// pointer was; nil when no hit of the word holds an address.
    static func webAddress(
        around text: String, in lines: [String], near row: Int?, columns: Int
    ) -> URL? {
        for hit in hits(of: text, in: lines, near: row) {
            let chars = Array(lines[hit.line])
            let (low, high) = word(chars, hit.start, hit.end)
            var joined = String(chars[low..<high])
            var index = hit.line
            if chars[..<low].allSatisfy(\.isWhitespace) {
                while index > 0, hit.line - index < 3 {
                    let above = Array(lines[index - 1].trimmingTrailingWhitespace)
                    guard isFull(above, columns), !above.isEmpty else { break }
                    let (l, h) = word(above, above.count - 1, above.count)
                    joined = String(above[l..<h]) + joined
                    index -= 1
                    guard above[..<l].allSatisfy(\.isWhitespace) else { break }
                }
            }
            index = hit.line
            var ending = Array(lines[index].trimmingTrailingWhitespace)
            if high >= ending.count {
                while index < lines.count - 1, index - hit.line < 3, isFull(ending, columns) {
                    let below = Array(lines[index + 1])
                    guard let first = below.firstIndex(where: { !$0.isWhitespace }) else { break }
                    let (l, h) = word(below, first, first + 1)
                    joined += String(below[l..<h])
                    index += 1
                    ending = Array(lines[index].trimmingTrailingWhitespace)
                    guard h >= ending.count else { break }
                }
            }
            if let url = webAddress(in: joined) { return url }
        }
        return nil
    }

    /// Whether a row reaches the right edge, give or take the margin a
    /// program keeps there (Claude Code keeps two columns).
    private static func isFull(_ row: [Character], _ columns: Int) -> Bool {
        columns > 0 && row.count >= columns - 3
    }

    /// Where `text` stands on the screen, the hits nearest `row` first.
    private static func hits(of text: String, in lines: [String], near row: Int?)
        -> [(line: Int, start: Int, end: Int)]
    {
        guard !text.isEmpty else { return [] }
        var hits: [(line: Int, start: Int, end: Int)] = []
        let wanted = Array(text)
        for (index, line) in lines.enumerated() {
            let chars = Array(line)
            guard chars.count >= wanted.count else { continue }
            for start in 0...(chars.count - wanted.count)
            where chars[start..<(start + wanted.count)].elementsEqual(wanted) {
                hits.append((index, start, start + wanted.count))
            }
        }
        guard let row else { return hits }
        return hits.enumerated().sorted {
            let a = abs($0.element.line - row), b = abs($1.element.line - row)
            return a != b ? a < b : $0.offset < $1.offset
        }.map(\.element)
    }

    /// A word under a click the runtime saw no link in, opened in the
    /// browser when the screen around it holds a web address.
    @discardableResult
    static func openRecoveredAddress(
        _ word: String, screen: [String], near row: Int?, columns: Int
    ) -> Bool {
        guard let url = webAddress(around: word, in: screen, near: row, columns: columns)
        else { return false }
        return hand(url)
    }
}

extension String {
    fileprivate var trimmingTrailingWhitespace: String {
        var chars = Substring(self)
        while let last = chars.last, last.isWhitespace { chars = chars.dropLast() }
        return String(chars)
    }
}
