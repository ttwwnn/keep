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
            device: deviceAttributes)
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

    init?(directory: URL, onTitle: @escaping (String) -> Void, onDirectory: @escaping (String) -> Void) {
        let fd = open(directory.path, O_EVTONLY)
        guard fd >= 0 else { return nil }
        self.directory = directory
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
        for name in [Keeper.title, Keeper.directory, Keeper.clipboard, Keeper.background] {
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
                // As the surface's runtime writes it (write_clipboard_cb).
                guard !text.isEmpty else { continue }
                let board = NSPasteboard.general
                board.clearContents()
                board.setString(text, forType: .string)
                Trace.log("keeper", "\(directory.lastPathComponent) clipboard: \(body.count) bytes")
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
}
