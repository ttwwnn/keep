//! Searching tab history, which is the daemon's job because the daemon is
//! where the history lives.

use std::time::{Duration, Instant};

use keepd::Registry;
use portable_pty::CommandBuilder;

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

/// A workspace with one tab that has said something specific.
///
/// The wait is for a line that IS the marker, not one that merely contains
/// it: a shell echoes the command before running it, so `contains` is
/// satisfied by the typing rather than by the output.
fn workspace_with_output(registry: &Registry, name: &str, marker: &str) -> u32 {
    let workspace = registry.get_or_create(name).expect("workspace");
    let (id, tab) = workspace.new_tab(None, 80, 10, 0, 0).expect("tab");
    tab.send(format!("printf '%s\\n' {marker}\n").as_bytes()).expect("send");
    assert!(
        wait_for(Duration::from_secs(10), || {
            tab.screen_text()
                .map(|t| t.lines().any(|l| l.trim() == marker))
                .unwrap_or(false)
        }),
        "the tab never printed {marker} on a line of its own"
    );
    id
}

#[test]
fn finds_a_line_and_says_where_it_is() {
    let registry = Registry::new();
    let tab = workspace_with_output(&registry, "alpha", "NEEDLEONE");

    let hits = registry.search("needleone", 50, None);
    let hit = hits
        .iter()
        .find(|h| h.text.trim() == "NEEDLEONE")
        .unwrap_or_else(|| panic!("no hit for NEEDLEONE; got {hits:?}"));

    assert_eq!(hit.workspace, "alpha");
    assert_eq!(hit.tab, tab);
    // The match is located, not merely reported.
    let start = hit.match_start as usize;
    let end = start + hit.match_len as usize;
    assert_eq!(&hit.text[start..end], "NEEDLEONE", "match offsets point elsewhere");

    registry.kill("alpha").ok();
}

#[test]
fn scope_keeps_the_search_inside_one_pane() {
    let registry = Registry::new();
    workspace_with_output(&registry, "alpha", "SHARED");
    let beta_tab = workspace_with_output(&registry, "beta", "SHARED");

    let everywhere = registry.search("shared", 50, None);
    let workspaces: std::collections::HashSet<&str> =
        everywhere.iter().map(|h| h.workspace.as_str()).collect();
    assert!(
        workspaces.contains("alpha") && workspaces.contains("beta"),
        "a global search missed a workspace: {workspaces:?}"
    );

    let scoped = registry.search("shared", 50, Some(("beta", beta_tab)));
    assert!(!scoped.is_empty(), "the scoped search found nothing");
    assert!(
        scoped.iter().all(|h| h.workspace == "beta" && h.tab == beta_tab),
        "the scoped search escaped its pane: {scoped:?}"
    );

    registry.kill("alpha").ok();
    registry.kill("beta").ok();
}

#[test]
fn a_hit_carries_the_lines_around_it() {
    let registry = Registry::new();
    let workspace = registry.get_or_create("ctx").expect("workspace");
    let (_, tab) = workspace.new_tab(None, 80, 10, 0, 0).expect("tab");
    tab.send(b"printf 'above\\nMIDDLE\\nbelow\\n'\n").expect("send");
    assert!(
        wait_for(Duration::from_secs(10), || {
            tab.screen_text()
                .map(|t| t.lines().any(|l| l.trim() == "below"))
                .unwrap_or(false)
        }),
        "the tab never printed the block"
    );

    let hits = registry.search("MIDDLE", 50, None);
    let hit = hits
        .iter()
        .find(|h| h.text.trim() == "MIDDLE")
        .unwrap_or_else(|| panic!("no hit on the middle line; got {hits:?}"));
    assert!(
        hit.before.iter().any(|l| l.contains("above")),
        "the line above did not travel with the hit: {:?}",
        hit.before
    );
    assert!(
        hit.after.iter().any(|l| l.contains("below")),
        "the line below did not travel with the hit: {:?}",
        hit.after
    );

    registry.kill("ctx").ok();
}
