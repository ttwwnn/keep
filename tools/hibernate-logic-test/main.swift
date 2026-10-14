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

// What the app tells a surface's client, and the client keeping a pane.
func lines(_ name: String, _ got: [String], _ want: [String]) {
    if got == want { print("ok   \(name)") } else {
        failures += 1
        print("FAIL \(name): got \(got), wanted \(want)")
    }
}
lines("a client follows its surface into a mirror as long as its history",
      KeeperProtocol.clientOptions(hibernating: true, scrollback: 10_000_000, grapheme: true, blink: true, pid: 42, take: nil),
      ["mirror=10000000", "grapheme=1", "blink=1", "app=42"])
lines("one that may take over a kept pane is told whose",
      KeeperProtocol.clientOptions(hibernating: true, scrollback: 5, grapheme: false, blink: false, pid: 7, take: "/tmp/keep-attach/X"),
      ["mirror=5", "grapheme=0", "blink=0", "app=7", "take=/tmp/keep-attach/X"])
lines("with hibernation off, nothing: no mirror, nothing to take",
      KeeperProtocol.clientOptions(hibernating: false, scrollback: 10_000_000, grapheme: true, blink: true, pid: 42, take: "/tmp/x"),
      [])
lines("a take that is not a path is not passed on",
      KeeperProtocol.clientOptions(hibernating: true, scrollback: 1, grapheme: true, blink: true, pid: 1, take: "relative"),
      ["mirror=1", "grapheme=1", "blink=1", "app=1"])
lines("the surface's cursor before any program says, as the config names it",
      KeeperProtocol.clientOptions(hibernating: true, scrollback: 1, grapheme: true, blink: true, cursor: "bar", pid: 1, take: nil),
      ["mirror=1", "grapheme=1", "blink=1", "app=1", "cursor=bar"])
lines("a cursor word it does not know is not passed on",
      KeeperProtocol.clientOptions(hibernating: true, scrollback: 1, grapheme: true, blink: true, cursor: "sideways", pid: 1, take: nil),
      ["mirror=1", "grapheme=1", "blink=1", "app=1"])
let palette: [[UInt8]] = (0..<256).map { [UInt8($0), 0, 0] }
let facts = KeeperProtocol.facts(
    foreground: [0xff, 0xff, 0xff], background: [0x28, 0x2c, 0x34], cursor: nil, palette: palette,
    dark: false, cell: (16, 34), version: "1.3.2-HEAD-+a53771a", device: "62;22;52/1;10;0")
lines("the facts a kept pane answers with",
      facts.filter { !$0.hasPrefix("palette=") },
      ["foreground=ffffff", "background=282c34", "cursor=ffffff", "scheme=light", "cell=16,34",
       "xtversion=ghostty 1.3.2-HEAD-+a53771a", "da=62;22;52/1;10;0", "clipboard-read=ask"])
lines("the palette, all 256, in order",
      facts.filter { $0.hasPrefix("palette=") }.map { String($0.prefix(29)) },
      ["palette=000000,010000,020000,"])
lines("a cursor with a colour of its own keeps it; dark is said",
      KeeperProtocol.facts(foreground: [1, 2, 3], background: nil, cursor: [9, 9, 9], palette: [],
                           dark: true, cell: nil, version: nil, device: "d"),
      ["foreground=010203", "cursor=090909", "scheme=dark", "da=d", "clipboard-read=ask"])
lines("clipboard-read as the config says it",
      KeeperProtocol.facts(foreground: nil, background: nil, cursor: nil, palette: [],
                           dark: true, cell: nil, version: nil, device: "d", clipboardRead: "deny"),
      ["scheme=dark", "da=d", "clipboard-read=deny"])
lines("a palette short of a colour is no palette: the client keeps its own",
      KeeperProtocol.facts(foreground: nil, background: nil, cursor: nil, palette: Array(palette.dropLast()),
                           dark: false, cell: nil, version: nil, device: "d").filter { $0.hasPrefix("palette=") },
      [])
func colour(_ name: String, _ text: String, _ want: [UInt8]?) {
    let got = KeeperProtocol.colour(text)
    if got == want { print("ok   \(name)") } else {
        failures += 1
        print("FAIL \(name): got \(String(describing: got)), wanted \(String(describing: want))")
    }
}
colour("a background the client wrote", "010203\n", [1, 2, 3])
colour("not a colour", "01020", nil)

// What is swept at launch, and what is put on the pasteboard.
func yes(_ name: String, _ got: Bool, _ want: Bool) {
    if got == want { print("ok   \(name)") } else {
        failures += 1
        print("FAIL \(name): got \(got), wanted \(want)")
    }
}
// The app file beside a client: the app's pid, a line.
yes("a directory left by an app that is gone is swept",
    KeeperProtocol.isOrphan(appFile: "4242\n", me: 1, alive: { _ in false }), true)
yes("not one whose app runs", KeeperProtocol.isOrphan(appFile: "4242\n", me: 1, alive: { $0 == 4242 }), false)
yes("nor this app's own", KeeperProtocol.isOrphan(appFile: "4242\n", me: 4242, alive: { _ in false }), false)
yes("nor one whose file names nothing",
    KeeperProtocol.isOrphan(appFile: "", me: 1, alive: { _ in false }), false)
yes("nor one whose file says something else",
    KeeperProtocol.isOrphan(appFile: "mirror=1\n", me: 1, alive: { _ in false }), false)
yes("a program's clipboard write goes to the pasteboard", KeeperProtocol.relaysClipboardWrite(policy: "allow"), true)
yes("as with a surface asked to ask", KeeperProtocol.relaysClipboardWrite(policy: "ask"), true)
yes("and not when clipboard-write says deny", KeeperProtocol.relaysClipboardWrite(policy: "deny"), false)
yes("a program reading the clipboard is answered where clipboard-read allows",
    KeeperProtocol.clipboardRead(policy: "allow") == .answer, true)
yes("refused where it denies", KeeperProtocol.clipboardRead(policy: "deny") == .refuse, true)
yes("and the person asked where it asks, or says nothing (libghostty's default)",
    KeeperProtocol.clipboardRead(policy: "ask") == .ask && KeeperProtocol.clipboardRead(policy: nil) == .ask, true)

// A surface woken out of sight takes its program's grid (`takeSizeOutOfSight`):
// the size it has, moved by whole cells. What libghostty makes of a size —
// what is left past the padding, divided by the cell, rounded down — is the
// grid asked for, from any size it started at.
func fitted(_ name: String, _ got: (width: Double, height: Double)?, _ want: (width: Double, height: Double)?) {
    let same = got.map { [$0.width, $0.height] } == want.map { [$0.width, $0.height] }
    if same { print("ok   \(name)") } else {
        failures += 1
        print("FAIL \(name): got \(String(describing: got)), wanted \(String(describing: want))")
    }
}
fitted("the program's grid already: nothing to move",
       KeeperProtocol.pixels((1698, 1296), grid: (105, 38), cell: (16, 34), program: (105, 38)), nil)
fitted("a smaller program's grid: whole cells fewer, both ways",
       KeeperProtocol.pixels((1698, 1296), grid: (105, 38), cell: (16, 34), program: (80, 24)), (1298, 820))
fitted("a larger one: whole cells more",
       KeeperProtocol.pixels((1698, 1296), grid: (105, 38), cell: (16, 34), program: (144, 48)), (2322, 1636))
fitted("only the rows off: the columns left alone",
       KeeperProtocol.pixels((1698, 1296), grid: (105, 38), cell: (16, 34), program: (105, 24)), (1698, 820))
fitted("no grid from the daemon: nothing",
       KeeperProtocol.pixels((1698, 1296), grid: (105, 38), cell: (16, 34), program: (0, 0)), nil)
fitted("no cell yet: nothing",
       KeeperProtocol.pixels((1698, 1296), grid: (105, 38), cell: (0, 0), program: (80, 24)), nil)
var lands = true
for padding in [0.0, 4.0, 11.0] {
    for start in stride(from: 900.0, through: 2400.0, by: 37.0) {
        let grid = { (size: Double, cell: Double) in Int(((size - padding) / cell).rounded(.down)) }
        for program in [(80, 24), (105, 38), (144, 48), (200, 60)] {
            let now = (cols: grid(start, 16.5), rows: grid(start * 0.7, 34))
            guard let fit = KeeperProtocol.pixels(
                (start, start * 0.7), grid: now, cell: (16.5, 34), program: program)
            else { if now != program { lands = false }; continue }
            if grid(fit.width, 16.5) != program.0 || grid(fit.height, 34) != program.1 { lands = false }
        }
    }
}
check("the grid libghostty makes of the size given is the program's, whatever the padding", [lands ? 1 : 0], [1])

exit(failures == 0 ? 0 : 1)
