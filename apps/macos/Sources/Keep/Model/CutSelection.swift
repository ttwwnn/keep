import Foundation

/// What it takes to cut selected text out of the line a program is editing.
///
/// The terminal's selection is a view of the screen, not of the program's
/// text, so it cannot be deleted from here; the program is told with keys
/// instead, as a person would: move to the end of the selection, then
/// backspace over it. That works on the row the cursor is on — the prompt of
/// a shell — and across the rows of Claude Code's prompt box, whose text is
/// pieced back from its rows (`logical`); anywhere else a cut is only a copy.
enum CutSelection {
    /// The keys that delete `selected` from the row the cursor is on, or nil
    /// when that cannot be told: more than one row selected, or text that is
    /// not on the cursor's row exactly once. `row` is that row as it is on
    /// screen and `cursorColumn` the cell the cursor is in.
    static func keys(selected: String, row: String, cursorColumn: Int) -> String? {
        let wanted = Array(selected)
        let chars = Array(row)
        guard !wanted.isEmpty, !selected.contains("\n"), chars.count >= wanted.count else {
            return nil
        }
        var starts: [Int] = []
        for start in 0...(chars.count - wanted.count)
        where chars[start..<(start + wanted.count)].elementsEqual(wanted) {
            starts.append(start)
        }
        guard starts.count == 1, let start = starts.first else { return nil }
        let end = start + wanted.count
        let move = end - cursorColumn
        let steps = move >= 0 ? String(repeating: right, count: move)
            : String(repeating: left, count: -move)
        return steps + String(repeating: backspace, count: wanted.count)
    }

    /// The rows of the prompt box the cursor is in — between the rule above
    /// it and the rule below — with the box's gutter (`❯` and a blank on the
    /// first row, two blanks on the rest) taken off, and the screen row the
    /// box's first row is on. Nil when the cursor is in no box.
    static func promptBoxRows(_ lines: [String], cursorRow: Int) -> (rows: [String], first: Int)? {
        func isRule(_ line: String) -> Bool {
            line.trimmingCharacters(in: .whitespaces).hasPrefix(String(repeating: "─", count: 12))
        }
        guard lines.indices.contains(cursorRow), !isRule(lines[cursorRow]),
            let above = lines[..<cursorRow].lastIndex(where: isRule),
            let below = lines[(cursorRow + 1)...].firstIndex(where: isRule)
        else { return nil }
        let rows = lines[(above + 1)..<below].map { line -> String in
            var t = Substring(line)
            // Claude Code writes `❯` and a no-break space; a shell, `> `.
            if let first = t.first, first == "❯" || first == ">" {
                t = t.dropFirst()
                if let blank = t.first, blank.isWhitespace { t = t.dropFirst() }
            } else if t.hasPrefix("  ") {
                t = t.dropFirst(2)
            }
            return String(t)
        }
        guard rows.first.map({ $0.trimmingCharacters(in: .whitespaces).isEmpty }) == false else { return nil }
        return (rows, above + 1)
    }

    static func promptBox(_ lines: [String], cursorRow: Int) -> [String]? {
        promptBoxRows(lines, cursorRow: cursorRow)?.rows
    }

    /// The keys that empty the prompt when what was selected is the whole of
    /// it (however its rows were wrapped): to the end, and back over all of
    /// it. More of either than needed does nothing at the ends, so the rows'
    /// breaks — a typed new line or only a wrap, which the screen cannot
    /// tell apart — are covered by counting each as one.
    static func keysEmptying(selected: String, box: [String]) -> String? {
        func squeezed(_ text: String) -> String { text.filter { !$0.isWhitespace } }
        let all = box.joined(separator: "\n")
        guard !squeezed(selected).isEmpty, squeezed(selected) == squeezed(all) else { return nil }
        let count = box.reduce(0) { $0 + $1.count } + box.count
        return String(repeating: right, count: count) + String(repeating: backspace, count: count)
    }

    /// The text of a prompt box as the program holds it, rebuilt from its
    /// rows: each break between two rows stands for one character — the space
    /// the wrap swallowed, or a new line typed there — except where a word too
    /// long for the row was cut in the middle, which stands for none. Breaks are kept as `\n`, a blank either
    /// way. `starts[i]` is where row `i` begins in the text.
    static func logical(_ box: [String], width: Int) -> (text: [Character], starts: [Int]) {
        var text: [Character] = []
        var starts: [Int] = []
        for (i, row) in box.enumerated() {
            starts.append(text.count)
            text.append(contentsOf: row)
            guard i < box.count - 1 else { break }
            // A word longer than the row is cut where the row ends (Claude
            // Code wraps hard): the row is full, and the word across the
            // break — its end here, its start below — is longer than a row.
            // A word that fits would have gone down whole, leaving a blank.
            let next = box[i + 1]
            let tail = row.reversed().prefix { !$0.isWhitespace }.count
            let head = next.prefix { !$0.isWhitespace }.count
            let cutMidWord = row.count >= width && tail > 0 && head > 0 && tail + head > width
            if !cutMidWord { text.append("\n") }
        }
        return (text, starts)
    }

    /// The keys that delete `selected` from Claude Code's prompt box when it
    /// spans rows. The selection is found in the box's text with the blanks
    /// set aside — a copy of wrapped rows may carry new lines where the text
    /// has spaces — and must be found there exactly once. The cursor is at
    /// `cursorRow`, `cursorColumn` of the box (gutter off).
    static func keysAcrossRows(
        selected: String, box: [String], width: Int, cursorRow: Int, cursorColumn: Int
    ) -> String? {
        guard box.indices.contains(cursorRow) else { return nil }
        let (text, starts) = logical(box, width: width)
        let solid = text.indices.filter { !text[$0].isWhitespace }
        let wanted = selected.filter { !$0.isWhitespace }.map { $0 }
        guard !wanted.isEmpty, solid.count >= wanted.count else { return nil }
        var found: [Int] = []
        for at in 0...(solid.count - wanted.count)
        where (0..<wanted.count).allSatisfy({ text[solid[at + $0]] == wanted[$0] }) {
            found.append(at)
        }
        guard found.count == 1, let at = found.first else { return nil }
        var start = solid[at]
        var end = solid[at + wanted.count - 1] + 1
        // Blanks the selection begins or ends with go too.
        var leading = selected.prefix(while: \.isWhitespace).count
        while leading > 0, start > 0, text[start - 1].isWhitespace { start -= 1; leading -= 1 }
        var trailing = selected.reversed().prefix(while: \.isWhitespace).count
        while trailing > 0, end < text.count, text[end].isWhitespace { end += 1; trailing -= 1 }
        let cursor = starts[cursorRow] + min(max(cursorColumn, 0), box[cursorRow].count)
        let move = end - cursor
        let steps = move >= 0 ? String(repeating: right, count: move)
            : String(repeating: left, count: -move)
        return steps + String(repeating: backspace, count: end - start)
    }

    static let right = "\u{1B}[C"
    static let left = "\u{1B}[D"
    static let backspace = "\u{7F}"

    /// `keys` as the argument of libghostty's `text:` action, which takes
    /// Zig string escapes and sends the bytes as typed, not as pasted.
    static func action(for keys: String) -> String {
        "text:" + keys.unicodeScalars.map { scalar in
            scalar.value < 0x20 || scalar.value == 0x7F
                ? String(format: "\\x%02x", scalar.value) : String(scalar)
        }.joined()
    }
}
