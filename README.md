# keep

Persistent terminal sessions, built on [libghostty-vt](https://libghostty.tip.ghostty.org/).

> Working name. Early development — nothing here is usable yet.

## Why

Terminal emulators lose your work. Close the window and every process dies with
it. `tmux` solves that, but it also brings a hierarchy — sessions, then windows,
then panes — that you have to navigate even when you never split a pane.

`keep` takes the other half of the trade: real persistence, one flat list. A
session is a project. You switch with a fuzzy finder, not by remembering which
window index holds which task.

## How

A terminal that owns your processes cannot be the same thing as the window you
close. So `keep` splits them:

    ┌─ daemon ──────────────────────────────────┐
    │  owns the PTYs, outlives every client     │
    │  libghostty-vt keeps each session's grid  │
    └───────────────────────────────────────────┘
                        ▲  unix socket
            ┌───────────┴───────────┐
            │ client (attach/detach)│
            └───────────────────────┘

Because the daemon holds the screen state, a client that reconnects can be
repainted exactly as the session left it — `libghostty-vt` can emit the whole
screen back as VT sequences, colors and hyperlinks included.

The core is portable and knows nothing about UI. Each platform gets a real
native client instead of a lowest-common-denominator one.

## Status

**Phase 1 — daemon + terminal client.** The client runs inside an existing
terminal and paints by writing VT sequences, so there is no renderer and no
font handling to build. This is enough for persistence and session switching.

- [x] `keep-vt` — safe Rust bindings for libghostty-vt
- [~] `keepd` — PTY ownership and screen state done; session registry and
      socket protocol still missing
- [ ] `keep` — client: attach, detach, repaint, input forwarding

**Phase 2 — native macOS client.** SwiftUI for chrome, `NSView` + Metal for the
grid, talking to the same daemon. Ghostty itself is built this way: its terminal
surface is an `NSView`, bridged into SwiftUI with `NSViewRepresentable`, because
SwiftUI cannot draw a character grid at frame rate.

## Building

Requires a Rust toolchain. **Zig is not needed** — upstream ships a prebuilt
universal xcframework:

```sh
./vendor/fetch.sh   # downloads libghostty-vt
cargo test
```

libghostty-vt has no stable ABI yet and its only release tag is `tip`. The
vendored copy is pinned on purpose, and `keep-vt` asserts the C struct sizes it
was built against — if those tests fail after re-vendoring, read the headers
before changing the numbers.

## License

MIT
