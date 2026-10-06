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
_ = real
exit(failures == 0 ? 0 : 1)
