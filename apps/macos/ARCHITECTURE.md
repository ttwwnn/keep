# The macOS shell, in layers

The app is organized like a game engine: isolated layers with single
responsibilities, entities with handles, and immutable snapshots crossing the
boundaries. Nothing below the UI knows a window exists; nothing above a layer
mutates the one beneath it directly.

The rule that shapes everything else: **there is one NSWindow, forever.** Every
bug the shell ever had — flicker on switch, focus falling to another app, the
tab bar landing in the wrong row, fights with tiling window managers — came
from creating, closing, hiding or revealing windows. Switching workspace and
switching tab are the same operation: flip which mounted views are visible.
Nothing else moves.

## Layers

```
6  UI / render      MainWindowController, KeepWindow, TabStripView,
                    TabContentContainer, SidebarHost, WorkspaceSidebar
5  Workspace        Session, WorkspaceEntity          which tab is active, order, focus
4  Pane / Tab       TabEntity                         per-tab state: label, panes, focus
3  Display          SurfacePool, TerminalSurfaceView, GhosttyApp
2  Emulation        keepd + libghostty                (crates/, untouched by the app)
1  PTY              keepd                             (crates/, untouched by the app)
```

- **Intents flow down** (6 → 5): the UI never mutates state; it dispatches an
  `Intent` and waits to be handed a new snapshot.
- **Snapshots flow up** (4 → 5 → 6): plain `Equatable` values. Views never
  appear in snapshots — they carry ids, and the UI resolves ids through
  `SurfacePool`.
- **One writer**: `Session` is the only code that changes which workspace or
  tab is active. `Store.selectedWorkspace`, `WindowManager.currentWorkspace`
  and the re-entrancy flags that guarded them do not exist anymore.

## Folder structure

```
apps/macos/Sources/Keep/
├── main.swift                    entry point, AppDelegate, menus (⌘T, ⌘W, ⌘1–9…)
├── Support/
│   └── Trace.swift               KEEP_TRACE-gated session tracing
├── Daemon/
│   ├── Daemon.swift              wire protocol + Daemon.Tab/Workspace models
│   └── DaemonPoller.swift        2 s timer → Session.reconcile
├── Model/                        layers 4–5 — no AppKit import
│   ├── Snapshots.swift           TabID, SidebarState, PaneState, Intent, *Snapshot
│   ├── TabEntity.swift           layer 4: one per daemon root tab
│   ├── WorkspaceEntity.swift     layer 5: order, active tab, per-tab focus
│   ├── Session.swift             layer-5 root: single writer, replaces Store
│   └── SidebarStateStore.swift   per-tab sidebar persistence (JSON)
├── Display/
│   ├── SurfacePool.swift         surfaces created once, keyed workspace/tab
│   └── Ghostty/
│       ├── GhosttyApp.swift      libghostty runtime
│       └── SurfaceView.swift     Metal surface; visibility-aware display link
└── UI/
    ├── MainWindowController.swift  THE window; renders snapshots; switch pipeline
    ├── KeepWindow.swift            chrome: unified toolbar, tint, sidebar toggle
    ├── TabStripView.swift          app-drawn tab bar, native look
    ├── TabContentContainer.swift   mounted tab hosts; exactly one visible
    ├── SidebarHost.swift           flat sidebar host + backdrop + state applier
    └── WorkspaceSidebar.swift      SwiftUI workspace list (snapshot in, intent out)
```

## The interfaces that matter

```swift
/// Identity of a UI tab. Daemon tab ids are per-workspace counters, so the
/// workspace name is part of the identity ("dawd/1" and "luhw/1" are different
/// tabs wearing the same number).
struct TabID: Hashable, Codable { let workspace: String; let root: UInt32 }

/// The sidebar's collapsed state and width. ONE value for the whole app —
/// see "The sidebar is furniture" below.
struct SidebarState: Codable, Equatable { var isCollapsed: Bool; var width: CGFloat }

/// The one downward channel. Every mutation in the app enters through here.
enum Intent {
    case activateWorkspace(String), activateTab(TabID), activateTabIndex(Int)
    case newTab(in: String?), newWorkspace(named: String)
    case closeTab(TabID?), killWorkspace(String)
    case split(UInt8), focusPane(UInt32)
    case setSidebar(SidebarState)
    case nextTab, previousTab
}

/// Layer 4. Owns what is this tab's own business — title, busy, panes, which
/// pane holds the keyboard — and nothing about how any of it is drawn.
@MainActor final class TabEntity {
    let id: TabID
    private(set) var title: String, busy: Bool, panes: [PaneState]
    private(set) var focusedPane: UInt32
    func apply(root: Daemon.Tab, panes: [Daemon.Tab]) -> Bool   // reconciliation; true = changed
    func noteFocus(pane: UInt32)                                 // single writer: Session
}

/// Layer 5 root. The UI holds a reference to this and to nothing else below.
@MainActor final class Session {
    weak var renderer: SessionRendering?
    func start()
    func dispatch(_ intent: Intent)
    func reconcile(_ listing: [Daemon.Workspace])   // from DaemonPoller
}

/// What the UI implements. render() receives a full immutable snapshot and
/// diffs it against the last one applied.
@MainActor protocol SessionRendering: AnyObject {
    func render(_ snapshot: SessionSnapshot)
    func present(error: String)
}
```

## The switch pipeline

One synchronous function, one run-loop turn, no dispatch hops. It is the only
switch path in the program — sidebar clicks, strip clicks, ⌘1–9 and empty-
workspace entry all funnel into it.

```
1. sidebarHost.apply(incoming.sidebar, animated: false)  chrome geometry first:
                                                         sidebar width decides
                                                         terminal width
2. incoming.layoutSubtreeIfNeeded()                      surfaces sized for the
                                                         geometry they will have
3. incoming panes: resumeDrawing()                       display link on + ONE
                                                         synchronous draw while
                                                         still hidden
4. CATransaction { incoming.isHidden = false             the swap commits as a
                   outgoing.isHidden = true }            single compositor frame:
                                                         never zero tabs visible
5. outgoing panes: suspendDrawing()                      after hiding, never before
6. makeFirstResponder(incoming.focusedPane surface)      intra-window move; the
                                                         window never resigns key
7. strip selection + guarded window.title                pure paints
```

Hidden surfaces draw zero frames (`viewDidHide` stops the link; occlusion
observing remains the backstop for minimize/bury) and defer PTY resizes —
a window resize touches only the visible tab's sessions; the backlog flushes
as one call on reveal.

## Data flows

**Open a new tab** (⌘T or strip "+"): strip → `dispatch(.newTab(in: nil))` →
Session resolves the active workspace, calls the daemon, re-lists, creates a
`TabEntity` whose sidebar state is seeded from the active tab's (so ⌘T never
jumps the sidebar) → activates it → `render`: the container mounts a host on
first presentation — this is hydration, the one moment a surface and its
`keep` client are created — then the pipeline above runs.

**Collapse the sidebar**: toggle → `dispatch(.setSidebar(collapsed))` →
Session stores it once, app-wide, and persists it (debounced JSON,
`~/Library/Application Support/Keep/sidebar-state.json`) → render applies it
animated. A divider drag reports back the same way, debounced, guarded
against echo.

**The sidebar is furniture.** It was per-tab at first, frozen and restored
with each one. In use that reads as a glitch, not as memory: you collapse it,
move to another tab, and it is back. So there is one state, and it stays
where you put it no matter where you are. This is a deliberate reversal of
the original per-tab spec, made after living with it.

**Switch tab** (same flow for switch workspace): click → `dispatch(.activateTab)`
→ Session updates the active ids — the outgoing entity needs no freezing,
since nothing sends a deactivated entity messages → `render` → pipeline. A workspace click resolves to that workspace's remembered
active tab and joins the identical path.

**Return to the previous tab**: identical dispatch; every step is a cache hit.
The host is still mounted, surfaces alive, Metal layers holding their last
frame; the pipeline puts the keyboard back on the pane that had it, and the
focus ring marks which one that is when the tab is split.

## Rules kept from the session's scars

- Navigation never hangs off selection state (`List(selection:)` wrote
  selection on reveal; every write was a switch nobody asked for).
- The daemon is the source of truth for tabs; the poller reconciles every 2 s
  and only ever prunes or relabels — it cannot mount, present, or switch.
- Closing the window quits the app; the daemon keeps everything running.
  Closing a *tab* is only ever the explicit intent. ⌘W closes the focused
  **pane** — for a tab with no splits the two are the same thing — and the
  daemon hands a closed pane's children to its parent, so closing one pane
  never takes the arrangement apart.
- Unchanged values are silent: snapshots are Equatable and every applier
  diffs, so a quiet poll produces zero view churn (and zero accessibility
  noise for window managers to react to).
