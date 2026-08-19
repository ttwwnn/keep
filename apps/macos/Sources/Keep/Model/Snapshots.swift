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

/// One pane beyond a tab's root, in daemon (creation) order.
struct PaneState: Hashable {
    let tab: UInt32
    /// The pane this one was split off from — the root, or another pane.
    let splitOf: UInt32
    /// Protocol values: 1 = right, 2 = down.
    let splitDir: UInt8
}

/// How a tab's panes are arranged.
///
/// Splitting replaces the pane you were in with a pair: that pane and the new
/// one, side by side or stacked. Do it again on either half and that half is
/// replaced in turn — so an arrangement is a tree, and a tab that mixes
/// directions is the ordinary case rather than the exotic one.
///
/// The daemon has recorded this all along (each pane knows which pane it was
/// split from, and in which direction); this rebuilds the shape from those
/// records.
indirect enum PaneTree: Hashable {
    case leaf(UInt32)
    /// `vertical` is the divider's orientation: vertical divider = side by
    /// side, which is what "split right" means.
    case split(vertical: Bool, PaneTree, PaneTree)

    static func build(root: UInt32, panes: [PaneState]) -> PaneTree {
        var tree = PaneTree.leaf(root)
        // Creation order matters: a pane can only be split off something that
        // already exists, so replaying in order rebuilds the exact shape.
        for pane in panes {
            tree = tree.replacing(
                leaf: pane.splitOf,
                with: .split(
                    vertical: pane.splitDir != 2,
                    .leaf(pane.splitOf),
                    .leaf(pane.tab)
                )
            )
        }
        return tree
    }

    private func replacing(leaf target: UInt32, with subtree: PaneTree) -> PaneTree {
        switch self {
        case .leaf(let id):
            return id == target ? subtree : self
        case .split(let vertical, let first, let second):
            return .split(
                vertical: vertical,
                first.replacing(leaf: target, with: subtree),
                second.replacing(leaf: target, with: subtree)
            )
        }
    }

    var leaves: [UInt32] {
        switch self {
        case .leaf(let id): return [id]
        case .split(_, let first, let second): return first.leaves + second.leaves
        }
    }
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
