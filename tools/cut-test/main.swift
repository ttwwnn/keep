import Foundation

var failures = 0
func check(_ name: String, _ got: String?, _ want: String?) {
    if got == want { print("ok   \(name)") } else {
        failures += 1
        print("FAIL \(name): got \(String(describing: got)), wanted \(String(describing: want))")
    }
}

let R = CutSelection.right, L = CutSelection.left, B = CutSelection.backspace
let row = "❯ ola mundo cruel"
// "mundo" spans columns 6..<11.
check("cursor at the end: left over the tail, then backspace",
      CutSelection.keys(selected: "mundo", row: row, cursorColumn: 17),
      String(repeating: L, count: 6) + String(repeating: B, count: 5))
check("cursor right after the selection: just backspace",
      CutSelection.keys(selected: "mundo", row: row, cursorColumn: 11),
      String(repeating: B, count: 5))
check("cursor before the selection: right, then backspace",
      CutSelection.keys(selected: "mundo", row: row, cursorColumn: 2),
      String(repeating: R, count: 9) + String(repeating: B, count: 5))
check("the text twice on the row: not told apart",
      CutSelection.keys(selected: "a", row: "banana", cursorColumn: 6), nil)
check("more than one row", CutSelection.keys(selected: "a\nb", row: "a", cursorColumn: 1), nil)
check("not on the cursor's row", CutSelection.keys(selected: "zzz", row: row, cursorColumn: 5), nil)
check("nothing selected", CutSelection.keys(selected: "", row: row, cursorColumn: 5), nil)
check("as an action",
      CutSelection.action(for: R + B), "text:\\x1b[C\\x7f")

let screen = [
    "conversa",
    String(repeating: "─", count: 40),
    "❯ primeira linha do pedido que",
    "  continua aqui",
    String(repeating: "─", count: 40),
    "  ⏵⏵ bypass permissions on",
]
check("the prompt box, gutter off", CutSelection.promptBox(screen, cursorRow: 3)?.joined(separator: "|"),
      "primeira linha do pedido que|continua aqui")
check("no box around the cursor", CutSelection.promptBox(screen, cursorRow: 0) == nil ? "nil" : "box", "nil")
let n = "primeira linha do pedido que".count + "continua aqui".count + 2
check("the whole prompt selected, wrapped or not: emptied",
      CutSelection.keysEmptying(selected: "primeira linha do pedido que\ncontinua aqui", box: ["primeira linha do pedido que", "continua aqui"]),
      String(repeating: R, count: n) + String(repeating: B, count: n))
check("part of it across rows: not emptied",
      CutSelection.keysEmptying(selected: "pedido que\ncontinua", box: ["primeira linha do pedido que", "continua aqui"]), nil)

// Claude Code 2.1.296's prompt box, as it was on screen (12/10), each cut
// tried on the real editor and found right (tools/cut-test/claude-prompt-box.jsonl:
// the box between its rules, the cursor, the selection and the keys).
let fixtures = (try? String(contentsOfFile: "tools/cut-test/claude-prompt-box.jsonl", encoding: .utf8)) ?? ""
var tried = 0
for line in fixtures.split(separator: "\n") {
    guard let json = try? JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any],
        let lines = json["lines"] as? [String], let row = json["cursorRow"] as? Int,
        let column = json["cursorCol"] as? Int, let selected = json["selected"] as? String,
        let want = json["keys"] as? String
    else { continue }
    tried += 1
    let name = "Claude's box: cut \(selected.prefix(24).replacingOccurrences(of: "\n", with: "⏎"))…"
    guard let (box, first) = CutSelection.promptBoxRows(lines, cursorRow: row) else {
        check(name, "no box", want); continue
    }
    let width = lines[first - 1].trimmingCharacters(in: .whitespaces).count - 4
    let keys = CutSelection.keysAcrossRows(
        selected: selected, box: box, width: width, cursorRow: row - first, cursorColumn: column - 2)
        ?? CutSelection.keysEmptying(selected: selected, box: box)
    check(name, keys.map { Data($0.utf8).map { String(format: "%02x", $0) }.joined() }, want)
}
check("the real cases were read", String(tried), "8")
let twice = ["─────────────────────", "❯\u{a0}alfa beta alfa beta", "─────────────────────"]
check("words found twice in the box: not cut", CutSelection.keysAcrossRows(
    selected: "alfa", box: CutSelection.promptBox(twice, cursorRow: 1) ?? [], width: 17, cursorRow: 0, cursorColumn: 20), nil)
exit(failures == 0 ? 0 : 1)
