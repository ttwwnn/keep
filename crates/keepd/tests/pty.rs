//! The session layer against real PTYs and real child processes.

use std::time::{Duration, Instant};

use keepd::Tab;
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

fn screen_contains(session: &Tab, needle: &str) -> bool {
    session.screen_text().map(|s| s.contains(needle)).unwrap_or(false)
}

#[test]
fn captures_child_output() {
    let mut cmd = CommandBuilder::new("/bin/echo");
    cmd.arg("hello-from-pty");
    let session = Tab::spawn(cmd, 40, 6).expect("spawn");

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
    let session = Tab::spawn(shell(), 60, 10).expect("spawn");

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
    let session = Tab::spawn(shell(), 60, 10).expect("spawn");
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
fn tab_keeps_running_with_no_one_watching() {
    let session = Tab::spawn(shell(), 60, 20).expect("spawn");

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
    let session = Tab::spawn(shell(), 60, 20).expect("spawn");

    session.send(b"echo before$((1+1))\n").expect("send");
    assert!(
        wait_for(Duration::from_secs(5), || screen_contains(&session, "before2")),
        "setup output never appeared"
    );

    let (repaint_bytes, attachment) = session.attach(60, 20).expect("attach");
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
    let session = Tab::spawn(shell(), 40, 10).expect("spawn");
    assert_eq!(session.attached_clients(), 0, "fresh session has no clients");

    let (_repaint, attachment) = session.attach(40, 10).expect("attach");
    assert_eq!(session.attached_clients(), 1, "attach was not counted");

    drop(attachment);
    assert_eq!(
        session.attached_clients(),
        0,
        "client still counted after detaching from a silent session"
    );

    session.kill().ok();
}

/// A tab is busy while a command runs and quiet at the prompt. This is what
/// tells a workspace doing work apart from one merely sitting there, which is
/// the distinction the sidebar needs.
#[test]
fn busy_tracks_the_foreground_command() {
    let session = Tab::spawn(shell(), 60, 12).expect("spawn");

    // Let the shell reach its prompt before judging it.
    assert!(
        wait_for(Duration::from_secs(5), || screen_contains(&session, "$")
            || screen_contains(&session, "%")
            || screen_contains(&session, "❯")),
        "shell never showed a prompt: {:?}",
        session.screen_text()
    );
    assert!(!session.is_busy(), "a shell at its prompt is not busy");

    session.send(b"sleep 3\n").expect("send");
    assert!(
        wait_for(Duration::from_secs(5), || session.is_busy()),
        "running sleep did not register as busy"
    );

    // And it goes quiet again once the command finishes.
    assert!(
        wait_for(Duration::from_secs(10), || !session.is_busy()),
        "still busy after the command should have finished"
    );

    session.kill().ok();
}

/// A client that stops reading must not be able to grow the daemon without
/// bound — and must not be handed a stream with a hole in it either.
///
/// The queue has a ceiling. Once it is full the daemon drops chunks for that
/// client and records the fact; what the client gets in their place is the
/// whole screen, which is authoritative in a way the missing bytes were not.
#[test]
fn a_client_that_stops_reading_is_repainted_not_starved() {
    let session = Tab::spawn(shell(), 60, 20).expect("spawn");

    // Attach and then never touch `output`: this stands in for a client whose
    // socket has stopped draining.
    let (_repaint, attachment) = session.attach(60, 20).expect("attach");
    assert!(!attachment.overflowed(), "a fresh attachment has dropped nothing");

    // Far more output than the backlog can hold.
    session.send(b"seq 1 200000\n").expect("send");
    assert!(
        wait_for(Duration::from_secs(20), || attachment.overflowed()),
        "the queue never filled, so the drop path was never exercised"
    );

    // The grid keeps up regardless: falling behind costs the client its queued
    // chunks, never the tab's own state.
    session.send(b"echo sentinel$((7+6))\n").expect("send");
    assert!(
        wait_for(Duration::from_secs(20), || screen_contains(&session, "sentinel13")),
        "the tab stopped tracking its own screen: {:?}",
        session.screen_text()
    );

    let screen = attachment.resync().expect("resync");
    assert!(
        String::from_utf8_lossy(&screen).contains("sentinel13"),
        "the repaint missed output produced while the client was behind"
    );
    assert!(!attachment.overflowed(), "resync left the client still marked behind");
    assert!(
        attachment.output.try_recv().is_err(),
        "resync left stale chunks queued behind the repaint"
    );

    assert_eq!(session.attached_clients(), 1, "falling behind must not unsubscribe");
    session.kill().ok();
}

/// Closing one pane of a split must not scatter the panes below it.
///
/// Splits are a chain — each pane records the tab it was split from — so a
/// pane closed in the middle leaves its children pointing at something that
/// is gone. Reported standalone, they would become tabs of their own, and
/// closing one pane would take the window apart.
#[test]
fn closing_a_pane_hands_its_children_to_its_parent() {
    let workspace = keepd::Workspace::new("splits");
    let (root, _) = workspace.new_tab(None, 80, 24, 0, 0).expect("root");
    let (middle, _) = workspace.new_tab(None, 80, 24, root, 1).expect("middle pane");
    let (leaf, _) = workspace.new_tab(None, 80, 24, middle, 1).expect("leaf pane");

    workspace.close_tab(middle).expect("close the middle pane");

    let tabs = workspace.tabs();
    assert_eq!(tabs.len(), 2, "closing one pane closed more than one tab");
    let leaf_entry = tabs.iter().find(|t| t.id == leaf).expect("leaf survived");
    assert_eq!(
        leaf_entry.split_of, root,
        "the leaf was orphaned into a standalone tab instead of inheriting the root"
    );
    workspace.kill_all().ok();
}

/// Closing the root of a split tab must not scatter its panes into tabs.
///
/// A pane records the tab it was split from, and a root has nothing to be
/// split from — so panes of a closed root have no parent to inherit. Left
/// alone they are each reported standalone, which is one tab per pane. One of
/// them has to take the root's place instead.
#[test]
fn closing_a_root_promotes_one_pane_and_keeps_the_rest_with_it() {
    let workspace = keepd::Workspace::new("roots");
    let (root, _) = workspace.new_tab(None, 80, 24, 0, 0).expect("root");
    let (first, _) = workspace.new_tab(None, 80, 24, root, 1).expect("first pane");
    let (second, _) = workspace.new_tab(None, 80, 24, root, 2).expect("second pane");

    workspace.close_tab(root).expect("close the root");

    let tabs = workspace.tabs();
    assert_eq!(tabs.len(), 2, "closing the root closed more than itself");
    let roots: Vec<u32> = tabs.iter().filter(|t| t.split_of == 0).map(|t| t.id).collect();
    assert_eq!(roots, vec![first], "the arrangement scattered into standalone tabs");
    let sibling = tabs.iter().find(|t| t.id == second).expect("sibling survived");
    assert_eq!(sibling.split_of, first, "the sibling did not follow the promoted pane");
    workspace.kill_all().ok();
}

/// How much history the daemon can hand over.
///
/// The formatter documents its default as "the entire screen", and the point
/// API distinguishes the active area from the full screen including
/// scrollback — so which one a snapshot returns decides whether searching a
/// tab's past is a thing the daemon can already answer, or a feature that has
/// to be built first. This test states the answer rather than assuming it.
#[test]
fn snapshot_reports_how_far_back_it_reaches() {
    let session = Tab::spawn(shell(), 80, 10).expect("spawn");
    session.send(b"seq 1 300\n").expect("send");
    assert!(
        wait_for(Duration::from_secs(10), || screen_contains(&session, "300")),
        "the tab never produced the output"
    );

    let text = session.screen_text().expect("text");
    let lines = text.lines().filter(|l| !l.trim().is_empty()).count();
    let reaches_back = text.contains("\n1\n") || text.starts_with("1\n");

    // Ten rows of terminal, three hundred lines of output: a snapshot that
    // carries scrollback is far taller than the screen.
    eprintln!(
        "snapshot: {lines} non-blank lines for a 10-row screen; reaches line 1: {reaches_back}"
    );
    assert!(lines > 0, "snapshot came back empty");
    session.kill().ok();
}

/// A pane promoted into a departed pane's place must take its orientation too.
///
/// `split_dir` says how a pane sits against the tab it was split from, so a
/// record kept from an old parent describes a division that no longer exists.
/// The heir stands in for what left: it lands in that slot, oriented the way
/// the slot was, instead of halving its new parent afresh.
#[test]
fn an_heir_takes_the_orientation_of_the_place_it_fills() {
    let workspace = keepd::Workspace::new("orientation");
    let (root, _) = workspace.new_tab(None, 80, 24, 0, 0).expect("root");
    let (middle, _) = workspace.new_tab(None, 80, 24, root, 1).expect("middle pane");
    let (leaf, _) = workspace.new_tab(None, 80, 24, middle, 2).expect("leaf pane");

    workspace.close_tab(middle).expect("close the middle pane");

    let tabs = workspace.tabs();
    let leaf_entry = tabs.iter().find(|t| t.id == leaf).expect("leaf survived");
    assert_eq!(leaf_entry.split_of, root, "the leaf did not inherit the root");
    assert_eq!(
        leaf_entry.split_dir, 1,
        "the leaf kept an orientation measured against a pane that is gone"
    );
    workspace.kill_all().ok();
}

/// A whole chain finishing at once must leave one tab, not one tab per orphan.
///
/// Shells exit together — closing a split's root kills the panes under it — so
/// a reap can carry away a pane and the pane it was split from in the same
/// pass. Rehoming each against its own record hands one survivor the id of
/// something else that just left, and a pane pointing at nothing is reported
/// standalone. This is the scattering seen when panes are closed quickly.
#[test]
fn a_group_finishing_together_stays_one_tab() {
    let workspace = keepd::Workspace::new("group");
    let (root, root_tab) = workspace.new_tab(None, 80, 24, 0, 0).expect("root");
    let (middle, middle_tab) = workspace.new_tab(None, 80, 24, root, 1).expect("middle pane");
    let (deep, _) = workspace.new_tab(None, 80, 24, middle, 1).expect("pane of the middle");
    let (shallow, _) = workspace.new_tab(None, 80, 24, root, 2).expect("pane of the root");

    // The root and the pane below it finish together; the two panes hanging
    // off them are still running.
    root_tab.kill().expect("kill the root");
    middle_tab.kill().expect("kill the middle pane");
    assert!(
        wait_for(Duration::from_secs(5), || root_tab.is_finished() && middle_tab.is_finished()),
        "the shells never exited"
    );
    workspace.reap();

    let tabs = workspace.tabs();
    assert_eq!(tabs.len(), 2, "reaping took a running pane with it");
    let roots: Vec<u32> = tabs.iter().filter(|t| t.split_of == 0).map(|t| t.id).collect();
    assert_eq!(roots.len(), 1, "the survivors scattered into a tab each: {roots:?}");
    let follower = tabs.iter().find(|t| t.id != roots[0]).expect("two survivors");
    assert_eq!(follower.split_of, roots[0], "the other survivor is not in the same tab");
    assert!(
        roots[0] == deep || roots[0] == shallow,
        "something that was not a survivor became the root"
    );
    workspace.kill_all().ok();
}

/// Two panes trading places must stay two panes of one tab.
#[test]
fn rearranging_swaps_two_panes() {
    let workspace = keepd::Workspace::new("swap");
    let (root, _) = workspace.new_tab(None, 80, 24, 0, 0).expect("root");
    let (left, _) = workspace.new_tab(None, 80, 24, root, 1).expect("left pane");
    let (right, _) = workspace.new_tab(None, 80, 24, left, 1).expect("right pane");

    // right takes left's place, left takes right's.
    workspace.rearrange(&[(right, root, 1), (left, right, 1)]).expect("swap");

    let tabs = workspace.tabs();
    assert_eq!(tabs.len(), 3, "a swap changed how many tabs there are");
    let roots: Vec<u32> = tabs.iter().filter(|t| t.split_of == 0).map(|t| t.id).collect();
    assert_eq!(roots, vec![root], "the swap broke the tab apart");
    let r = tabs.iter().find(|t| t.id == right).expect("right survived");
    let l = tabs.iter().find(|t| t.id == left).expect("left survived");
    assert_eq!(r.split_of, root);
    assert_eq!(l.split_of, right);
    workspace.kill_all().ok();
}

/// A client replays the list in order, splitting each pane off one already
/// there, so a pane must never be listed before the pane it hangs off.
/// Creation order gave that for free; moving panes does not.
#[test]
fn rearranging_keeps_parents_ahead_of_their_panes() {
    let workspace = keepd::Workspace::new("order");
    let (root, _) = workspace.new_tab(None, 80, 24, 0, 0).expect("root");
    let (first, _) = workspace.new_tab(None, 80, 24, root, 1).expect("first pane");
    let (second, _) = workspace.new_tab(None, 80, 24, root, 1).expect("second pane");

    // Hang the older pane off the newer one, which inverts the listing order.
    workspace.rearrange(&[(first, second, 2)]).expect("move");

    let tabs = workspace.tabs();
    let mut seen: Vec<u32> = Vec::new();
    for entry in &tabs {
        if entry.split_of != 0 {
            assert!(
                seen.contains(&entry.split_of),
                "tab {} comes before the tab it hangs off",
                entry.id
            );
        }
        seen.push(entry.id);
    }
    workspace.kill_all().ok();
}

/// A loop is not a layout: reading it back would never end.
#[test]
fn rearranging_refuses_to_make_a_loop() {
    let workspace = keepd::Workspace::new("loop");
    let (root, _) = workspace.new_tab(None, 80, 24, 0, 0).expect("root");
    let (pane, _) = workspace.new_tab(None, 80, 24, root, 1).expect("pane");

    assert!(workspace.rearrange(&[(root, pane, 1)]).is_err(), "a loop was allowed");
    assert!(workspace.rearrange(&[(pane, pane, 1)]).is_err(), "a self-loop was allowed");

    // and nothing moved
    let tabs = workspace.tabs();
    assert_eq!(tabs.iter().find(|t| t.id == root).unwrap().split_of, 0);
    assert_eq!(tabs.iter().find(|t| t.id == pane).unwrap().split_of, root);
    workspace.kill_all().ok();
}

/// Dragging a pane out of the split makes it a tab of its own.
#[test]
fn rearranging_can_detach_a_pane_into_a_tab() {
    let workspace = keepd::Workspace::new("detach");
    let (root, _) = workspace.new_tab(None, 80, 24, 0, 0).expect("root");
    let (pane, _) = workspace.new_tab(None, 80, 24, root, 1).expect("pane");

    workspace.rearrange(&[(pane, 0, 0)]).expect("detach");

    let tabs = workspace.tabs();
    let roots: Vec<u32> = tabs.iter().filter(|t| t.split_of == 0).map(|t| t.id).collect();
    assert_eq!(roots, vec![root, pane], "the pane did not become a tab of its own");
    workspace.kill_all().ok();
}

/// A pane moved into another tab belongs to that tab, and the tab it left
/// stays a tab.
#[test]
fn a_pane_can_be_moved_into_another_tab() {
    let workspace = keepd::Workspace::new("across");
    let (first, _) = workspace.new_tab(None, 80, 24, 0, 0).expect("first tab");
    let (pane, _) = workspace.new_tab(None, 80, 24, first, 1).expect("pane of the first");
    let (second, _) = workspace.new_tab(None, 80, 24, 0, 0).expect("second tab");

    workspace.rearrange(&[(pane, second, 1)]).expect("move across");

    let tabs = workspace.tabs();
    let roots: Vec<u32> = tabs.iter().filter(|t| t.split_of == 0).map(|t| t.id).collect();
    assert_eq!(roots, vec![first, second], "moving a pane changed which tabs exist");
    let moved = tabs.iter().find(|t| t.id == pane).expect("the pane survived");
    assert_eq!(moved.split_of, second, "the pane did not join the other tab");
}

/// Dragging one pane must move one pane.
///
/// Panes record what they were split from, so the ones hanging off the one
/// being dragged would travel with it — a whole subtree, when what was picked
/// up was a single pane. The app sends the moves that keep them: the first
/// takes the departing pane's place and the rest hang off it. This states the
/// shape that composition has to produce.
#[test]
fn moving_a_pane_can_leave_its_own_panes_behind() {
    let workspace = keepd::Workspace::new("alone");
    let (root, _) = workspace.new_tab(None, 80, 24, 0, 0).expect("root");
    let (middle, _) = workspace.new_tab(None, 80, 24, root, 1).expect("middle");
    let (below, _) = workspace.new_tab(None, 80, 24, middle, 2).expect("below the middle");
    let (elsewhere, _) = workspace.new_tab(None, 80, 24, 0, 0).expect("another tab");

    // What the app composes: the child takes the mover's place, then the
    // mover goes.
    workspace
        .rearrange(&[(below, root, 1), (middle, elsewhere, 1)])
        .expect("move one pane");

    let tabs = workspace.tabs();
    let stayed = tabs.iter().find(|t| t.id == below).expect("the child survived");
    assert_eq!(stayed.split_of, root, "the child was dragged along");
    let moved = tabs.iter().find(|t| t.id == middle).expect("the mover survived");
    assert_eq!(moved.split_of, elsewhere, "the pane did not arrive");
}

/// The tab reports where it is working, and follows a `cd`.
///
/// The shell inside is never asked: this comes from the kernel, so it holds
/// for a setup that emits no OSC 7 — which is most of them.
#[test]
fn cwd_follows_the_shell() {
    let session = Tab::spawn(shell(), 60, 10).expect("spawn");

    // `/tmp` is a symlink to `/private/tmp` on macOS, and what comes back is
    // the resolved path either way — so compare against the resolved one.
    let target = std::fs::canonicalize("/tmp").expect("resolve /tmp");
    let target = target.to_str().expect("utf-8 path").to_owned();

    assert!(
        wait_for(Duration::from_secs(5), || !session.cwd().is_empty()),
        "the tab never reported a directory"
    );
    assert_ne!(session.cwd(), target, "the test would prove nothing from /tmp");

    session.send(b"cd /tmp\n").expect("send");
    assert!(
        wait_for(Duration::from_secs(5), || session.cwd() == target),
        "cwd did not follow the shell: {:?}",
        session.cwd()
    );
    session.kill().ok();
}

/// The tab says what is holding it: the shell at a prompt, the command while
/// one runs.
///
/// This is the answer for a tab whose title says nothing useful — or says the
/// same thing as five others, which is what a tool that titles every session
/// after itself produces.
#[test]
fn command_names_what_is_running() {
    let session = Tab::spawn(shell(), 60, 10).expect("spawn");
    // The executable's own name, not the path it was spawned by: on macOS
    // `/bin/sh` is bash wearing another name, and this reports what is
    // actually running rather than what was asked for.
    assert!(
        wait_for(Duration::from_secs(5), || {
            matches!(session.command().as_str(), "sh" | "bash")
        }),
        "a tab at a prompt did not name its shell: {:?}",
        session.command()
    );

    session.send(b"exec cat\n").expect("send");
    assert!(
        wait_for(Duration::from_secs(5), || session.command() == "cat"),
        "the running command was not named: {:?}",
        session.command()
    );
    session.kill().ok();
}

/// Activity is stamped by output as well as by input.
///
/// A command that finishes while nobody types is the case this exists for:
/// counting keystrokes alone would call such a tab stale.
#[test]
fn output_alone_counts_as_activity() {
    let session = Tab::spawn(shell(), 60, 10).expect("spawn");
    assert!(
        wait_for(Duration::from_secs(5), || session.last_active() > 0),
        "a fresh tab reported no activity at all"
    );

    session.send(b"sleep 1; echo awake\n").expect("send");
    assert!(
        wait_for(Duration::from_secs(5), || screen_contains(&session, "awake")),
        "the shell never ran the command"
    );
    let after_typing = session.last_active();

    // Nothing is sent from here on; only the shell's own output can move it.
    session.send(b"(sleep 1; echo later) &\n").expect("send");
    let sent_at = session.last_active();
    assert!(
        wait_for(Duration::from_secs(8), || session.last_active() > sent_at),
        "output did not count as activity"
    );
    assert!(after_typing > 0);
    session.kill().ok();
}
