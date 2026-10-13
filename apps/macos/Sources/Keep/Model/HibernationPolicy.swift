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
