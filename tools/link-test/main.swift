import Foundation

enum Trace { static func log(_ a: String, _ b: String) {} }

var failures = 0
let log = (NSTemporaryDirectory() as NSString).appendingPathComponent("keep-link-log-\(getpid())")
setenv("KEEP_LINK_LOG", log, 1)
defer { try? FileManager.default.removeItem(atPath: log) }
func check(_ name: String, _ got: String?, _ want: String?) {
    if got == want { print("ok   \(name)") } else {
        failures += 1
        print("FAIL \(name): got \(got ?? "nil"), wanted \(want ?? "nil")")
    }
}

let root = (NSTemporaryDirectory() as NSString).appendingPathComponent("keep-link-\(getpid())")
try! FileManager.default.createDirectory(atPath: root + "/sub dir", withIntermediateDirectories: true)
defer { try? FileManager.default.removeItem(atPath: root) }
FileManager.default.createFile(atPath: root + "/a.txt", contents: Data())
FileManager.default.createFile(atPath: root + "/sub dir/b.md", contents: Data())
FileManager.default.createFile(atPath: root + "/odd:7", contents: Data())
let real = (root as NSString).resolvingSymlinksInPath

func path(_ text: String, _ dir: String?) -> String? {
    LinkOpener.resolve(text, from: dir)?.path
}

check("absolute", path(root + "/a.txt", nil), root + "/a.txt")
check("relative to the shell's directory", path("a.txt", root), root + "/a.txt")
check("relative without a directory", path("a.txt", nil), nil)
check("dot-dot", path("../a.txt", root + "/sub dir"), root + "/a.txt")
check("a space in the name", path("sub dir/b.md", root), root + "/sub dir/b.md")
check("line suffix", path("a.txt:42", root), root + "/a.txt")
check("line and column suffix", path("a.txt:42:7", root), root + "/a.txt")
check("a name that really ends in :7", path("odd:7", root), root + "/odd:7")
check("a folder", path(root, nil), root)
check("missing file", path("nope.txt:3", root), nil)
check("home", path("~", nil), NSHomeDirectory())
// What the runtime found is part of a path; the screen holds the rest.
let deep = root + "/projetos/clinica-integrativa/app/Filament/Resources/PacienteResource/Pages"
try! FileManager.default.createDirectory(atPath: deep, withIntermediateDirectories: true)
FileManager.default.createFile(atPath: deep + "/EditPaciente.php", contents: Data())
try! FileManager.default.createDirectory(atPath: root + "/Área de trabalho", withIntermediateDirectories: true)
let shot = root + "/Área de trabalho/Captura de Tela 2026-10-07 às 10.00.png"
FileManager.default.createFile(atPath: shot, contents: Data())

func recovered(_ text: String, _ lines: [String], _ row: Int? = nil, _ dir: String? = nil) -> String? {
    LinkOpener.recover(text, in: lines, near: row, from: dir)?.path
}

// As Claude Code prints a path wider than the screen: the rest on the next
// row, indented under the first.
let full = deep + "/EditPaciente.php"
let cut = full.index(full.endIndex, offsetBy: -9)
let head = String(full[..<cut]), tail = String(full[cut...]) + ":42"
let wrapped = ["⏺ Abri o arquivo:", "", "  " + head, "  " + tail, "", "  Mais texto."]
check("a path broken across two rows, clicked on the first", recovered(head, wrapped, 2), full)
check("the same, clicked on the second", recovered(tail, wrapped, 3), full)
check(
    "a path over three rows",
    recovered(
        String(full.dropFirst(20).prefix(20)),
        ["  " + String(full.prefix(20)), "  " + String(full.dropFirst(20).prefix(20)),
         "  " + String(full.dropFirst(40)) + "."]),
    full)
check(
    "a path that ends its row, the next row another sentence",
    recovered(root + "/a.txt", ["  veja " + root + "/a.txt", "  e depois"]), root + "/a.txt")

// As a file dropped into the tab arrives: spaces escaped.
let escaped = shot.replacingOccurrences(of: " ", with: "\\ ")
let cutAtBackslash = String(escaped[..<escaped.firstIndex(of: "\\")!])
check("a dropped file, spaces escaped", recovered(cutAtBackslash, ["❯ " + escaped + " "], 0), shot)
check(
    "a dropped file, clicked past the first escaped space",
    recovered("trabalho/Captura", ["❯ olha " + escaped]), shot)
check("inside backticks, a full stop after", recovered(root + "/a.txt", ["Gravei em `" + root + "/a.txt`."]), root + "/a.txt")
check("relative, with the gutter", recovered("sub\\ dir/b.md:3", ["  ⎿  sub\\ dir/b.md:3"], nil, root), root + "/sub dir/b.md")
check("nothing on screen", recovered(head, []), nil)
check("nothing there to piece together", recovered("nope/x.txt", ["  nope/x.txt", "  y"], nil, root), nil)

// Web addresses: a word read off the screen, and one Claude Code broke
// across rows (it fills a row to two columns short of the edge and goes on,
// indented, on the next).
func address(_ word: String) -> String? { LinkOpener.webAddress(in: word)?.absoluteString }
check("an address, the full stop after it left off", address("https://example.com/a."), "https://example.com/a")
check("inside a markdown link", address("[docs](https://example.com/x)"), "https://example.com/x")
check("brackets that belong to it", address("https://en.wikipedia.org/wiki/Foo_(bar)"), "https://en.wikipedia.org/wiki/Foo_(bar)")
check("in quotes, a comma after", address("\"https://example.com/?q=1&b=2\","), "https://example.com/?q=1&b=2")
check("www without a scheme", address("www.example.com.br/x"), "https://www.example.com.br/x")
check("mail", address("mailto:alguem@example.com"), "mailto:alguem@example.com")
check("a word that ends in a colon is no address", address("Nota:"), nil)
check("a scheme with nothing after", address("https://"), nil)
check("a host without a dot", address("http://foo"), nil)

let columns = 60
let link = "https://github.com/ttwwnn/keep/blob/main/apps/macos/Sources/Keep/Model/LinkOpener.swift?plain=1&utm_source=keep&utm_medium=terminal&utm_campaign=links#L120"
let lead = "  o link é "
let first = String(link.prefix(columns - 2 - lead.count))
let second = String(link.dropFirst(first.count).prefix(columns - 4))
let third = String(link.dropFirst(first.count + second.count))
let broken = ["⏺ Pronto.", "", lead + first, "  " + second, "  " + third + " e também", "", "  Fim."]
func around(_ text: String, _ lines: [String], _ row: Int? = nil) -> String? {
    LinkOpener.webAddress(around: text, in: lines, near: row, columns: columns)?.absoluteString
}
check("an address over three rows, clicked on the first", around(first, broken, 2), link)
check("the same, clicked on the middle row", around(second, broken, 3), link)
check("the same, clicked on the last", around(third, broken, 4), link)
check(
    "an address that ends a row not full keeps clear of the next",
    around("https://example.com/a", ["  veja https://example.com/a", "  Isso aqui"]),
    "https://example.com/a")
check(
    "a full row before it, the address alone",
    around("https://example.com/a", [String(repeating: "x", count: 58), "https://example.com/a"]),
    "https://example.com/a")
check(
    "an address that ends a column short of full keeps the next row's word out",
    around("https://example.com/a", ["  veja " + String(repeating: "x", count: columns - 32) + " https://example.com/a", "  e também"]),
    "https://example.com/a")
check("a word with no address near", around("Fim.", broken, 6), nil)
check("without the width nothing is joined", LinkOpener.webAddress(around: second, in: broken, near: 3, columns: 0)?.absoluteString, nil)

// What a click opens: the runtime's piece of the address made whole, unless
// the program put the address behind its text and it arrived whole.
func opened(_ body: () -> Bool) -> String? {
    try? "".write(toFile: log, atomically: false, encoding: .utf8)
    guard body() else { return nil }
    return (try? String(contentsOfFile: log, encoding: .utf8))?.trimmingCharacters(in: .newlines)
}
check("the runtime's piece, made whole", opened { LinkOpener.open(first, from: nil, screen: broken, near: 2, columns: columns) }, link)
check("an address that arrived whole, though the screen goes on past it", opened { LinkOpener.open(first, from: nil, screen: broken, near: 2, columns: columns, whole: true) }, first)
check("a word the runtime missed, under a plain click", opened { LinkOpener.openRecoveredAddress(third, screen: broken, near: 4, columns: columns) }, link)
check("a word with no address under a plain click", opened { LinkOpener.openRecoveredAddress("também", screen: broken, near: 4, columns: columns) }, nil)
func plain(_ text: String, _ programHasMouse: Bool = false) -> String {
    String(LinkOpener.plainClickMayOpen(text, programHasMouse: programHasMouse))
}
check("a plain click opens a web address", plain("https://example.com"), "true")
check("a plain click opens a path", plain("/tmp/a.txt:3"), "true")
check("a plain click opens a file address", plain("file:///tmp/a.txt"), "true")
check("a plain click does not start an app by its address", plain("vscode://file/tmp/a"), "false")
check("an address is the program's when it has the mouse", plain("https://example.com", true), "false")
check("a path is still opened when the program has the mouse", plain("/tmp/a.txt:3", true), "true")

// The `[Image #N]` of a pasted picture: the file behind it, when it is one.
FileManager.default.createFile(atPath: root + "/1.png", contents: Data())
let png = URL(fileURLWithPath: root + "/1.png").absoluteString
check("a picture behind a link", LinkOpener.image(behind: png)?.path, root + "/1.png")
check("not a picture", LinkOpener.image(behind: URL(fileURLWithPath: root + "/a.txt").absoluteString)?.path, nil)
check("a picture no longer there", LinkOpener.image(behind: URL(fileURLWithPath: root + "/2.png").absoluteString)?.path, nil)
check("a web address", LinkOpener.image(behind: "https://example.com/1.png")?.path, nil)
_ = real
exit(failures == 0 ? 0 : 1)
