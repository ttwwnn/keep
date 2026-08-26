# Graph Report - .  (2026-08-18)

## Corpus Check
- Corpus is ~18,042 words - fits in a single context window. You may not need a graph.

## Summary
- 502 nodes · 1024 edges · 21 communities (18 shown, 3 thin omitted)
- Extraction: 96% EXTRACTED · 4% INFERRED · 0% AMBIGUOUS · INFERRED: 40 edges (avg confidence: 0.81)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- macOS Window Chrome
- Product Architecture
- Wire Protocol Codec
- macOS Daemon Client
- Ghostty Surface Input
- VT Terminal Core
- Persistent Tab Sessions
- Daemon Server Registry
- Workspace Model
- CLI Bootstrap
- Persistence Integration Tests
- Ghostty App Configuration
- Workspace Picker
- macOS App Lifecycle
- PTY Runtime Tests
- Client Attachment
- Daemon Entry Point
- Ghostty Vendor Build
- Ghostty Vendor Fetch

## God Nodes (most connected - your core abstractions)
1. `TerminalSurfaceView` - 32 edges
2. `Tab` - 24 edges
3. `Store` - 22 edges
4. `Daemon` - 20 edges
5. `TerminalWindowController` - 20 edges
6. `Registry` - 18 edges
7. `Keep Application Target` - 18 edges
8. `Workspace` - 16 edges
9. `WorkspaceInfo` - 15 edges
10. `Reader` - 14 edges

## Surprising Connections (you probably didn't know these)
- `Metal Framework` --semantically_similar_to--> `Metal Terminal Rendering`  [INFERRED] [semantically similar]
  apps/macos/project.yml → README.md
- `GhosttyKit.xcframework` --conceptually_related_to--> `libghostty-vt`  [INFERRED]
  apps/macos/project.yml → README.md
- `Failure` --implements--> `Error`  [EXTRACTED]
  apps/macos/Sources/Keep/Daemon.swift → crates/keep-vt/src/lib.rs
- `Keep Application Target` --implements--> `Native macOS App`  [INFERRED]
  apps/macos/project.yml → README.md
- `Cocoa Framework` --conceptually_related_to--> `Native macOS Tab Integration`  [INFERRED]
  apps/macos/project.yml → README.md

## Import Cycles
- None detected.

## Hyperedges (group relationships)
- **Persistent Workspace Runtime** — readme_persistent_terminal_workspaces, readme_keep_daemon, readme_keep_client, readme_unix_socket, readme_libghostty_vt, readme_terminal_screen_repaint [EXTRACTED 1.00]
- **Phase One Components** — readme_keep_vt, readme_keep_proto, readme_keepd, readme_keep_cli, readme_persistent_terminal_workspaces [EXTRACTED 1.00]
- **Native macOS Terminal Surface Stack** — readme_native_macos_app, readme_native_tab_integration, readme_metal_rendering, readme_swiftui_nsview_bridge, apps_macos_project_keep_target, apps_macos_project_ghosttykit_xcframework, apps_macos_project_metal_framework, apps_macos_project_metalkit_framework [INFERRED 0.95]

## Communities (21 total, 3 thin omitted)

### Community 0 - "macOS Window Chrome"
Cohesion: 0.06
Nodes (31): AppKit, KeepWindow, SidebarToggle, NSView, Store, String, UInt8, Any (+23 more)

### Community 1 - "Product Architecture"
Cohesion: 0.06
Nodes (44): Carbon Framework, Cocoa Framework, Code Signing Configuration, CoreGraphics Framework, CoreText Framework, CoreVideo Framework, GhosttyKit.xcframework, Hardened Runtime (+36 more)

### Community 2 - "Wire Protocol Codec"
Cohesion: 0.13
Nodes (24): bad(), Buf, client_messages_round_trip(), ClientMsg, Cursor, Cursor<'a>, frames_do_not_desynchronise(), oversized_frame_is_refused_not_allocated() (+16 more)

### Community 3 - "macOS Daemon Client"
Cohesion: 0.17
Nodes (20): Daemon, Failure, cannotConnect, protocolError, Reader, Bool, String, UInt32 (+12 more)

### Community 4 - "Ghostty Surface Input"
Cohesion: 0.10
Nodes (17): Bool, NSCoder, String, UInt32, TerminalSurface, TerminalSurfaceView, Context, CVDisplayLink (+9 more)

### Community 5 - "VT Terminal Core"
Cohesion: 0.11
Nodes (24): c_int, c_void, _assert_c_void_used(), Error, Format, FormatterImpl, FormatterOptions, GhosttyString (+16 more)

### Community 6 - "Persistent Tab Sessions"
Cohesion: 0.09
Nodes (22): AtomicBool, Box, Child, Attachment, Inner, Arc, CommandBuilder, Drop (+14 more)

### Community 7 - "Daemon Server Registry"
Cohesion: 0.11
Nodes (21): Registry, Arc, Mutex, Option, Result, Self, String, Vec (+13 more)

### Community 8 - "Workspace Model"
Cohesion: 0.16
Nodes (13): AtomicU32, Entry, Arc, Mutex, Option, Result, Self, String (+5 more)

### Community 9 - "CLI Bootstrap"
Cohesion: 0.19
Nodes (21): a_missing_tab_means_any_tab(), daemon_binary(), embedded_target(), embedded_target_in(), ensure_daemon(), enter(), list(), main() (+13 more)

### Community 10 - "Persistence Integration Tests"
Cohesion: 0.20
Nodes (20): attaching_twice_reuses_one_shell(), clients_of(), disconnected_client_stops_being_counted(), list_reports_live_workspaces(), read_repaint(), read_until(), Duration, Option (+12 more)

### Community 11 - "Ghostty App Configuration"
Cohesion: 0.19
Nodes (9): GhosttyApp, NSColor, String, Foundation, ghostty_action_color_change_s, ghostty_app_t, ghostty_config_color_s, ghostty_config_t (+1 more)

### Community 12 - "Workspace Picker"
Cohesion: 0.18
Nodes (14): WorkspaceInfo, adjacent_matches_outrank_scattered_ones(), Choice, filtered(), fuzzy_score(), matches_subsequences_not_just_prefixes(), pick(), Option (+6 more)

### Community 13 - "macOS App Lifecycle"
Cohesion: 0.22
Nodes (7): AppDelegate, Any, Bool, Notification, NSApplication, NSApplicationDelegate, NSObject

### Community 14 - "PTY Runtime Tests"
Cohesion: 0.21
Nodes (13): attach_has_no_gap_and_no_duplicate(), busy_tracks_the_foreground_command(), detaching_stops_counting_the_client_right_away(), forwards_input_to_child(), repaint_carries_styling_that_plain_text_drops(), CommandBuilder, Duration, Tab (+5 more)

### Community 15 - "Client Attachment"
Cohesion: 0.29
Nodes (7): attach(), Outcome, RawGuard, Drop, Path, Result, Self

## Knowledge Gaps
- **24 isolated node(s):** `TerminalImpl`, `FormatterImpl`, `GhosttyString`, `build-ghostty-macos.sh script`, `fetch.sh script` (+19 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **3 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Error` connect `VT Terminal Core` to `Wire Protocol Codec`, `macOS Daemon Client`?**
  _High betweenness centrality (0.411) - this node is a cross-community bridge._
- **Why does `Failure` connect `macOS Daemon Client` to `VT Terminal Core`?**
  _High betweenness centrality (0.365) - this node is a cross-community bridge._
- **Why does `Daemon` connect `macOS Daemon Client` to `macOS Window Chrome`?**
  _High betweenness centrality (0.254) - this node is a cross-community bridge._
- **Are the 3 inferred relationships involving `TerminalWindowController` (e.g. with `.openTab()` and `.show()`) actually correct?**
  _`TerminalWindowController` has 3 INFERRED edges - model-reasoned connections that need verification._
- **What connects `TerminalImpl`, `FormatterImpl`, `GhosttyString` to the rest of the system?**
  _25 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `macOS Window Chrome` be split into smaller, more focused modules?**
  _Cohesion score 0.058385093167701865 - nodes in this community are weakly interconnected._
- **Should `Product Architecture` be split into smaller, more focused modules?**
  _Cohesion score 0.06236786469344609 - nodes in this community are weakly interconnected._