import Foundation

/// Identity of a UI tab. Daemon tab ids are per-workspace counters, so the
/// workspace name is part of the identity: "dawd/1" and "luhw/1" are two
/// different tabs wearing the same number.
struct TabID: Hashable, Codable, CustomStringConvertible {
    let workspace: String
    let root: UInt32
    var description: String { "\(workspace)/\(root)" }
}

/// The sidebar's collapsed state and width.
///
/// One value for the whole app, not one per tab: the sidebar is furniture,
/// and furniture that rearranges itself as you move between tabs reads as a
/// glitch rather than as memory.
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
    case closeTab(TabID?)             // nil = the active tab, panes and all
    case closePane(UInt32?)           // nil = the focused pane (⌘W)
    case killWorkspace(String)
    case split(UInt8)                 // protocol values: 1 = right, 2 = down
    case focusPane(UInt32)            // a surface of the active tab took focus
    case setSidebar(SidebarState)     // from the toggle or a divider drag
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
        let focusedPane: UInt32         // daemon tab id holding the keyboard
    }

    var sidebar: SidebarState
    var rows: [SidebarRow]
    var strip: [StripItem]
    var active: ActiveTab?
    /// Every live tab across all workspaces. The UI unmounts hosts whose id
    /// left this set; it never decides on its own that a tab is gone.
    var universe: Set<TabID>
}
