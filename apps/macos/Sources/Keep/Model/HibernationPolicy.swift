import Foundation

/// Which hidden tabs let go of their surfaces.
///
/// A surface keeps three render targets of tens of megabytes for as long as
/// it exists, whether or not it is on screen — about fifty megabytes a tab,
/// measured, against two for the shell it shows. Mounting every tab ever
/// visited and merely hiding it (see `TabContentContainer`) made that bill
/// grow with the number of tabs, on a machine that has eight gigabytes to
/// share with the programs inside them.
///
/// So the tabs used least recently are hibernated: their surfaces, and the
/// `keep` clients attached through them, are let go of. Nothing else about
/// the tab changes — it is still in the strip, the sidebar and the search,
/// its colour and its activity still come from the daemon, which still has
/// the shell and its screen — and showing it builds the surfaces again, the
/// way the app does for a tab it has not shown yet.
enum HibernationPolicy {
    /// How many tabs keep their surfaces, the ones on screen included.
    static let defaultLimit = 6

    /// The limit this run goes by, read once: `KEEP_HYDRATED_LIMIT=<n>`
    /// changes it, and `off` keeps every surface, as before hibernation
    /// existed. Nil means never hibernate.
    static let limit = parseLimit(ProcessInfo.processInfo.environment["KEEP_HYDRATED_LIMIT"])

    static func parseLimit(_ raw: String?) -> Int? {
        guard let raw else { return defaultLimit }
        if raw == "off" { return nil }
        return Int(raw).map { max(0, $0) } ?? defaultLimit
    }

    struct Candidate {
        /// On screen in some window: never let go of.
        let visible: Bool
        /// When it was last put on screen, on a clock shared by all windows.
        let lastShown: UInt64
        /// Something the daemon cannot give back lives in its surfaces.
        let blocked: Bool
    }

    /// The indices of the candidates to hibernate.
    ///
    /// The tabs on screen count against the limit and are never chosen; of
    /// the rest, the most recently shown fill what room is left, and the
    /// others are chosen unless they are blocked. A blocked tab stays over
    /// the limit rather than lose what it holds, and is asked about again.
    static func sleepers(_ candidates: [Candidate], limit: Int) -> [Int] {
        let visible = candidates.filter(\.visible).count
        let room = max(0, limit - visible)
        let hidden = candidates.indices
            .filter { !candidates[$0].visible }
            .sorted { candidates[$0].lastShown > candidates[$1].lastShown }
        return hidden.dropFirst(room).filter { !candidates[$0].blocked }
    }
}

/// What the app tells the `keep` client of a surface, beyond its tab, and
/// what it tells the client keeping a pane once the surface is gone
/// (crates/keep/src/keeper.rs reads both). Pure, for the test of these rules
/// (tools/hibernate-logic-test.sh).
enum KeeperProtocol {
    /// The attach file's lines after the first three: follow the surface into
    /// a mirror keeping as much history (`scrollback`, in bytes), cut into
    /// cells as the surface cuts them, its cursor blinking or not as the
    /// surface's does before a program says; whose end is the app's (`pid`); and,
    /// for a pane that was kept, which client to take it over from. Nothing
    /// when panes are never let go of: then nobody needs a mirror.
    static func clientOptions(
        hibernating: Bool, scrollback: UInt, grapheme: Bool, blink: Bool, pid: Int32, take: String?
    ) -> [String] {
        guard hibernating, scrollback > 0 else { return [] }
        var lines = [
            "mirror=\(scrollback)", "grapheme=\(grapheme ? 1 : 0)", "blink=\(blink ? 1 : 0)", "app=\(pid)",
        ]
        if let take, take.hasPrefix("/") { lines.append("take=\(take)") }
        return lines
    }

    /// What a surface would answer about itself, for its client to answer
    /// with when it has none: `key=value`, colours as six hex digits.
    static func facts(
        foreground: [UInt8]?, background: [UInt8]?, cursor: [UInt8]?, palette: [[UInt8]],
        dark: Bool, cell: (width: UInt32, height: UInt32)?, version: String?, device: String
    ) -> [String] {
        var lines: [String] = []
        if let foreground, let hex = hex(foreground) { lines.append("foreground=\(hex)") }
        if let background, let hex = hex(background) { lines.append("background=\(hex)") }
        // A cursor with no colour of its own is drawn, and reported, in the
        // foreground's.
        if let cursor = cursor ?? foreground, let hex = hex(cursor) { lines.append("cursor=\(hex)") }
        let colours = palette.compactMap(hex)
        if colours.count == 256 { lines.append("palette=" + colours.joined(separator: ",")) }
        lines.append("scheme=\(dark ? "dark" : "light")")
        if let cell, cell.width > 0, cell.height > 0 { lines.append("cell=\(cell.width),\(cell.height)") }
        if let version, !version.isEmpty { lines.append("xtversion=ghostty \(version)") }
        lines.append("da=\(device)")
        return lines
    }

    static func hex(_ rgb: [UInt8]) -> String? {
        guard rgb.count == 3 else { return nil }
        return rgb.map { String(format: "%02x", $0) }.joined()
    }

    /// A colour the client wrote down as six hex digits.
    static func colour(_ text: String) -> [UInt8]? {
        let digits = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard digits.count == 6, let value = UInt32(digits, radix: 16) else { return nil }
        return [UInt8((value >> 16) & 0xff), UInt8((value >> 8) & 0xff), UInt8(value & 0xff)]
    }
}

/// The bytes a terminal's `text:` binding action types, for typing them
/// where there is no terminal to do it: into a hibernated tab, through the
/// daemon (`PaneTypist`).
enum TextAction {
    /// The action's text with its escapes read as the terminal reads them
    /// (`\xNN`, `\n`, `\r`, `\t`, `\\`, `\'`, `\"`). Nil for any other action, and
    /// for an escape it does not know: better to type nothing than the wrong
    /// thing into somebody's shell.
    static func bytes(of action: String) -> [UInt8]? {
        guard action.hasPrefix("text:") else { return nil }
        var out: [UInt8] = []
        var rest = action.dropFirst(5)
        while let first = rest.first {
            rest = rest.dropFirst()
            guard first == "\\" else {
                out.append(contentsOf: Array(String(first).utf8))
                continue
            }
            guard let kind = rest.first else { return nil }
            rest = rest.dropFirst()
            switch kind {
            case "n": out.append(0x0a)
            case "r": out.append(0x0d)
            case "t": out.append(0x09)
            case "\\", "'", "\"": out.append(contentsOf: Array(String(kind).utf8))
            case "x":
                guard rest.count >= 2, let byte = UInt8(rest.prefix(2), radix: 16) else { return nil }
                out.append(byte)
                rest = rest.dropFirst(2)
            default:
                return nil
            }
        }
        return out
    }
}
