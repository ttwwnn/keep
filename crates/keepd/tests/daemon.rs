//! End-to-end over a real unix socket: attach, work, disconnect, reattach.

use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

use keep_proto::{ClientMsg, ServerMsg, TAB_ANY};
use keepd::Server;

/// Start a daemon on a private socket. Returns the path; the server thread
/// lives for the rest of the test process.
fn start_daemon(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("keep-test-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("keep.sock");

    let server = Arc::new(Server::bind(&path).expect("bind"));
    let run = Arc::clone(&server);
    std::thread::spawn(move || {
        let _ = run.run();
    });

    // Wait until it actually accepts.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if UnixStream::connect(&path).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    // Keep the server alive for the process lifetime; dropping it unlinks
    // the socket.
    std::mem::forget(server);
    path
}

/// Read frames until `needle` shows up in the accumulated output.
fn read_until(stream: &mut UnixStream, needle: &str, timeout: Duration) -> String {
    let mut seen = String::new();
    stream.set_read_timeout(Some(Duration::from_millis(200))).ok();
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match ServerMsg::read(stream) {
            Ok(Some(ServerMsg::Output(data))) | Ok(Some(ServerMsg::Repaint(data))) => {
                seen.push_str(&String::from_utf8_lossy(&data));
                if seen.contains(needle) {
                    return seen;
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    seen
}

#[test]
fn work_survives_the_client_going_away() {
    let path = start_daemon("survive");

    // --- first client attaches and does some work ---
    let mut c1 = UnixStream::connect(&path).expect("connect");
    ClientMsg::Attach { workspace: "demo".into(), tab: TAB_ANY, cols: 80, rows: 24 }
        .write(&mut c1)
        .unwrap();

    // The daemon names the tab it chose, then repaints it.
    c1.set_read_timeout(Some(Duration::from_secs(5))).ok();
    match ServerMsg::read(&mut c1).unwrap() {
        Some(ServerMsg::Attached { tab }) => assert!(tab > 0, "tab id should be assigned"),
        other => panic!("expected attached first, got {other:?}"),
    }
    match ServerMsg::read(&mut c1).unwrap() {
        Some(ServerMsg::Repaint(_)) => {}
        other => panic!("expected repaint after attached, got {other:?}"),
    }

    ClientMsg::Input(b"echo alpha$((3+4))\n".to_vec()).write(&mut c1).unwrap();
    let seen = read_until(&mut c1, "alpha7", Duration::from_secs(10));
    assert!(seen.contains("alpha7"), "shell never ran the command: {seen:?}");

    // --- client disappears, session must not ---
    drop(c1);
    std::thread::sleep(Duration::from_millis(200));

    // --- second client attaches to the same name ---
    let mut c2 = UnixStream::connect(&path).expect("reconnect");
    ClientMsg::Attach { workspace: "demo".into(), tab: TAB_ANY, cols: 80, rows: 24 }
        .write(&mut c2)
        .unwrap();
    c2.set_read_timeout(Some(Duration::from_secs(5))).ok();

    let repaint = match read_repaint(&mut c2) {
        Some(data) => String::from_utf8_lossy(&data).into_owned(),
        None => panic!("no repaint on reattach"),
    };
    assert!(
        repaint.contains("alpha7"),
        "reattach lost the earlier work: {repaint:?}"
    );

    // And the same shell is still there, holding its state.
    ClientMsg::Input(b"echo beta$((5+5))\n".to_vec()).write(&mut c2).unwrap();
    let seen = read_until(&mut c2, "beta10", Duration::from_secs(10));
    assert!(seen.contains("beta10"), "reattached shell is dead: {seen:?}");
}

#[test]
fn list_reports_live_workspaces() {
    let path = start_daemon("list");

    let mut attached = UnixStream::connect(&path).unwrap();
    ClientMsg::Attach { workspace: "one".into(), tab: TAB_ANY, cols: 80, rows: 24 }
        .write(&mut attached)
        .unwrap();
    attached.set_read_timeout(Some(Duration::from_secs(5))).ok();
    ServerMsg::read(&mut attached).unwrap();

    let mut lister = UnixStream::connect(&path).unwrap();
    ClientMsg::List.write(&mut lister).unwrap();
    lister.set_read_timeout(Some(Duration::from_secs(5))).ok();

    match ServerMsg::read(&mut lister).unwrap() {
        Some(ServerMsg::Workspaces(list)) => {
            let names: Vec<_> = list.iter().map(|s| s.name.as_str()).collect();
            assert!(names.contains(&"one"), "session missing from list: {names:?}");
            let one = list.iter().find(|s| s.name == "one").unwrap();
            assert_eq!(one.clients(), 1, "attached client not counted");
            assert_eq!(one.tabs.len(), 1, "attach should have opened exactly one tab");
            assert!(!one.tabs[0].finished);
        }
        other => panic!("expected session list, got {other:?}"),
    }
}

#[test]
fn attaching_twice_reuses_one_shell() {
    let path = start_daemon("reuse");

    let mut a = UnixStream::connect(&path).unwrap();
    ClientMsg::Attach { workspace: "shared".into(), tab: TAB_ANY, cols: 80, rows: 24 }.write(&mut a).unwrap();
    a.set_read_timeout(Some(Duration::from_secs(5))).ok();
    ServerMsg::read(&mut a).unwrap();

    ClientMsg::Input(b"echo mark$((8+1))\n".to_vec()).write(&mut a).unwrap();
    assert!(read_until(&mut a, "mark9", Duration::from_secs(10)).contains("mark9"));

    // A second client on the same name must land in the same shell, seeing
    // the first client's work in its repaint.
    let mut b = UnixStream::connect(&path).unwrap();
    ClientMsg::Attach { workspace: "shared".into(), tab: TAB_ANY, cols: 80, rows: 24 }.write(&mut b).unwrap();
    b.set_read_timeout(Some(Duration::from_secs(5))).ok();

    let data = read_repaint(&mut b).expect("no repaint for second client");
    let s = String::from_utf8_lossy(&data);
    assert!(s.contains("mark9"), "second client got a different shell: {s:?}");
}

/// After a client disconnects the daemon must stop counting it, even though
/// the session is silent and never triggers a failed write.
#[test]
fn disconnected_client_stops_being_counted() {
    let path = start_daemon("count");

    let mut client = UnixStream::connect(&path).unwrap();
    ClientMsg::Attach { workspace: "quiet".into(), tab: TAB_ANY, cols: 80, rows: 24 }.write(&mut client).unwrap();
    client.set_read_timeout(Some(Duration::from_secs(5))).ok();
    ServerMsg::read(&mut client).unwrap();

    assert_eq!(clients_of(&path, "quiet"), 1, "attached client not counted");

    drop(client);

    // The pump notices on its next tick; give it a few.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && clients_of(&path, "quiet") != 0 {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        clients_of(&path, "quiet"),
        0,
        "daemon still reports a client that disconnected"
    );
}

/// Skip past the `Attached` frame and return the repaint payload.
fn read_repaint(stream: &mut UnixStream) -> Option<Vec<u8>> {
    for _ in 0..4 {
        match ServerMsg::read(stream) {
            Ok(Some(ServerMsg::Repaint(data))) => return Some(data),
            Ok(Some(_)) => continue,
            _ => return None,
        }
    }
    None
}

fn clients_of(path: &std::path::Path, name: &str) -> u32 {
    let mut sock = UnixStream::connect(path).unwrap();
    ClientMsg::List.write(&mut sock).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(5))).ok();
    match ServerMsg::read(&mut sock) {
        Ok(Some(ServerMsg::Workspaces(list))) => {
            list.iter().find(|s| s.name == name).map(|s| s.clients()).unwrap_or(0)
        }
        _ => 0,
    }
}

/// Tabs in one workspace are independent terminals, and both persist.
#[test]
fn tabs_are_independent_and_both_survive() {
    let path = start_daemon("tabs");

    // First tab, via a plain attach.
    let mut one = UnixStream::connect(&path).unwrap();
    ClientMsg::Attach { workspace: "proj".into(), tab: TAB_ANY, cols: 80, rows: 24 }
        .write(&mut one)
        .unwrap();
    one.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let first_id = match ServerMsg::read(&mut one).unwrap() {
        Some(ServerMsg::Attached { tab }) => tab,
        other => panic!("expected attached, got {other:?}"),
    };
    read_repaint(&mut one);
    ClientMsg::Input(b"echo one$((1+0))\n".to_vec()).write(&mut one).unwrap();
    assert!(read_until(&mut one, "one1", Duration::from_secs(10)).contains("one1"));

    // Ask for a second tab in the same workspace.
    let mut opener = UnixStream::connect(&path).unwrap();
    ClientMsg::NewTab {
        workspace: "proj".into(),
        cwd: None,
        cols: 80,
        rows: 24,
        split_of: TAB_ANY,
        split_dir: 0,
    }
    .write(&mut opener)
    .unwrap();
    opener.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let second_id = match ServerMsg::read(&mut opener).unwrap() {
        Some(ServerMsg::TabCreated { tab }) => tab,
        other => panic!("expected tab created, got {other:?}"),
    };
    assert_ne!(first_id, second_id, "second tab reused the first tab's id");

    // Work in the second tab.
    let mut two = UnixStream::connect(&path).unwrap();
    ClientMsg::Attach { workspace: "proj".into(), tab: second_id, cols: 80, rows: 24 }
        .write(&mut two)
        .unwrap();
    two.set_read_timeout(Some(Duration::from_secs(5))).ok();
    ServerMsg::read(&mut two).unwrap();
    read_repaint(&mut two);
    ClientMsg::Input(b"echo two$((1+1))\n".to_vec()).write(&mut two).unwrap();
    assert!(read_until(&mut two, "two2", Duration::from_secs(10)).contains("two2"));

    // One workspace, two tabs.
    let mut lister = UnixStream::connect(&path).unwrap();
    ClientMsg::List.write(&mut lister).unwrap();
    lister.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let workspaces = match ServerMsg::read(&mut lister).unwrap() {
        Some(ServerMsg::Workspaces(list)) => list,
        other => panic!("expected list, got {other:?}"),
    };
    let proj = workspaces.iter().find(|s| s.name == "proj").expect("workspace missing");
    assert_eq!(proj.tabs.len(), 2, "expected two tabs, got {:?}", proj.tabs);

    // Each tab kept its own screen: neither shows the other's output.
    drop(one);
    drop(two);
    std::thread::sleep(Duration::from_millis(200));

    let mut back = UnixStream::connect(&path).unwrap();
    ClientMsg::Attach { workspace: "proj".into(), tab: first_id, cols: 80, rows: 24 }
        .write(&mut back)
        .unwrap();
    back.set_read_timeout(Some(Duration::from_secs(5))).ok();
    ServerMsg::read(&mut back).unwrap();
    let screen = String::from_utf8_lossy(&read_repaint(&mut back).expect("repaint")).into_owned();
    assert!(screen.contains("one1"), "first tab lost its own work: {screen:?}");
    assert!(!screen.contains("two2"), "first tab shows the second tab's output: {screen:?}");
}

/// The title a program sets with OSC has to reach the workspace list, because
/// that is what labels a tab you are not currently looking at.
#[test]
fn tab_title_reaches_the_workspace_list() {
    let path = start_daemon("title");

    let mut client = UnixStream::connect(&path).unwrap();
    ClientMsg::Attach { workspace: "titled".into(), tab: TAB_ANY, cols: 80, rows: 24 }
        .write(&mut client)
        .unwrap();
    client.set_read_timeout(Some(Duration::from_secs(5))).ok();
    ServerMsg::read(&mut client).unwrap();
    read_repaint(&mut client);

    // Interactive shells retitle on every prompt, so the sleep holds our
    // title long enough to observe. That the shell competes here is the point:
    // real tabs get labelled without anyone doing anything.
    ClientMsg::Input(b"printf '\\033]2;building orion\\007'; sleep 3\n".to_vec())
        .write(&mut client)
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut seen = String::new();
    while Instant::now() < deadline {
        if let Some(t) = title_of(&path, "titled") {
            seen = t;
            if seen == "building orion" {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(seen, "building orion", "title never reached the list");
}

fn title_of(path: &std::path::Path, workspace: &str) -> Option<String> {
    let mut sock = UnixStream::connect(path).ok()?;
    ClientMsg::List.write(&mut sock).ok()?;
    sock.set_read_timeout(Some(Duration::from_secs(5))).ok();
    match ServerMsg::read(&mut sock) {
        Ok(Some(ServerMsg::Workspaces(list))) => list
            .iter()
            .find(|s| s.name == workspace)
            .and_then(|s| s.tabs.first())
            .map(|t| t.title.clone()),
        _ => None,
    }
}

/// The split arrangement lives in the daemon, so a client that reconnects can
/// rebuild the same layout. A pane whose base tab dies is promoted to a
/// standalone tab rather than left pointing at nothing.
#[test]
fn splits_are_recorded_and_orphans_promoted() {
    let path = start_daemon("split");

    // Base tab.
    let mut base = UnixStream::connect(&path).unwrap();
    ClientMsg::Attach { workspace: "dev".into(), tab: TAB_ANY, cols: 80, rows: 24 }
        .write(&mut base)
        .unwrap();
    base.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let base_id = match ServerMsg::read(&mut base).unwrap() {
        Some(ServerMsg::Attached { tab }) => tab,
        other => panic!("expected attached, got {other:?}"),
    };

    // A pane split to the right of it.
    let mut opener = UnixStream::connect(&path).unwrap();
    ClientMsg::NewTab {
        workspace: "dev".into(),
        cwd: None,
        cols: 80,
        rows: 24,
        split_of: base_id,
        split_dir: keep_proto::SPLIT_RIGHT,
    }
    .write(&mut opener)
    .unwrap();
    opener.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let pane_id = match ServerMsg::read(&mut opener).unwrap() {
        Some(ServerMsg::TabCreated { tab }) => tab,
        other => panic!("expected tab created, got {other:?}"),
    };

    // Splitting a tab that does not exist must be refused, not recorded.
    let mut bad = UnixStream::connect(&path).unwrap();
    ClientMsg::NewTab {
        workspace: "dev".into(),
        cwd: None,
        cols: 80,
        rows: 24,
        split_of: 9999,
        split_dir: keep_proto::SPLIT_RIGHT,
    }
    .write(&mut bad)
    .unwrap();
    bad.set_read_timeout(Some(Duration::from_secs(5))).ok();
    match ServerMsg::read(&mut bad).unwrap() {
        Some(ServerMsg::Error(_)) => {}
        other => panic!("expected an error for a dangling split, got {other:?}"),
    }

    // The list carries the arrangement.
    let tabs = tabs_of(&path, "dev");
    let pane = tabs.iter().find(|t| t.id == pane_id).expect("pane missing");
    assert_eq!(pane.split_of, base_id);
    assert_eq!(pane.split_dir, keep_proto::SPLIT_RIGHT);

    // Close the base; the pane must be promoted, not orphaned.
    let mut closer = UnixStream::connect(&path).unwrap();
    ClientMsg::CloseTab { workspace: "dev".into(), tab: base_id }.write(&mut closer).unwrap();
    closer.set_read_timeout(Some(Duration::from_secs(5))).ok();
    ServerMsg::read(&mut closer).unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut promoted = false;
    while Instant::now() < deadline {
        let tabs = tabs_of(&path, "dev");
        if let Some(pane) = tabs.iter().find(|t| t.id == pane_id) {
            if pane.split_of == TAB_ANY {
                promoted = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(promoted, "pane still points at the closed base tab");
}

fn tabs_of(path: &std::path::Path, workspace: &str) -> Vec<keep_proto::TabInfo> {
    let mut sock = UnixStream::connect(path).unwrap();
    ClientMsg::List.write(&mut sock).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(5))).ok();
    match ServerMsg::read(&mut sock) {
        Ok(Some(ServerMsg::Workspaces(list))) => list
            .into_iter()
            .find(|s| s.name == workspace)
            .map(|s| s.tabs)
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The size of a workspace's first tab, as the daemon reports it.
fn size_of(path: &std::path::Path, workspace: &str) -> (u16, u16) {
    tabs_of(path, workspace).first().map(|t| (t.cols, t.rows)).unwrap_or((0, 0))
}

/// Wait for a size, so a test does not race the negotiation.
fn size_settles(path: &std::path::Path, workspace: &str, want: (u16, u16)) -> (u16, u16) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut seen = size_of(path, workspace);
    while Instant::now() < deadline && seen != want {
        std::thread::sleep(Duration::from_millis(50));
        seen = size_of(path, workspace);
    }
    seen
}

fn attach(path: &std::path::Path, workspace: &str, cols: u16, rows: u16) -> UnixStream {
    let mut sock = UnixStream::connect(path).unwrap();
    ClientMsg::Attach { workspace: workspace.into(), tab: TAB_ANY, cols, rows }
        .write(&mut sock)
        .unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(5))).ok();
    ServerMsg::read(&mut sock).unwrap();
    sock
}

/// One viewer still gets exactly what it asks for.
///
/// The guard on everything below: a minimum computed over a stale or default
/// entry would clamp the ordinary single-client case, and every other test
/// here would still pass.
#[test]
fn one_viewer_gets_what_it_asks_for() {
    let path = start_daemon("size-one");
    let mut a = attach(&path, "solo", 100, 30);
    assert_eq!(size_settles(&path, "solo", (100, 30)), (100, 30));

    ClientMsg::Resize { cols: 132, rows: 43 }.write(&mut a).unwrap();
    assert_eq!(size_settles(&path, "solo", (132, 43)), (132, 43));
}

/// Two viewers, and the tab fits the smaller — in both directions, whichever
/// order they arrive in.
#[test]
fn the_smallest_viewer_decides() {
    let path = start_daemon("size-min");

    let _big = attach(&path, "both", 200, 50);
    assert_eq!(size_settles(&path, "both", (200, 50)), (200, 50));

    // A smaller one joining shrinks the tab: the big viewer letterboxes,
    // which is the harmless direction.
    let _small = attach(&path, "both", 80, 24);
    assert_eq!(size_settles(&path, "both", (80, 24)), (80, 24));

    // And a bigger one joining does not stretch it back.
    let _bigger = attach(&path, "both", 300, 60);
    assert_eq!(size_settles(&path, "both", (80, 24)), (80, 24));
}

/// A resize from one viewer is still bounded by the other.
#[test]
fn a_resize_is_still_bounded_by_the_other_viewer() {
    let path = start_daemon("size-bound");
    let mut big = attach(&path, "bound", 200, 50);
    let _small = attach(&path, "bound", 80, 24);
    assert_eq!(size_settles(&path, "bound", (80, 24)), (80, 24));

    ClientMsg::Resize { cols: 300, rows: 70 }.write(&mut big).unwrap();
    assert_eq!(size_settles(&path, "bound", (80, 24)), (80, 24), "the big viewer stretched it");
}

/// When the small viewer leaves, the room it was taking comes back.
#[test]
fn the_size_returns_when_the_small_viewer_leaves() {
    let path = start_daemon("size-leave");
    let _big = attach(&path, "leave", 200, 50);
    let small = attach(&path, "leave", 80, 24);
    assert_eq!(size_settles(&path, "leave", (80, 24)), (80, 24));

    drop(small);
    assert_eq!(
        size_settles(&path, "leave", (200, 50)),
        (200, 50),
        "the tab stayed clamped to a viewer that left"
    );
}

/// A viewer that did not ask for the change is handed a whole screen.
///
/// It is holding one drawn for the old size, and there is no message that
/// says so — the daemon repays it the same way it repays a client that fell
/// behind, which is machinery that already exists and is already tested.
#[test]
fn a_size_change_repaints_the_other_viewer() {
    let path = start_daemon("size-repaint");
    let mut big = attach(&path, "repaint", 200, 50);
    ClientMsg::Input(b"echo before$((3+4))\n".to_vec()).write(&mut big).unwrap();
    assert!(read_until(&mut big, "before7", Duration::from_secs(10)).contains("before7"));

    let _small = attach(&path, "repaint", 80, 24);

    // The big viewer asked for nothing and must still be sent a screen.
    let seen = read_until(&mut big, "before7", Duration::from_secs(5));
    assert!(seen.contains("before7"), "no repaint reached the viewer that did not ask: {seen:?}");
}

/// A viewer that has looked away stops voting on the size.
///
/// The app keeps every tab a window has visited mounted, with a live client
/// on it, so a narrow window that once visited this tab would otherwise clamp
/// it forever while showing something else entirely. Zero means "not looking",
/// which `Tab::resize` already ignores, so it falls out of the minimum for
/// free.
#[test]
fn a_hidden_viewer_does_not_throttle_the_other() {
    let path = start_daemon("size-hidden");
    let _big = attach(&path, "hidden", 200, 50);
    let mut small = attach(&path, "hidden", 80, 24);
    assert_eq!(size_settles(&path, "hidden", (80, 24)), (80, 24));

    // The small window switched to another tab. Its client stays connected.
    ClientMsg::Resize { cols: 0, rows: 0 }.write(&mut small).unwrap();
    assert_eq!(
        size_settles(&path, "hidden", (200, 50)),
        (200, 50),
        "a viewer nobody can see is still clamping the tab"
    );

    // And it counts again the moment it is shown.
    ClientMsg::Resize { cols: 80, rows: 24 }.write(&mut small).unwrap();
    assert_eq!(size_settles(&path, "hidden", (80, 24)), (80, 24));
}

/// With everyone looking away the tab keeps the size it had.
///
/// Not a detail: reflowing to some default for an audience of nobody would
/// scramble the scrollback of a build the user comes back to read.
#[test]
fn nobody_looking_leaves_the_size_alone() {
    let path = start_daemon("size-nobody");
    let mut only = attach(&path, "nobody", 120, 40);
    assert_eq!(size_settles(&path, "nobody", (120, 40)), (120, 40));

    ClientMsg::Resize { cols: 0, rows: 0 }.write(&mut only).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(size_of(&path, "nobody"), (120, 40), "an unwatched tab reflowed itself");
}

/// A tab moves to another workspace with its shell still running.
///
/// Nothing restarts: the same PTY is simply filed under the other name with
/// an id of that workspace's own, which is what makes this safe to do to a
/// tab with hours of work in it.
#[test]
fn a_tab_moves_between_workspaces_with_its_shell() {
    let path = start_daemon("move");
    let mut sock = attach(&path, "origin", 100, 30);
    ClientMsg::Input(b"echo carried$((10+7))\n".to_vec()).write(&mut sock).unwrap();
    assert!(read_until(&mut sock, "carried17", Duration::from_secs(10)).contains("carried17"));

    let mut mover = UnixStream::connect(&path).unwrap();
    ClientMsg::MoveTab { workspace: "origin".into(), tab: 1, to: "target".into() }
        .write(&mut mover)
        .unwrap();
    mover.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let moved = match ServerMsg::read(&mut mover) {
        Ok(Some(ServerMsg::TabCreated { tab })) => tab,
        other => panic!("expected the new id, got {other:?}"),
    };

    // Gone from where it was, present where it went.
    assert!(tabs_of(&path, "origin").is_empty(), "still filed under its old name");
    let target = tabs_of(&path, "target");
    assert_eq!(target.len(), 1);
    assert_eq!(target[0].id, moved);

    // And it is the same shell: attaching repaints the history it made
    // before the move.
    let mut back = UnixStream::connect(&path).unwrap();
    ClientMsg::Attach { workspace: "target".into(), tab: moved, cols: 100, rows: 30 }
        .write(&mut back)
        .unwrap();
    back.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let seen = read_until(&mut back, "carried17", Duration::from_secs(10));
    assert!(seen.contains("carried17"), "the shell did not survive the move: {seen:?}");
}
