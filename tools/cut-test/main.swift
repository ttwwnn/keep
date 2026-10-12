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
exit(failures == 0 ? 0 : 1)
