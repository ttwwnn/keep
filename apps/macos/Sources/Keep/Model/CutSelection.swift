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
