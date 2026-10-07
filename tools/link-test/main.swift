import Foundation

enum Trace { static func log(_ a: String, _ b: String) {} }

var failures = 0
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
_ = real
exit(failures == 0 ? 0 : 1)
