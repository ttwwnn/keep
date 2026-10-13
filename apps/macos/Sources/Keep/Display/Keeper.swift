import AppKit
import GhosttyKit

/// A pane whose surface was let go of, kept by the `keep` client that was
/// drawing into it (crates/keep/src/keeper.rs).
///
/// Every client follows what it writes to its surface into a terminal of its
/// own, keeping as much history as the surface does (`mirror=` on the attach
/// file, `TerminalSurfaceView.clientOptions`). When `Hibernation` lets go of a
/// surface it first writes, beside the client, what the surface would answer
/// a program that asked about it (`facts`) and that it means to come back
/// (`hibernate`). The client then stays attached to the daemon without a
/// surface: the tab's output goes on into its mirror, the program's questions
/// are answered, and what the program says about itself is left in files
/// this watches (`KeeperWatch`). The surface made for the pane next names
/// that directory (`take=`), and the client there is handed the whole
/// terminal — both screens, every row of history, the title, the modes — and
/// the very connection to the daemon, so that nothing the tab wrote in
/// between is missed or written twice.
enum Keeper {
    /// The file names the client and the app share; see keeper.rs.
    static let flag = "hibernate"
    static let facts = "facts"
    static let title = "title"
    static let directory = "pwd"
    static let clipboard = "clipboard"
    static let background = "background"
    static let kept = "kept"
    static let unkeepable = "unkeepable"
    static let took = "took"
    static let clipboardRequest = "clipboard-request"
    static let clipboardAnswer = "clipboard-answer"
    static let wake = "wake"

    /// What a surface answers that libghostty-vt would answer otherwise:
    /// CSI c and CSI > c as Ghostty answers them (it leaves CSI = c
    /// unanswered), measured and pinned by tools/hibernate-test.sh, which
    /// asks a surface and a kept pane the same questions.
    static let deviceAttributes = "62;22;52/1;10;0"

    /// Write what a surface would answer about itself, for its client to
    /// answer with once it has no surface: the theme's colours, light or
    /// dark, the size of a cell in pixels, the terminal's name and version.
    static func writeFacts(to directory: URL, cell: (width: UInt32, height: UInt32)?, dark: Bool) {
        let theme = GhosttyApp.shared.themeColours
        let lines = KeeperProtocol.facts(
            foreground: theme.foreground, background: theme.background, cursor: theme.cursor,
            palette: theme.palette, dark: dark, cell: cell, version: GhosttyApp.version,
            device: deviceAttributes, clipboardRead: GhosttyApp.shared.clipboardReadPolicy)
        try? (lines.joined(separator: "\n") + "\n").write(
            to: directory.appendingPathComponent(facts), atomically: true, encoding: .utf8)
    }

    /// Say the app will not come back for a kept pane: its client sees the
    /// flag gone within a quarter of a second, lets go of the tab and takes
    /// its directory with it. What is still there a while later — a client
    /// that never kept — goes too.
    static func release(_ directory: URL, sweepAfter delay: TimeInterval = 5) {
        try? FileManager.default.removeItem(at: directory.appendingPathComponent(flag))
        sweep(directory, after: delay)
    }

    /// The directories left behind by a Keep that is gone — a client that
    /// died keeping, or never started — whose attach file names an app no
    /// longer running. Their keepers, if any, are going by themselves.
    /// Those of a Keep still running, this one or another, are left alone.
    static func sweepOrphans() {
        let base = FileManager.default.temporaryDirectory.appendingPathComponent("keep-attach", isDirectory: true)
        DispatchQueue.global(qos: .utility).async {
            let me = getpid()
            let alive: (Int32) -> Bool = { pid in kill(pid, 0) == 0 || errno == EPERM }
            let names = (try? FileManager.default.contentsOfDirectory(atPath: base.path)) ?? []
            var swept = 0
            for name in names {
                let directory = base.appendingPathComponent(name, isDirectory: true)
                guard let attach = try? String(
                    contentsOf: directory.appendingPathComponent(".keep-attach"), encoding: .utf8),
                    KeeperProtocol.isOrphan(attachFile: attach, me: me, alive: alive)
                else { continue }
                if (try? FileManager.default.removeItem(at: directory)) != nil { swept += 1 }
            }
            if swept > 0 { Trace.log("keeper", "swept \(swept) left by a Keep that is gone") }
        }
    }

    /// A kept pane's directory, once whatever was to happen in it has: the
    /// keeper removes it itself when it hands over or lets go.
    static func sweep(_ directory: URL, after delay: TimeInterval) {
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) {
            let flagFile = directory.appendingPathComponent(flag)
            // Still kept: somebody is coming back for it after all.
            guard !FileManager.default.fileExists(atPath: flagFile.path) else { return }
            try? FileManager.default.removeItem(at: directory)
        }
    }
}

/// What a kept pane's program says about itself, carried to the app as the
/// surface would have carried it: the title to the strip, the directory to
/// where new panes open, the clipboard to the pasteboard, the background to
/// the window's chrome. Main thread only.
final class KeeperWatch {
    private let directory: URL
    private let source: DispatchSourceFileSystemObject
    private var seen: [String: Data] = [:]
    private let onTitle: (String) -> Void
    private let onDirectory: (String) -> Void
    /// The keeper asks for a surface (`Keeper.wake`): its program is drawing
    /// an image.
    private let onWake: () -> Void
    /// The window the pane was shown in, where a question about it is asked.
    private weak var window: NSWindow?

    init?(
        directory: URL, window: NSWindow? = nil, onTitle: @escaping (String) -> Void,
        onWake: @escaping () -> Void = {}, onDirectory: @escaping (String) -> Void
    ) {
        let fd = open(directory.path, O_EVTONLY)
        guard fd >= 0 else { return nil }
        self.directory = directory
        self.window = window
        self.onWake = onWake
        self.onTitle = onTitle
        self.onDirectory = onDirectory
        // Files are written whole, under another name and renamed into
        // place: each is a change to the directory.
        source = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: fd, eventMask: [.write, .rename, .link], queue: .main)
        source.setCancelHandler { close(fd) }
        source.setEventHandler { [weak self] in self?.look() }
        source.resume()
        // Whatever is there already, from before this watch.
        seen[Keeper.title] = read(Keeper.title)
        seen[Keeper.directory] = read(Keeper.directory)
        seen[Keeper.clipboard] = read(Keeper.clipboard)
        seen[Keeper.background] = read(Keeper.background)
        seen[Keeper.clipboardRequest] = read(Keeper.clipboardRequest)
        seen[Keeper.wake] = read(Keeper.wake)
    }

    deinit {
        source.cancel()
    }

    func stop() {
        source.cancel()
    }

    private func read(_ name: String) -> Data? {
        try? Data(contentsOf: directory.appendingPathComponent(name))
    }

    private func look() {
        for name in [
            Keeper.title, Keeper.directory, Keeper.clipboard, Keeper.background, Keeper.clipboardRequest, Keeper.wake,
        ] {
            guard let body = read(name), body != seen[name] else { continue }
            seen[name] = body
            let text = String(decoding: body, as: UTF8.self)
            switch name {
            case Keeper.title:
                Trace.log("keeper", "\(directory.lastPathComponent) title: \(text)")
                onTitle(text)
            case Keeper.directory:
                onDirectory(text)
            case Keeper.clipboard:
                // As the surface's runtime writes it (write_clipboard_cb),
                // and only where libghostty would have let it.
                guard !text.isEmpty else { continue }
                guard KeeperProtocol.relaysClipboardWrite(policy: GhosttyApp.shared.clipboardWritePolicy) else {
                    Trace.log("keeper", "\(directory.lastPathComponent) clipboard refused: clipboard-write = deny")
                    continue
                }
                let board = NSPasteboard.general
                board.clearContents()
                board.setString(text, forType: .string)
                Trace.log("keeper", "\(directory.lastPathComponent) clipboard: \(body.count) bytes")
            case Keeper.clipboardRequest:
                answerClipboard(request: text.split(separator: "\n").first.map(String.init) ?? "")
            case Keeper.wake:
                Trace.log("keeper", "\(directory.lastPathComponent) asks for a surface: \(text.trimmingCharacters(in: .whitespacesAndNewlines))")
                onWake()
            case Keeper.background:
                guard let rgb = KeeperProtocol.colour(text) else { continue }
                let color = NSColor(
                    srgbRed: CGFloat(rgb[0]) / 255, green: CGFloat(rgb[1]) / 255,
                    blue: CGFloat(rgb[2]) / 255, alpha: 1)
                // As GHOSTTY_ACTION_COLOR_CHANGE is answered for a surface.
                GhosttyApp.shared.adoptProgramBackground(color)
                Trace.log("keeper", "\(directory.lastPathComponent) background: \(text)")
            default:
                break
            }
        }
    }

    /// The kept program asked for the clipboard (OSC 52), and its client
    /// waits for the answer: the pasteboard, as a surface's runtime reads
    /// it (`read_clipboard_cb`) — after asking, where `clipboard-read` says
    /// to, with the question a surface asks (`confirmPaste`); nothing, where
    /// the person says no.
    private func answerClipboard(request id: String) {
        guard !id.isEmpty else { return }
        let text = NSPasteboard.general.string(forType: .string) ?? ""
        let directory = directory
        let reply: (String) -> Void = { answer in
            try? Data((id + "\n" + answer).utf8).write(
                to: directory.appendingPathComponent(Keeper.clipboardAnswer), options: .atomic)
            Trace.log("keeper", "\(directory.lastPathComponent) clipboard read \(id): \(answer.utf8.count) bytes")
        }
        switch KeeperProtocol.clipboardRead(policy: GhosttyApp.shared.clipboardReadPolicy) {
        case .answer:
            reply(text)
        case .refuse:
            // Its client knows not to ask; told nothing, should it ask anyway.
            reply("")
        case .ask:
            let alert = TerminalSurfaceView.pasteQuestion(text)
            // On the pane's own window, as a surface asks on its own.
            guard let window = window ?? NSApp.keyWindow ?? NSApp.mainWindow else {
                reply(alert.runModal() == .alertFirstButtonReturn ? text : "")
                return
            }
            alert.beginSheetModal(for: window) { response in
                reply(response == .alertFirstButtonReturn ? text : "")
            }
        }
    }
}
