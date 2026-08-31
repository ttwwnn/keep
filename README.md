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
a sidebar toggle and the tab row all share the titlebar (an approach adapted
from Ghostty's titlebar tabs). The workspace sidebar runs the full height of
the window Finder-style and the tab row starts at its edge — collapse it and
the tabs slide over to the toggle. Each tab is a real libghostty surface
(Metal rendering, your own Ghostty fonts and theme) running the `keep` client.

The tab row is drawn by the app rather than by AppKit. Native tabs are windows
sharing a `tabbingIdentifier`, which made every workspace switch a matter of
creating and closing windows — the source of every flicker, focus loss and
fight with a tiling window manager the app ever had. Switching is now a
visibility flip between mounted views, and ⌘1–⌘9, drag-to-reorder and the
close buttons are ours.

**Windows are yours to ask for.** ⌘⇧N opens another, ⌥⌘W closes one, a tab
dragged clear of the row opens in one of its own, and nothing else in the app
makes or unmakes a window. Each carries its own list
of workspaces — a new one starts empty, and ⌘P reaches every workspace there
is and brings it into that window. Two windows can show the same tab; it is
the same shell with two viewers, the way two clients attach to one tmux
session, and the daemon fits the tab to the smaller of them. Closing a window
kills nothing; closing the last one quits, and everything keeps running in the
daemon.

Splits are a tree (⌘D right, ⌘⇧D down) and the daemon hands a closed pane's
children to its parent, so closing one pane never takes the arrangement apart.
There is a picker (⌘P) over every workspace and tab, search (⌘F, ⌘⇧F), and
keyboard motion between panes, workspaces and picker rows.

Tabs are labelled with the title the program inside sets (OSC 0/2), which
shells and editors do on their own, so a tab says what it is without anyone
naming it. A workspace also reports whether it is *running* something, taken
from the terminal's foreground process group rather than from shell
integration, so a build in a workspace nobody is watching still says so.

The same process group answers two more questions the picker needs, because a
title is not always a name: run the same tool in six tabs and six rows say the
same sentence. So the daemon also reports each tab's working directory — asked
of the kernel, so it needs no shell setup and follows a `cd` rather than
waiting for the next prompt — and when a byte last went either way through it.
The picker shows the directory relative to the workspace's own, which is short
enough to sit in a column: `777leads/api` rather than `~/www/777leads/api`.

One wrinkle worth knowing about: the libghostty build we link against accepts
the per-surface `command`, `env_vars` and `initial_input` fields and then
ignores them. Only `working_directory` survives, so the app fixes the client
app-wide and hands each surface its target through a file in a private
working directory, which the client reads and deletes.

Another, this one by design: a tab shown in two windows answers a program's
questions twice. When something asks the terminal where the cursor is or what
it is, the daemon passes the question to every client and each replies on the
one PTY. Mirroring a tab costs you that.

Not done yet: scrollback (the repaint covers the visible screen only), and
workspaces do not survive a reboot.

## Building

Requires a Rust toolchain. On macOS, **Zig is not needed** — upstream ships a
prebuilt universal xcframework:

```sh
./vendor/fetch.sh   # downloads libghostty-vt (and builds the Linux slices if zig is present)
cargo build --release
```

### Linux

The daemon and the client run on Linux — that half of Keep is the product
there, the way tmux is. Upstream ships no prebuilt Linux library, so the
build needs Zig (which also serves as the cross-linker, meaning the whole
thing builds *from a Mac* with no container):

```sh
brew install zig cargo-zigbuild   # or your distro's zig ≥ 0.16
tools/build-linux.sh              # → dist/linux-{x86_64,aarch64}/keep, keepd
```

On a Linux machine, `./vendor/fetch.sh && cargo build --release` does the
same natively. The macOS app does not port; on Linux you attach from any
terminal, which is what the client is for.

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
