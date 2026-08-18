//! The session layer against real PTYs and real child processes.

use std::time::{Duration, Instant};

use keepd::Session;
use portable_pty::CommandBuilder;

/// Poll until `cond` holds. PTY output is asynchronous, so tests must wait on
/// a condition rather than on a fixed sleep.
fn wait_for(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

fn shell() -> CommandBuilder {
    let mut cmd = CommandBuilder::new("/bin/sh");
    cmd.env("PS1", "$ ");
    cmd
}

fn screen_contains(session: &Session, needle: &str) -> bool {
    session.screen_text().map(|s| s.contains(needle)).unwrap_or(false)
}

#[test]
fn captures_child_output() {
    let mut cmd = CommandBuilder::new("/bin/echo");
    cmd.arg("hello-from-pty");
    let session = Session::spawn(cmd, 40, 6).expect("spawn");

    assert!(
        wait_for(Duration::from_secs(5), || screen_contains(&session, "hello-from-pty")),
        "child output never reached the screen: {:?}",
        session.screen_text()
    );
    assert!(
        wait_for(Duration::from_secs(5), || session.is_finished()),
        "session never reported the child exiting"
    );
}

#[test]
fn forwards_input_to_child() {
    let session = Session::spawn(shell(), 60, 10).expect("spawn");

    // 42 appears only in the output, never in the echoed command text.
    session.send(b"echo $((6*7))\n").expect("send");

    assert!(
        wait_for(Duration::from_secs(5), || screen_contains(&session, "42")),
        "shell never ran the command: {:?}",
        session.screen_text()
    );
    session.kill().ok();
}

#[test]
fn repaint_carries_styling_that_plain_text_drops() {
    let session = Session::spawn(shell(), 60, 10).expect("spawn");
    session
        .send(b"printf '\\033[1;31mDANGER\\033[0m\\n'\n")
        .expect("send");

    assert!(
        wait_for(Duration::from_secs(5), || screen_contains(&session, "DANGER")),
        "output never appeared"
    );

    let plain = session.screen_text().expect("text");
    let vt = session.repaint().expect("repaint");

    // Plain text loses the color; the repaint stream must keep it, because
    // that stream is what an attaching client gets painted with.
    assert!(!plain.contains('\u{1b}'), "plain text should carry no escapes");
    assert!(
        vt.windows(2).any(|w| w == b"\x1b["),
        "repaint stream carries no escape sequences"
    );
    session.kill().ok();
}

/// The reason this project exists: work continues while nobody is attached,
/// and the state is all there when someone looks again.
#[test]
fn session_keeps_running_with_no_one_watching() {
    let session = Session::spawn(shell(), 60, 20).expect("spawn");

    session
        .send(b"for i in 1 2 3 4; do echo tick-$i; sleep 0.15; done; echo fin$((20+2))\n")
        .expect("send");

    // Simulate a detached client: read nothing at all while the work happens.
    std::thread::sleep(Duration::from_millis(300));

    // Now "attach" and check the whole run is there. The sentinel is computed
    // by the shell so it cannot appear in the echoed command line — matching
    // the echo would let this pass before the work actually ran.
    assert!(
        wait_for(Duration::from_secs(5), || screen_contains(&session, "fin22")),
        "work did not finish: {:?}",
        session.screen_text()
    );

    let screen = session.screen_text().expect("text");
    for i in 1..=4 {
        assert!(screen.contains(&format!("tick-{i}")), "lost tick-{i}: {screen:?}");
    }
    session.kill().ok();
}

/// Attaching must be a single atomic step: the snapshot and the subscription
/// happen under one lock. If they did not, a client would either miss output
/// produced between them, or see output the snapshot already showed.
#[test]
fn attach_has_no_gap_and_no_duplicate() {
    let session = Session::spawn(shell(), 60, 20).expect("spawn");

    session.send(b"echo before$((1+1))\n").expect("send");
    assert!(
        wait_for(Duration::from_secs(5), || screen_contains(&session, "before2")),
        "setup output never appeared"
    );

    let (repaint_bytes, attachment) = session.attach().expect("attach");
    let repaint = String::from_utf8_lossy(&repaint_bytes).into_owned();
    assert!(repaint.contains("before2"), "repaint missed prior output");

    session.send(b"echo after$((2+2))\n").expect("send");

    // Collect what the feed delivers after the snapshot.
    let mut streamed = String::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        match attachment.output.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => {
                streamed.push_str(&String::from_utf8_lossy(&chunk));
                if streamed.contains("after4") {
                    break;
                }
            }
            Err(_) => continue,
        }
    }

    assert!(streamed.contains("after4"), "live feed missed new output: {streamed:?}");
    assert!(
        !streamed.contains("before2"),
        "live feed replayed output already in the repaint: {streamed:?}"
    );
    session.kill().ok();
}

/// A departed client must stop being counted immediately, even if the session
/// produces no further output. Pruning only on a failed send would leave a
/// quiet session reporting watchers that left long ago.
#[test]
fn detaching_stops_counting_the_client_right_away() {
    let session = Session::spawn(shell(), 40, 10).expect("spawn");
    assert_eq!(session.attached_clients(), 0, "fresh session has no clients");

    let (_repaint, attachment) = session.attach().expect("attach");
    assert_eq!(session.attached_clients(), 1, "attach was not counted");

    drop(attachment);
    assert_eq!(
        session.attached_clients(),
        0,
        "client still counted after detaching from a silent session"
    );

    session.kill().ok();
}
