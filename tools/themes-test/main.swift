// Keep's own theme (apps/macos/Sources/Keep/Model/TerminalPrefs.swift,
// `KeepThemes`), built with that file and nothing else: the two files written
// where they are missing and never over one of the same name, what they say,
// and the default line only when both halves are there. In a folder of its
// own: the person's themes are never touched.
import Foundation

// What TerminalPrefs.swift asks of the rest of the app (Model/SidebarStateStore.swift).
func stateDirectory() -> URL {
    URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("keep-themes-test-state")
}

var failures = 0
func check(_ what: String, _ got: String, _ wanted: String) {
    if got == wanted {
        print("  ok    \(what)")
    } else {
        print("  FAIL  \(what): wanted \(wanted), got \(got)")
        failures += 1
    }
}
func read(_ path: String) -> String {
    (try? String(contentsOfFile: path, encoding: .utf8)) ?? "(missing)"
}

let work = CommandLine.arguments[1]
let themes = (work as NSString).appendingPathComponent("themes")
let dark = (themes as NSString).appendingPathComponent("Keep Escuro")
let light = (themes as NSString).appendingPathComponent("Keep Claro")

print("an empty folder: no default until the files are there")
check("no default line", KeepThemes.defaultLine(in: themes) ?? "nil", "nil")
KeepThemes.install(into: themes)
check("the default line", KeepThemes.defaultLine(in: themes) ?? "nil", "theme = dark:Keep Escuro,light:Keep Claro")

print("what the two say: Ghostty's dark, and a white ground with GitHub Light's palette")
let darkText = read(dark)
check("dark ground and ink", darkText.contains("\nbackground = #282c34\nforeground = #ffffff\n") ? "yes" : darkText, "yes")
let lightText = read(light)
check("light ground and ink", lightText.contains("\nbackground = #ffffff\nforeground = #1f2328\n") ? "yes" : lightText, "yes")
check("light palette slots", String(lightText.components(separatedBy: "\npalette = ").count - 1), "16")
check("light ends in one newline", lightText.hasSuffix("#8c959f\n") && !lightText.hasSuffix("\n\n") ? "yes" : "no", "yes")

print("a file of the same name is the person's: never written over")
try? "background = #123456\n".write(toFile: light, atomically: true, encoding: .utf8)
try? FileManager.default.removeItem(atPath: dark)
KeepThemes.install(into: themes)
check("theirs stays", read(light), "background = #123456\n")
check("the missing one comes back", read(dark) == darkText ? "yes" : "no", "yes")

print("one half gone: no default, Ghostty's own stands")
try? FileManager.default.removeItem(atPath: dark)
check("no default line", KeepThemes.defaultLine(in: themes) ?? "nil", "nil")

exit(failures == 0 ? 0 : 1)
