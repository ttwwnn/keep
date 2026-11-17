import Foundation

var failures = 0
func check(_ name: String, _ got: [String], _ want: [String]) {
    if got == want { print("ok   \(name)") } else {
        failures += 1
        print("FAIL \(name):\n       got    \(got)\n       wanted \(want)")
    }
}

let root = (NSTemporaryDirectory() as NSString).appendingPathComponent("keep-chat-files-\(getpid())")
try! FileManager.default.createDirectory(atPath: root + "/dir com espaço", withIntermediateDirectories: true)
defer { try? FileManager.default.removeItem(atPath: root) }
let fm = FileManager.default
let old = Date(timeIntervalSince1970: 1_700_000_000)
func file(_ name: String, created: Date? = nil) -> String {
    let path = root + "/" + name
    fm.createFile(atPath: path, contents: Data("x".utf8))
    if let created { try! fm.setAttributes([.creationDate: created, .modificationDate: created], ofItemAtPath: path) }
    return path
}
let dropped = file("dir com espaço/contrato.pdf", created: old)
let typedPath = file("notas.md", created: old)
let written = file("relatorio.html", created: old)
let step = file("rascunho.html")
let scratch = file("extrai.sh")
let handed = file("resumo.pdf")
let madeByBash = file("grafico.png")
let onlyRead = file("entrada.csv", created: old)
let inSummary = file("do-resumo.txt", created: old)
let fromOtherTab = file("de-outra-aba.txt", created: old)
let png = Data([0x89, 0x50, 0x4E, 0x47]).base64EncodedString()

func line(_ json: [String: Any]) -> String {
    String(data: try! JSONSerialization.data(withJSONObject: json), encoding: .utf8)!
}
let start = ISO8601DateFormatter().string(from: Date().addingTimeInterval(-60))
var lines = [
    line(["type": "user", "timestamp": start, "origin": ["kind": "human"],
          "message": ["role": "user", "content": "olha \(dropped.replacingOccurrences(of: " ", with: "\\ ")) e `\(typedPath)`"]]),
    line(["type": "user", "timestamp": start, "imagePasteIds": [3], "uuid": "u1",
          "message": ["role": "user", "content": [["type": "image", "source": ["type": "base64", "media_type": "image/png", "data": png]],
                                                  ["type": "text", "text": "[Image #3] veja"]]]]),
    line(["type": "user", "timestamp": start, "isCompactSummary": true,
          "message": ["role": "user", "content": "Resumo: \(inSummary)"]]),
    line(["type": "user", "timestamp": start, "origin": ["kind": "task-notification"],
          "message": ["role": "user", "content": "de \(fromOtherTab)"]]),
    line(["type": "user", "timestamp": start,
          "message": ["role": "user", "content": "<task-notification>\(fromOtherTab)</task-notification>"]]),
    line(["type": "assistant", "timestamp": start, "message": ["role": "assistant", "content": [
        ["type": "tool_use", "name": "Write", "input": ["file_path": written, "content": "x"]],
        ["type": "tool_use", "name": "Write", "input": ["file_path": step, "content": "x"]],
        ["type": "tool_use", "name": "Write", "input": ["file_path": scratch, "content": "x"]],
        ["type": "tool_use", "name": "Bash", "input": ["command": "sh \(scratch); print \(step) > \(handed); plot \(onlyRead) > \(madeByBash) && cat /etc/hosts"]],
        ["type": "tool_use", "name": "SendUserFile", "input": ["files": [handed]]],
        ["type": "text", "text": "Gravei **\(madeByBash)** e o relatorio.html, a partir de entrada.csv: pronto."],
    ]]]),
]
let transcript = root + "/t.jsonl"
try! (lines.joined(separator: "\n") + "\n").write(toFile: transcript, atomically: true, encoding: .utf8)

let images = root + "/imagens"
let reader = ChatFilesReader(transcript: transcript, session: "sessao", imageFolder: images)
var files = reader.read()
let image = images + "/sessao/Imagem 3.png"
check("sent: the picture, then what was typed and dropped, newest first", files.sent.map(\.url.path), [image, typedPath, dropped])
check("made: handed over, and what an answer names, newest first; not the steps on the way",
      files.made.map(\.url.path), [madeByBash, handed, written])
check("the picture is written out, for its owner alone",
      [String(format: "%o", (try? fm.attributesOfItem(atPath: image))?[.posixPermissions] as? Int ?? 0)], ["600"])

// The transcript grows: only the new line is read, and it counts.
let later = file("depois.txt")
let more = line(["type": "assistant", "timestamp": start, "message": ["role": "assistant", "content": [
    ["type": "tool_use", "name": "Write", "input": ["file_path": later, "content": "x"]],
    ["type": "text", "text": "Também gravei depois.txt."]]]])
let handle = FileHandle(forWritingAtPath: transcript)!
handle.seekToEndOfFile()
handle.write(Data(more.utf8))  // no newline yet: still being written
files = reader.read()
check("a line still being written waits", files.made.map(\.url.path), [madeByBash, handed, written])
handle.write(Data("\n".utf8))
try! handle.close()
files = reader.read()
check("and is read once whole", files.made.map(\.url.path), [later, madeByBash, handed, written])

// A file gone from disk leaves the list.
try! fm.removeItem(atPath: written)
check("a file deleted is no longer listed", reader.read().made.map(\.url.path), [later, madeByBash, handed])

// Two pictures in one message, without paste ids: two files, not one twice.
let two = root + "/dois.jsonl"
try! (line(["type": "user", "timestamp": start, "uuid": "abcdef0123",
            "message": ["role": "user", "content": [
                ["type": "image", "source": ["type": "base64", "media_type": "image/jpeg", "data": png]],
                ["type": "image", "source": ["type": "base64", "media_type": "image/jpeg", "data": png]]]]]) + "\n")
    .write(toFile: two, atomically: true, encoding: .utf8)
check("two pictures without ids, each its own file",
      ChatFilesReader(transcript: two, session: "dois", imageFolder: images).read().sent.map(\.url.lastPathComponent),
      ["Imagem abcdef01-1.jpg", "Imagem abcdef01-0.jpg"])

// A bare name in an answer, two files by that name: neither is taken on the name alone.
try! fm.createDirectory(atPath: root + "/a", withIntermediateDirectories: true)
try! fm.createDirectory(atPath: root + "/b", withIntermediateDirectories: true)
let twins = [file("a/index.html"), file("b/index.html")]
let twinsLog = root + "/twins.jsonl"
try! (line(["type": "assistant", "timestamp": start, "message": ["role": "assistant", "content":
    twins.map { ["type": "tool_use", "name": "Write", "input": ["file_path": $0, "content": "x"]] }
        + [["type": "text", "text": "Veja o index.html."]]]]) + "\n").write(toFile: twinsLog, atomically: true, encoding: .utf8)
check("a bare name two files share names neither",
      ChatFilesReader(transcript: twinsLog, session: "gemeos", imageFolder: images).read().made.map(\.url.path), [])

// A copy a command made from a file it never wrote does not count against the one it wrote.
try! fm.createDirectory(atPath: root + "/modelo", withIntermediateDirectories: true)
let template = file("modelo/pagina.html", created: old)
let page = file("pagina.html")
let copied = root + "/copiado.jsonl"
try! (line(["type": "assistant", "timestamp": start, "message": ["role": "assistant", "content": [
    ["type": "tool_use", "name": "Bash", "input": ["command": "cp \(template) /tmp/nada"]],
    ["type": "tool_use", "name": "Write", "input": ["file_path": page, "content": "x"]],
    ["type": "text", "text": "Veja a pagina.html."]]]]) + "\n").write(toFile: copied, atomically: true, encoding: .utf8)
check("a namesake only read does not hide the one it wrote",
      ChatFilesReader(transcript: copied, session: "copia", imageFolder: images).read().made.map(\.url.path), [page])

// Pictures of a conversation untouched for 30 days go; newer ones stay.
let cache = root + "/cache"
for (name, age) in [("velha", 31.0), ("nova", 2.0)] {
    try! fm.createDirectory(atPath: cache + "/" + name, withIntermediateDirectories: true)
    try! fm.setAttributes([.modificationDate: Date().addingTimeInterval(-age * 86_400)], ofItemAtPath: cache + "/" + name)
}
ChatFilesReader.pruneOnce(cache)
check("a conversation's pictures left for 30 days are gone", ((try? fm.contentsOfDirectory(atPath: cache)) ?? []).sorted(), ["nova"])

// The Downloads folder: its files and folders, the hidden ones left out.
let downloads = root + "/baixados-teste"
try! fm.createDirectory(atPath: downloads + "/pasta", withIntermediateDirectories: true)
fm.createFile(atPath: downloads + "/nota.pdf", contents: Data())
fm.createFile(atPath: downloads + "/.DS_Store", contents: Data())
check("the Downloads folder, hidden files left out",
      ChatFilesReader.downloads(in: URL(fileURLWithPath: downloads)).map(\.url.lastPathComponent).sorted(), ["nota.pdf", "pasta"])

check("paths in text", ChatFilesReader.paths(in: "veja file:///tmp/a.txt, ~/b.md:12 e \"/x/c d.pdf\"."),
      ["/tmp/a.txt", NSHomeDirectory() + "/b.md", "/x/c d.pdf"])
exit(failures == 0 ? 0 : 1)
