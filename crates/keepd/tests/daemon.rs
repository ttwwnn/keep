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
    ClientMsg::Attach { session: "demo".into(), tab: TAB_ANY, cols: 80, rows: 24 }
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
    ClientMsg::Attach { session: "demo".into(), tab: TAB_ANY, cols: 80, rows: 24 }
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
fn list_reports_live_sessions() {
    let path = start_daemon("list");

    let mut attached = UnixStream::connect(&path).unwrap();
    ClientMsg::Attach { session: "one".into(), tab: TAB_ANY, cols: 80, rows: 24 }
        .write(&mut attached)
        .unwrap();
    attached.set_read_timeout(Some(Duration::from_secs(5))).ok();
    ServerMsg::read(&mut attached).unwrap();

    let mut lister = UnixStream::connect(&path).unwrap();
    ClientMsg::List.write(&mut lister).unwrap();
    lister.set_read_timeout(Some(Duration::from_secs(5))).ok();

    match ServerMsg::read(&mut lister).unwrap() {
        Some(ServerMsg::Sessions(list)) => {
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
    ClientMsg::Attach { session: "shared".into(), tab: TAB_ANY, cols: 80, rows: 24 }.write(&mut a).unwrap();
    a.set_read_timeout(Some(Duration::from_secs(5))).ok();
    ServerMsg::read(&mut a).unwrap();

    ClientMsg::Input(b"echo mark$((8+1))\n".to_vec()).write(&mut a).unwrap();
    assert!(read_until(&mut a, "mark9", Duration::from_secs(10)).contains("mark9"));

    // A second client on the same name must land in the same shell, seeing
    // the first client's work in its repaint.
    let mut b = UnixStream::connect(&path).unwrap();
    ClientMsg::Attach { session: "shared".into(), tab: TAB_ANY, cols: 80, rows: 24 }.write(&mut b).unwrap();
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
    ClientMsg::Attach { session: "quiet".into(), tab: TAB_ANY, cols: 80, rows: 24 }.write(&mut client).unwrap();
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
        Ok(Some(ServerMsg::Sessions(list))) => {
            list.iter().find(|s| s.name == name).map(|s| s.clients()).unwrap_or(0)
        }
        _ => 0,
    }
}

/// Tabs in one session are independent terminals, and both persist.
#[test]
fn tabs_are_independent_and_both_survive() {
    let path = start_daemon("tabs");

    // First tab, via a plain attach.
    let mut one = UnixStream::connect(&path).unwrap();
    ClientMsg::Attach { session: "proj".into(), tab: TAB_ANY, cols: 80, rows: 24 }
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

    // Ask for a second tab in the same session.
    let mut opener = UnixStream::connect(&path).unwrap();
    ClientMsg::NewTab { session: "proj".into(), cwd: None, cols: 80, rows: 24 }
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
    ClientMsg::Attach { session: "proj".into(), tab: second_id, cols: 80, rows: 24 }
        .write(&mut two)
        .unwrap();
    two.set_read_timeout(Some(Duration::from_secs(5))).ok();
    ServerMsg::read(&mut two).unwrap();
    read_repaint(&mut two);
    ClientMsg::Input(b"echo two$((1+1))\n".to_vec()).write(&mut two).unwrap();
    assert!(read_until(&mut two, "two2", Duration::from_secs(10)).contains("two2"));

    // One session, two tabs.
    let mut lister = UnixStream::connect(&path).unwrap();
    ClientMsg::List.write(&mut lister).unwrap();
    lister.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let sessions = match ServerMsg::read(&mut lister).unwrap() {
        Some(ServerMsg::Sessions(list)) => list,
        other => panic!("expected list, got {other:?}"),
    };
    let proj = sessions.iter().find(|s| s.name == "proj").expect("session missing");
    assert_eq!(proj.tabs.len(), 2, "expected two tabs, got {:?}", proj.tabs);

    // Each tab kept its own screen: neither shows the other's output.
    drop(one);
    drop(two);
    std::thread::sleep(Duration::from_millis(200));

    let mut back = UnixStream::connect(&path).unwrap();
    ClientMsg::Attach { session: "proj".into(), tab: first_id, cols: 80, rows: 24 }
        .write(&mut back)
        .unwrap();
    back.set_read_timeout(Some(Duration::from_secs(5))).ok();
    ServerMsg::read(&mut back).unwrap();
    let screen = String::from_utf8_lossy(&read_repaint(&mut back).expect("repaint")).into_owned();
    assert!(screen.contains("one1"), "first tab lost its own work: {screen:?}");
    assert!(!screen.contains("two2"), "first tab shows the second tab's output: {screen:?}");
}
