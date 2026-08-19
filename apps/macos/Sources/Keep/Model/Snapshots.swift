import Foundation

/// Identity of a UI tab. Daemon tab ids are per-workspace counters, so the
/// workspace name is part of the identity: "dawd/1" and "luhw/1" are two
/// different tabs wearing the same number.
struct TabID: Hashable, Codable, CustomStringConvertible {
    let workspace: String
    let root: UInt32
    var description: String { "\(workspace)/\(root)" }
}

/// The whole of a tab's sidebar memory. Frozen on deactivation by simply not
/// being touched; restored by being re-applied.
struct SidebarState: Hashable, Codable {
    var isCollapsed: Bool
    var width: CGFloat
    static let initial = SidebarState(isCollapsed: false, width: 220)
}

/// One pane beyond a tab's root, in daemon order.
struct PaneState: Hashable {
    let tab: UInt32
    /// Protocol values: 1 = right, 2 = down.
    let splitDir: UInt8
}

/// The one downward channel. Every mutation in the app enters as one of
/// these; nothing in the UI reaches past this into state.
enum Intent {
    case activateWorkspace(String)
    case activateTab(TabID)
    /// 0-based; -1 means the last tab (⌘9, per macOS convention).
    case activateTabIndex(Int)
    case nextTab
    case previousTab
    case newTab(in: String?)          // nil = the active workspace
    case newWorkspace(named: String)
    case closeTab(TabID?)             // nil = the active tab (⌘W)
    case killWorkspace(String)
    case split(UInt8)                 // protocol values: 1 = right, 2 = down
    case focusPane(UInt32)            // a surface of the active tab took focus
    case setSidebar(SidebarState)     // the active tab's; from toggle or divider drag
}

/// Everything layer 6 needs to draw a frame. Pure values: views never appear
/// here — the UI resolves ids through the surface pool.
struct SessionSnapshot: Hashable {
    struct SidebarRow: Hashable, Identifiable {
        let name: String
        let subtitle: String            // "3 tabs · running"
        let dot: Dot
        let isActive: Bool
        var id: String { name }
        enum Dot: Hashable { case empty, busy, attached, idle }
    }

    struct StripItem: Hashable, Identifiable {
        let id: TabID
        let title: String               // resolved label, no markers
        let busy: Bool                  // strip renders ✳
        let hasPanes: Bool              // strip renders ⊞
        let isActive: Bool
    }

    struct ActiveTab: Hashable {
        let id: TabID
        let title: String               // window title
        let panes: [PaneState]          // beyond the root, daemon order
        let sidebar: SidebarState       // the frozen state to restore
        let focusedPane: UInt32         // daemon tab id holding the keyboard
    }

    var rows: [SidebarRow]
    var strip: [StripItem]
    var active: ActiveTab?
    /// Every live tab across all workspaces. The UI unmounts hosts whose id
    /// left this set; it never decides on its own that a tab is gone.
    var universe: Set<TabID>
}
