import Foundation

/// What it takes to cut selected text out of the line a program is editing.
///
/// The terminal's selection is a view of the screen, not of the program's
/// text, so it cannot be deleted from here; the program is told with keys
/// instead, as a person would: move to the end of the selection, then
/// backspace over it. That works when the selection is on the row the cursor
/// is on — the prompt of a shell or of Claude Code — and nowhere else, so
/// anywhere else a cut is only a copy.
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
    /// it and the rule below — with the box's gutter (`❯ ` on the first row,
    /// two spaces on the rest) taken off. Nil when the cursor is in no box.
    static func promptBox(_ lines: [String], cursorRow: Int) -> [String]? {
        func isRule(_ line: String) -> Bool {
            line.trimmingCharacters(in: .whitespaces).hasPrefix(String(repeating: "─", count: 12))
        }
        guard lines.indices.contains(cursorRow), !isRule(lines[cursorRow]),
            let above = lines[..<cursorRow].lastIndex(where: isRule),
            let below = lines[(cursorRow + 1)...].firstIndex(where: isRule)
        else { return nil }
        let rows = lines[(above + 1)..<below].map { line -> String in
            var t = Substring(line)
            if t.hasPrefix("❯ ") || t.hasPrefix("> ") || t.hasPrefix("  ") { t = t.dropFirst(2) }
            else if t.hasPrefix("❯") || t.hasPrefix(">") { t = t.dropFirst() }
            return String(t)
        }
        return rows.first.map { $0.trimmingCharacters(in: .whitespaces).isEmpty } == false ? rows : nil
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
