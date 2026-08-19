import AppKit

/// Session tracing, off unless `KEEP_TRACE` is set in the environment.
///
/// Exists because the interesting questions about this app are not answerable
/// from outside it. An observer watching the window server can see that a
/// window resized; only the app knows whether *it* asked for that. Under a
/// tiling window manager that difference is the whole diagnosis.
///
/// Lines are timestamped to the millisecond in the same format the external
/// observer uses, so the two logs interleave with a plain sort.
///
/// Nothing here records terminal content or keystrokes.
enum Trace {
    static let enabled = ProcessInfo.processInfo.environment["KEEP_TRACE"] != nil

    /// True while the switch pipeline is running, so geometry changes can be
    /// attributed. A resize logged with `bySelf=false` came from outside the
    /// app — the person dragging an edge, or their window manager re-tiling.
    static var insideSwitch = false

    private static let formatter: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "HH:mm:ss.SSS"
        return f
    }()

    /// `detail` is an autoclosure so that building the string costs nothing
    /// when tracing is off — this is called from a display link handler.
    static func log(_ kind: String, _ detail: @autoclosure () -> String) {
        guard enabled else { return }
        let kind = kind.padding(toLength: 9, withPad: " ", startingAt: 0)
        FileHandle.standardError.write(
            "\(formatter.string(from: Date()))          \(kind) \(detail())\n".data(using: .utf8)!
        )
    }

    /// The dynamic class of a responder. `type(of:)` on the optional itself
    /// only ever reports `NSResponder`, which says nothing.
    static func describe(_ responder: NSResponder?) -> String {
        guard let responder else { return "nil" }
        return String(describing: type(of: responder))
    }

    static func frame(_ window: NSWindow?) -> String {
        guard let window else { return "no-window" }
        let f = window.frame
        return "\(Int(f.origin.x)),\(Int(f.origin.y)) \(Int(f.width))x\(Int(f.height))"
    }
}
