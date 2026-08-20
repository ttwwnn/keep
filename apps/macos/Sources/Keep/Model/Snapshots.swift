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
    static let initial = SidebarState(isCollapsed: false, width: 250)
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
    /// Where a dragged pane was let go, relative to the pane under it.
    enum DropSide {
        case left, right, top, bottom
        /// On the pane itself: the two trade places.
        case onto
    }

    /// Put the tabs of the active workspace in this order, by hand.
    case reorderTabs([UInt32])

    /// Put the workspaces in a different order, by hand.
    case reorderWorkspaces(from: IndexSet, to: Int)

    /// Move a pane next to another one, or trade places with it.
    case movePane(UInt32, to: UInt32, side: DropSide)
    /// Take a pane out of its arrangement and give it a tab of its own.
    case detachPane(UInt32)

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
    /// A surface took the keyboard. The tab travels with it: rebuilding an
    /// arrangement makes AppKit reassign the first responder, and a surface
    /// belonging to a hidden tab can pick it up — a report with no tab on it
    /// would be written to whichever tab happened to be active.
    case focusPane(TabID, UInt32)
    case setSidebar(SidebarState)     // from the toggle or a divider drag
    /// Open the picker, or put it away if it is already up: the key that
    /// summons it is the key that dismisses it.
    case togglePicker
    /// Same overlay, other question: what is in the history rather than
    /// where can I go. Scoped to the pane you are in, or across everything.
    case toggleSearch(global: Bool)
    case closePicker
    /// What was typed. Local filtering answers the "go to" list; searching
    /// history is a question only the daemon can answer.
    case setPickerQuery(String)
    /// Show what this row is; nil clears the preview.
    case previewPickerItem(String?)
    case choosePickerItem(String)
    /// ⌃D on a row: end what it points at.
    case dismissPickerItem(String)
}

/// The flat "go to" list.
///
/// Modelled on the picker this replaces, and on the reason its author gave
/// for it: you do not choose a session and then a window — you choose the
/// thing and land on it. So there is no hierarchy here. Everything running
/// is one row, most recently visited first, and below it the places to start
/// something new.
struct PickerModel: Hashable {
    /// Which question the overlay is asking. It is one overlay because it is
    /// one gesture — type, move, choose — and splitting it in two would mean
    /// two of everything to keep in step.
    enum Mode: Hashable {
        /// Filtering happens locally: the list is already in hand.
        case goTo
        /// Every keystroke is a question for the daemon, which is the only
        /// one holding the history. `global` decides whether the question is
        /// about everything or only the pane in front of you.
        case search(global: Bool)
    }

    struct Item: Hashable, Identifiable {
        enum Kind: Hashable {
            /// A tab that exists, in some workspace.
            case running(TabID)
            /// A directory to open a new workspace in.
            case destination(path: String)
            /// A line of history: the tab it is in, and the pane of that
            /// tab that holds it. A hit is found by pane, and a pane is not a
            /// tab — going to one means opening its tab and focusing it.
            case hit(TabID, pane: UInt32, line: UInt32, fromEnd: UInt32)

        }
        let kind: Kind
        /// "workspace › title", or the directory's name.
        let title: String
        /// The path, or what the tab is doing.
        let detail: String
        let busy: Bool
        var id: String {
            switch kind {
            case .running(let tab): return "run:\(tab)"
            case .destination(let path): return "dir:\(path)"
            case .hit(let tab, let pane, let line, _): return "hit:\(tab):\(pane):\(line)"
            }
        }
    }

    var mode: Mode
    /// Search only: what matched, so a row can mark it and a header can count.
    struct Match: Hashable {
        let range: Range<Int>       // byte range within the row's title
        let before: [String]
        let after: [String]
        let group: String           // "workspace › tab N"
    }
    var matches: [String: Match]
    /// Search only: nil while the daemon has not answered yet.
    var scopeLabel: String?
    var query: String
    var items: [Item]
    var previewOf: String?
    var previewText: String
}

/// Everything layer 6 needs to draw a frame. Pure values: views never appear
/// here — the UI resolves ids through the surface pool.
struct SessionSnapshot: Hashable {
    struct SidebarRow: Hashable, Identifiable {
        let name: String
        let subtitle: String            // "3 tabs · running", kept for the tooltip
        let tabs: Int
        /// What is running here, one entry per busy pane. The title a busy
        /// pane wears, which is the command for shells that rename their
        /// window while one runs.
        let running: [String]
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
    /// Non-nil while the picker is open.
    var picker: PickerModel?
    var rows: [SidebarRow]
    var strip: [StripItem]
    var active: ActiveTab?
    /// Every live tab across all workspaces. The UI unmounts hosts whose id
    /// left this set; it never decides on its own that a tab is gone.
    var universe: Set<TabID>
}
