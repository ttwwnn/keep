# keep

Persistent terminal workspaces, built on [libghostty-vt](https://libghostty.tip.ghostty.org/).

> Working name. Early, but usable: phase 1 works end to end.

## Why

Terminal emulators lose your work. Close the window and every process dies with
it. `tmux` solves that, but it also brings a hierarchy — workspaces, then windows,
then panes — that you have to navigate even when you never split a pane.

`keep` takes the other half of the trade: real persistence with a shape you
can hold in your head. A workspace is a project; a workspace has tabs. Both live
in the daemon, so closing the window loses neither.

## How

A terminal that owns your processes cannot be the same thing as the window you
close. So `keep` splits them:

    ┌─ daemon ──────────────────────────────────┐
    │  owns the PTYs, outlives every client     │
    │  libghostty-vt keeps each workspace's grid  │
    └───────────────────────────────────────────┘
                        ▲  unix socket
            ┌───────────┴───────────┐
            │ client (attach/detach)│
            └───────────────────────┘

Because the daemon holds the screen state, a client that reconnects can be
repainted exactly as the workspace left it — `libghostty-vt` can emit the whole
screen back as VT sequences, colors and hyperlinks included.

The core is portable and knows nothing about UI. Each platform gets a real
native client instead of a lowest-common-denominator one.

## Status

**Phase 1 — daemon + terminal client.** Working. The client runs inside an
existing terminal and paints by writing VT sequences, so there is no renderer
and no font handling to build.

- [x] `keep-vt` — safe bindings for libghostty-vt
- [x] `keep-proto` — framed wire protocol
- [x] `keepd` — PTY ownership, screen state, workspace registry, unix socket
- [x] `keep` — attach, detach, repaint, input forwarding, flat picker
- [x] workspaces hold tabs; tabs persist with the workspace

**Phase 2 — native macOS app.** Working. One line of chrome: traffic lights,
a sidebar toggle, the native macOS tab bar and its "+" button all share the
titlebar row (the tab bar is constrained in there, an approach adapted from
Ghostty's titlebar tabs). The workspace sidebar runs the full height of the
window Finder-style and the tab bar starts at its edge — collapse it and the
tabs slide over to the toggle. Each tab is a real libghostty surface (Metal
rendering, your own Ghostty fonts and theme) running the `keep` client, and
tabs are windows sharing a `tabbingIdentifier`, so ⌘1…⌘9, drag to reorder
and the overview come from the system. Closing a tab closes it; quitting the
app leaves every tab running in the daemon.

Tabs are labelled with the title the program inside sets (OSC 0/2), which
shells and editors do on their own, so a tab says what it is without anyone
naming it. A workspace also reports whether it is *running* something, taken
from the terminal's foreground process group rather than from shell
integration, so a build in a workspace nobody is watching still says so.

One wrinkle worth knowing about: the libghostty build we link against accepts
the per-surface `command`, `env_vars` and `initial_input` fields and then
ignores them. Only `working_directory` survives, so the app fixes the client
app-wide and hands each surface its target through a file in a private
working directory, which the client reads and deletes.

Not done yet: keyboard shortcuts, split views, scrollback (the repaint covers
the visible screen only), and workspaces do not survive a reboot.

**Phase 2 — native macOS client.** SwiftUI for chrome, `NSView` + Metal for the
grid, talking to the same daemon. Ghostty itself is built this way: its terminal
surface is an `NSView`, bridged into SwiftUI with `NSViewRepresentable`, because
SwiftUI cannot draw a character grid at frame rate.

## Building

Requires a Rust toolchain. **Zig is not needed** — upstream ships a prebuilt
universal xcframework:

```sh
./vendor/fetch.sh   # downloads libghostty-vt
cargo build --release
```

Then put `target/release/keep` and `target/release/keepd` on your `PATH`.
The client starts the daemon on demand; you never run `keepd` yourself.

```sh
keep              # pick a workspace, or type a name to start one
keep myproject    # attach to myproject, creating it if needed
keep ls           # list workspaces
keep kill name    # end one
```

Inside a workspace, `ctrl-\` detaches and leaves everything running.

libghostty-vt has no stable ABI yet and its only release tag is `tip`. The
vendored copy is pinned on purpose, and `keep-vt` asserts the C struct sizes it
was built against — if those tests fail after re-vendoring, read the headers
before changing the numbers.

## License

MIT
