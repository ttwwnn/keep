//! End-to-end over a real unix socket: attach, work, disconnect, reattach.

use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

use keep_proto::{ClientMsg, ServerMsg};
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
    ClientMsg::Attach { name: "demo".into(), cols: 80, rows: 24 }
        .write(&mut c1)
        .unwrap();

    // The repaint arrives first, even for a brand new session.
    c1.set_read_timeout(Some(Duration::from_secs(5))).ok();
    match ServerMsg::read(&mut c1).unwrap() {
        Some(ServerMsg::Repaint(_)) => {}
        other => panic!("expected repaint first, got {other:?}"),
    }

    ClientMsg::Input(b"echo alpha$((3+4))\n".to_vec()).write(&mut c1).unwrap();
    let seen = read_until(&mut c1, "alpha7", Duration::from_secs(10));
    assert!(seen.contains("alpha7"), "shell never ran the command: {seen:?}");

    // --- client disappears, session must not ---
    drop(c1);
    std::thread::sleep(Duration::from_millis(200));

    // --- second client attaches to the same name ---
    let mut c2 = UnixStream::connect(&path).expect("reconnect");
    ClientMsg::Attach { name: "demo".into(), cols: 80, rows: 24 }
        .write(&mut c2)
        .unwrap();
    c2.set_read_timeout(Some(Duration::from_secs(5))).ok();

    let repaint = match ServerMsg::read(&mut c2).unwrap() {
        Some(ServerMsg::Repaint(data)) => String::from_utf8_lossy(&data).into_owned(),
        other => panic!("expected repaint, got {other:?}"),
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
    ClientMsg::Attach { name: "one".into(), cols: 80, rows: 24 }
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
            assert_eq!(one.clients, 1, "attached client not counted");
            assert!(!one.finished);
        }
        other => panic!("expected session list, got {other:?}"),
    }
}

#[test]
fn attaching_twice_reuses_one_shell() {
    let path = start_daemon("reuse");

    let mut a = UnixStream::connect(&path).unwrap();
    ClientMsg::Attach { name: "shared".into(), cols: 80, rows: 24 }.write(&mut a).unwrap();
    a.set_read_timeout(Some(Duration::from_secs(5))).ok();
    ServerMsg::read(&mut a).unwrap();

    ClientMsg::Input(b"echo mark$((8+1))\n".to_vec()).write(&mut a).unwrap();
    assert!(read_until(&mut a, "mark9", Duration::from_secs(10)).contains("mark9"));

    // A second client on the same name must land in the same shell, seeing
    // the first client's work in its repaint.
    let mut b = UnixStream::connect(&path).unwrap();
    ClientMsg::Attach { name: "shared".into(), cols: 80, rows: 24 }.write(&mut b).unwrap();
    b.set_read_timeout(Some(Duration::from_secs(5))).ok();

    match ServerMsg::read(&mut b).unwrap() {
        Some(ServerMsg::Repaint(data)) => {
            let s = String::from_utf8_lossy(&data);
            assert!(s.contains("mark9"), "second client got a different shell: {s:?}");
        }
        other => panic!("expected repaint, got {other:?}"),
    }
}

/// After a client disconnects the daemon must stop counting it, even though
/// the session is silent and never triggers a failed write.
#[test]
fn disconnected_client_stops_being_counted() {
    let path = start_daemon("count");

    let mut client = UnixStream::connect(&path).unwrap();
    ClientMsg::Attach { name: "quiet".into(), cols: 80, rows: 24 }.write(&mut client).unwrap();
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

fn clients_of(path: &std::path::Path, name: &str) -> u32 {
    let mut sock = UnixStream::connect(path).unwrap();
    ClientMsg::List.write(&mut sock).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(5))).ok();
    match ServerMsg::read(&mut sock) {
        Ok(Some(ServerMsg::Sessions(list))) => {
            list.iter().find(|s| s.name == name).map(|s| s.clients).unwrap_or(0)
        }
        _ => 0,
    }
}
