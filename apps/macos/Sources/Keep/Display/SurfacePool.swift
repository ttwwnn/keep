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
/// does, or when the window that borrowed it closes.
///
/// **Keyed by window as well as by tab.** A surface is an `NSView`, and a view
/// has one superview: two windows showing one tab cannot share it, and trying
/// would silently tear the terminal out of whichever window asked first. So
/// each window gets its own, which means its own `keep` client, which means
/// its own subscriber on the daemon's tab — two clients on one shell, the way
/// two people attached to one tmux session are two clients. The daemon has
/// accepted that since before this app existed.
///
/// Main thread only: this holds views.
final class SurfacePool {
    static let shared = SurfacePool()

    private struct Key: Hashable {
        let window: WindowID
        let workspace: String
        let tab: UInt32
    }

    private var surfaces: [Key: TerminalSurfaceView] = [:]

    private init() {}

    /// The surface a window shows a tab through, made on first use.
    ///
    /// Named with the window rather than defaulting to one, so that adding
    /// windows made every caller of the old name fail to compile. A caller
    /// missed here does not misbehave visibly — it hands one window's live
    /// terminal to another, and the first goes blank.
    func surface(window: WindowID, workspace: String, tab: UInt32) -> TerminalSurfaceView {
        let key = Key(window: window, workspace: workspace, tab: tab)
        if let existing = surfaces[key] { return existing }
        let surface = TerminalSurfaceView(workspace: workspace, tab: tab)
        surface.translatesAutoresizingMaskIntoConstraints = false
        surfaces[key] = surface
        Trace.log("pool", "made \(window) \(workspace)/\(tab) total=\(surfaces.count)")
        return surface
    }

    /// One window's surface for a pane, only where it already exists.
    ///
    /// Distinct from `surface(window:workspace:tab:)`, which makes one: asking
    /// a question about a pane must not start a client for it.
    func existing(window: WindowID, workspace: String, tab: UInt32) -> TerminalSurfaceView? {
        surfaces[Key(window: window, workspace: workspace, tab: tab)]
    }

    /// Any window's surface for a pane.
    ///
    /// For questions whose answer cannot differ between windows — what
    /// directory the shell is in, say. They are all watching one shell, so
    /// whichever answers first is right. Anything that acts on a particular
    /// window's view of a tab, like scrolling it to a search hit, must ask for
    /// that window's instead.
    func anyExisting(workspace: String, tab: UInt32) -> TerminalSurfaceView? {
        surfaces.first { $0.key.workspace == workspace && $0.key.tab == tab }?.value
    }

    /// Let go of a tab's surfaces, in every window, which tears down their
    /// clients.
    ///
    /// Only for a tab the daemon no longer has. A surface dropped while its
    /// tab is alive would be rebuilt from nothing the next time the tab is
    /// shown, which is the cost this pool exists to avoid.
    func discard(workspace: String, tab: UInt32) {
        for key in surfaces.keys where key.workspace == workspace && key.tab == tab {
            drop(key)
        }
    }

    /// Drop every surface of a workspace, for a workspace that is being killed.
    func discardAll(workspace: String) {
        for key in surfaces.keys where key.workspace == workspace { drop(key) }
    }

    /// Drop everything one window was showing, for a window that has closed.
    ///
    /// Only that window's: the others are still on screen, and a tab mirrored
    /// into two windows keeps the surface belonging to the one that stayed.
    func discardAll(window: WindowID) {
        for key in surfaces.keys where key.window == window { drop(key) }
    }

    private func drop(_ key: Key) {
        guard let surface = surfaces.removeValue(forKey: key) else { return }
        surface.removeFromSuperview()
        Trace.log(
            "pool", "discarded \(key.window) \(key.workspace)/\(key.tab) total=\(surfaces.count)")
    }

    /// Tabs any window holds a surface for, so callers can reconcile. A tab
    /// mirrored into two windows appears once.
    func tabs(in workspace: String) -> Set<UInt32> {
        Set(surfaces.keys.filter { $0.workspace == workspace }.map(\.tab))
    }
}
