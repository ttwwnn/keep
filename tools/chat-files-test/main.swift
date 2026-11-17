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
        ["type": "tool_use", "name": "Bash", "input": ["command": "plot \(onlyRead) > \(madeByBash) && cat /etc/hosts"]],
        ["type": "tool_use", "name": "SendUserFile", "input": ["files": [handed]]],
        ["type": "text", "text": "Gravei **\(madeByBash)**: pronto."],
    ]]]),
]
let transcript = root + "/t.jsonl"
try! (lines.joined(separator: "\n") + "\n").write(toFile: transcript, atomically: true, encoding: .utf8)

let images = root + "/imagens"
let reader = ChatFilesReader(transcript: transcript, session: "sessao", imageFolder: images)
var files = reader.read()
let image = images + "/sessao/Imagem 3.png"
check("sent: the picture, then what was typed and dropped, newest first", files.sent.map(\.path), [image, typedPath, dropped])
check("made: written, handed over, and created by a command, newest first", files.made.map(\.path), [handed, madeByBash, written])
check("the picture is written out, for its owner alone",
      [String(format: "%o", (try? fm.attributesOfItem(atPath: image))?[.posixPermissions] as? Int ?? 0)], ["600"])

// The transcript grows: only the new line is read, and it counts.
let later = file("depois.txt")
let more = line(["type": "assistant", "timestamp": start, "message": ["role": "assistant", "content": [
    ["type": "tool_use", "name": "Write", "input": ["file_path": later, "content": "x"]]]]])
let handle = FileHandle(forWritingAtPath: transcript)!
handle.seekToEndOfFile()
handle.write(Data(more.utf8))  // no newline yet: still being written
files = reader.read()
check("a line still being written waits", files.made.map(\.path), [handed, madeByBash, written])
handle.write(Data("\n".utf8))
try! handle.close()
files = reader.read()
check("and is read once whole", files.made.map(\.path), [later, handed, madeByBash, written])

// A file gone from disk leaves the list.
try! fm.removeItem(atPath: written)
check("a file deleted is no longer listed", reader.read().made.map(\.path), [later, handed, madeByBash])

// Two pictures in one message, without paste ids: two files, not one twice.
let two = root + "/dois.jsonl"
try! (line(["type": "user", "timestamp": start, "uuid": "abcdef0123",
            "message": ["role": "user", "content": [
                ["type": "image", "source": ["type": "base64", "media_type": "image/jpeg", "data": png]],
                ["type": "image", "source": ["type": "base64", "media_type": "image/jpeg", "data": png]]]]]) + "\n")
    .write(toFile: two, atomically: true, encoding: .utf8)
check("two pictures without ids, each its own file",
      ChatFilesReader(transcript: two, session: "dois", imageFolder: images).read().sent.map(\.lastPathComponent),
      ["Imagem abcdef01-1.jpg", "Imagem abcdef01-0.jpg"])

check("paths in text", ChatFilesReader.paths(in: "veja file:///tmp/a.txt, ~/b.md:12 e \"/x/c d.pdf\"."),
      ["/tmp/a.txt", NSHomeDirectory() + "/b.md", "/x/c d.pdf"])
exit(failures == 0 ? 0 : 1)
