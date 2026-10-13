import Foundation

var failures = 0
func check(_ name: String, _ got: [Int], _ want: [Int]) {
    if got == want { print("ok   \(name)") } else {
        failures += 1
        print("FAIL \(name): got \(got), wanted \(want)")
    }
}

typealias C = HibernationPolicy.Candidate
func tab(_ shown: UInt64, visible: Bool = false, blocked: Bool = false) -> C {
    C(visible: visible, lastShown: shown, blocked: blocked)
}
func sleepers(_ tabs: [C], _ limit: Int) -> [Int] {
    HibernationPolicy.sleepers(tabs, limit: limit).sorted()
}

// Nine tabs shown in order 1...9, the last on screen: the six most recent
// stay, the three oldest sleep.
let nine = (1...9).map { tab(UInt64($0), visible: $0 == 9) }
check("nine tabs, limit six: the three least recent sleep", sleepers(nine, 6), [0, 1, 2])
check("six tabs, limit six: none", sleepers(Array(nine.suffix(6)), 6), [])
check("fewer than the limit: none", sleepers(Array(nine.suffix(2)), 6), [])
check("the tab on screen is never chosen, even when it is the oldest",
      sleepers([tab(1, visible: true), tab(5), tab(6), tab(7)], 2), [1, 2])
check("a visible tab counts against the limit",
      sleepers([tab(9, visible: true), tab(8), tab(7), tab(6)], 3), [3])
check("two windows, two on screen, limit three: one other stays, the older two sleep",
      sleepers([tab(1, visible: true), tab(2, visible: true), tab(3), tab(4), tab(5)], 3), [2, 3])
check("more tabs on screen than the limit: they all stay, the rest sleep",
      sleepers([tab(1, visible: true), tab(2, visible: true), tab(3, visible: true), tab(4)], 2), [3])
check("a tab holding a selection is not let go of",
      sleepers([tab(1, blocked: true), tab(2), tab(3), tab(9, visible: true)], 2), [1])
check("blocked, it does not use up the room of the recent ones",
      sleepers([tab(1, blocked: true), tab(2), tab(3), tab(9, visible: true)], 3), [])
check("the room goes by recency, not by position",
      sleepers([tab(7), tab(2), tab(9, visible: true), tab(5)], 3), [1])
check("limit zero: everything off screen sleeps",
      sleepers([tab(1), tab(2), tab(3, visible: true)], 0), [0, 1])

// The limit, from the environment.
func same(_ name: String, _ got: Int?, _ want: Int?) {
    if got == want { print("ok   \(name)") } else {
        failures += 1
        print("FAIL \(name): got \(String(describing: got)), wanted \(String(describing: want))")
    }
}
same("no KEEP_HYDRATED_LIMIT: six", HibernationPolicy.parseLimit(nil), 6)
same("KEEP_HYDRATED_LIMIT=3", HibernationPolicy.parseLimit("3"), 3)
same("KEEP_HYDRATED_LIMIT=off: never", HibernationPolicy.parseLimit("off"), nil)
same("KEEP_HYDRATED_LIMIT=nonsense: six", HibernationPolicy.parseLimit("x"), 6)

// What a `text:` action types into a hibernated tab, through the daemon.
func typed(_ name: String, _ action: String, _ want: [UInt8]?) {
    let got = TextAction.bytes(of: action)
    if got == want { print("ok   \(name)") } else {
        failures += 1
        print("FAIL \(name): got \(String(describing: got)), wanted \(String(describing: want))")
    }
}
typed("Enter, as the trust answer sends it", "text:\\x0d", [0x0d])
typed("the arrow down, as the trust answer sends it", "text:\\x1b[B", [0x1b, 0x5b, 0x42])
typed("the colour scheme report", "text:\\x1b[?997;1n", Array("\u{1b}[?997;1n".utf8))
typed("plain text and the short escapes", "text:a\\tb\\r\\n\\\\", [0x61, 0x09, 0x62, 0x0d, 0x0a, 0x5c])
typed("text beyond ASCII, as UTF-8", "text:❯", Array("❯".utf8))
typed("an escape it does not know: nothing", "text:\\q", nil)
typed("a broken hex escape: nothing", "text:\\x0", nil)
typed("not a text action: nothing", "copy_to_clipboard", nil)

exit(failures == 0 ? 0 : 1)
