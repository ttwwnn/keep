import AppKit

/// Every terminal surface the app has opened, kept alive across the windows
/// that show them.
///
/// A surface is not a piece of a window's layout — it owns a libghostty
/// renderer and a `keep` client process attached to a daemon tab, and building
/// one costs a process spawn, a socket, a shell and a repaint before it has
/// anything to draw. Tying its lifetime to whichever window happens to be
/// displaying it means paying that price every time the arrangement changes.
///
/// So windows borrow surfaces from here. A surface outlives being taken out of
/// one window and put into another; it goes away only when its daemon tab
/// does.
///
/// Main thread only, like `WindowManager` beside it: this holds views.
final class SurfacePool {
    static let shared = SurfacePool()

    private struct Key: Hashable {
        let workspace: String
        let tab: UInt32
    }

    private var surfaces: [Key: TerminalSurfaceView] = [:]

    private init() {}

    /// The surface for a tab, made on first use and reused after that.
    func surface(workspace: String, tab: UInt32) -> TerminalSurfaceView {
        let key = Key(workspace: workspace, tab: tab)
        if let existing = surfaces[key] { return existing }
        let surface = TerminalSurfaceView(workspace: workspace, tab: tab)
        surface.translatesAutoresizingMaskIntoConstraints = false
        surfaces[key] = surface
        Trace.log("pool", "made \(workspace)/\(tab) total=\(surfaces.count)")
        return surface
    }

    /// The surface for a pane, only where one already exists.
    ///
    /// Distinct from `surface(workspace:tab:)`, which makes one: asking a
    /// question about a pane must not start a client for it.
    func existing(workspace: String, tab: UInt32) -> TerminalSurfaceView? {
        surfaces[Key(workspace: workspace, tab: tab)]
    }

    /// Let go of a tab's surface, which tears down its client.
    ///
    /// Only for a tab the daemon no longer has. A surface dropped while its
    /// tab is alive would be rebuilt from nothing the next time the tab is
    /// shown, which is the cost this pool exists to avoid.
    func discard(workspace: String, tab: UInt32) {
        let key = Key(workspace: workspace, tab: tab)
        guard let surface = surfaces.removeValue(forKey: key) else { return }
        surface.removeFromSuperview()
        Trace.log("pool", "discarded \(workspace)/\(tab) total=\(surfaces.count)")
    }

    /// Drop every surface of a workspace, for a workspace that is being killed.
    func discardAll(workspace: String) {
        for key in surfaces.keys where key.workspace == workspace {
            discard(workspace: key.workspace, tab: key.tab)
        }
    }

    /// Surfaces whose daemon tab is gone, so callers can reconcile.
    func tabs(in workspace: String) -> [UInt32] {
        surfaces.keys.filter { $0.workspace == workspace }.map(\.tab)
    }
}
