//! A workspace: a named group of tabs.
//!
//! The workspace is what a person thinks of as "a project". It holds one or
//! more tabs, and it is the unit that survives clients coming and going. Tabs
//! live here rather than in the app so that closing the window loses nothing:
//! reopening finds the same tabs, still running.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use portable_pty::CommandBuilder;

use crate::Tab;

struct Entry {
    id: u32,
    tab: Arc<Tab>,
    /// The tab this one is a pane of (0 = standalone), and where it sits.
    /// Kept here, not in the app: the layout must survive every client.
    split_of: u32,
    split_dir: u8,
}

/// A snapshot of one tab and its place in the layout.
pub struct TabRef {
    pub id: u32,
    pub split_of: u32,
    pub split_dir: u8,
    pub tab: Arc<Tab>,
}

pub struct Workspace {
    name: String,
    tabs: Mutex<Vec<Entry>>,
    next_id: AtomicU32,
}

/// Give a departed tab's panes somewhere to belong.
///
/// A pane records the tab it was split from, and a pane whose record points at
/// nothing is reported standalone — so left alone, closing one pane would
/// scatter everything below it into tabs of its own.
///
/// One pane takes the departed's place: its parent and its orientation, so the
/// survivor lands in the slot the arrangement already had rather than halving
/// the parent afresh. The rest hang off that heir. A departed root has no slot
/// to take, so the heir becomes the root.
///
/// Returns the heir, where there was one to promote.
fn inherit(tabs: &mut [Entry], departed: u32, parent: u32, dir: u8) -> Option<u32> {
    // Creation order, so the oldest pane is the one promoted.
    let heir = tabs.iter().position(|e| e.split_of == departed)?;
    let heir_id = tabs[heir].id;
    tabs[heir].split_of = parent;
    tabs[heir].split_dir = dir;
    for entry in tabs.iter_mut() {
        if entry.split_of == departed && entry.id != heir_id {
            entry.split_of = heir_id;
        }
    }
    Some(heir_id)
}

/// Rehome the panes of everything that left, in one pass.
///
/// Departures arrive in groups — a shell exiting takes the shells of its panes
/// with it — and a group can hold both a pane and the pane it was split from.
/// Rehoming each against the record it kept would hand a survivor the id of
/// something else that left in the same pass, and a pane whose parent does not
/// exist is reported standalone: the arrangement would come apart at exactly
/// the moment a whole split finishes at once.
///
/// So each departed slot is resolved *through* the others first, up to the
/// first tab that is actually still there. Everything whose chain ends at the
/// same dead root is kept together under one heir instead of each orphan
/// becoming a tab.
fn rehome(tabs: &mut [Entry], departed: &HashMap<u32, (u32, u8)>) {
    let mut ids: Vec<u32> = departed.keys().copied().collect();
    ids.sort_unstable();

    // Dead root -> the pane promoted to stand in for it.
    let mut promoted: HashMap<u32, u32> = HashMap::new();

    for id in ids {
        let (own_parent, own_dir) = departed[&id];
        let (mut parent, mut dir) = (own_parent, own_dir);
        // The chain's last departed link, whose place the heir ends up taking.
        let mut top = id;
        // Ids only ever point backwards, so this cannot loop; bound it anyway
        // rather than trust a record to be well formed.
        let mut hops = departed.len();
        while let Some(&(grandparent, grandparent_dir)) = departed.get(&parent) {
            if hops == 0 {
                break;
            }
            hops -= 1;
            top = parent;
            parent = grandparent;
            dir = grandparent_dir;
        }

        if parent != 0 {
            inherit(tabs, id, parent, dir);
        } else if let Some(&heir) = promoted.get(&top) {
            // The root of this chain already has a stand-in; join it there,
            // keeping the orientation this pane was split with.
            inherit(tabs, id, heir, own_dir);
        } else if let Some(heir) = inherit(tabs, id, 0, 0) {
            promoted.insert(top, heir);
        }
    }
}

impl Workspace {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), tabs: Mutex::new(Vec::new()), next_id: AtomicU32::new(1) }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Start a tab running the user's shell.
    ///
    /// `split_of` other than 0 makes it a pane of that tab; the target must
    /// exist, or a stale client could quietly create an orphan layout.
    pub fn new_tab(
        &self,
        cwd: Option<&str>,
        cols: u16,
        rows: u16,
        split_of: u32,
        split_dir: u8,
    ) -> Result<(u32, Arc<Tab>)> {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let mut cmd = CommandBuilder::new(shell);
        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }
        // Programs expect these; without TERM many refuse to draw at all.
        cmd.env("TERM", "xterm-256color");
        cmd.env("KEEP_WORKSPACE", &self.name);

        let tab = Arc::new(Tab::spawn(cmd, cols, rows)?);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        let mut guard = self.tabs.lock().map_err(|_| anyhow!("workspace poisoned"))?;
        if split_of != 0 && !guard.iter().any(|e| e.id == split_of) {
            return Err(anyhow!("no tab {split_of} to split"));
        }
        guard.push(Entry { id, tab: Arc::clone(&tab), split_of, split_dir });
        Ok((id, tab))
    }

    pub fn tab(&self, id: u32) -> Option<Arc<Tab>> {
        let guard = self.tabs.lock().ok()?;
        guard.iter().find(|e| e.id == id).map(|e| Arc::clone(&e.tab))
    }

    /// The tab a client lands on when it names a workspace but not a tab.
    pub fn first_or_create(&self, cols: u16, rows: u16) -> Result<(u32, Arc<Tab>)> {
        {
            let guard = self.tabs.lock().map_err(|_| anyhow!("workspace poisoned"))?;
            if let Some(entry) = guard.iter().find(|e| !e.tab.is_finished()) {
                return Ok((entry.id, Arc::clone(&entry.tab)));
            }
        }
        self.new_tab(None, cols, rows, 0, 0)
    }

    pub fn tabs(&self) -> Vec<TabRef> {
        let guard = match self.tabs.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        let live: std::collections::HashSet<u32> = guard.iter().map(|e| e.id).collect();
        guard
            .iter()
            .map(|e| TabRef {
                id: e.id,
                // A pane whose base tab is gone is promoted to standalone
                // rather than reported dangling: the layout must always be
                // reconstructible from what this returns.
                split_of: if live.contains(&e.split_of) { e.split_of } else { 0 },
                split_dir: e.split_dir,
                tab: Arc::clone(&e.tab),
            })
            .collect()
    }

    pub fn close_tab(&self, id: u32) -> Result<()> {
        let tab = {
            let mut guard = self.tabs.lock().map_err(|_| anyhow!("workspace poisoned"))?;
            let pos = guard.iter().position(|e| e.id == id).ok_or_else(|| anyhow!("no tab {id}"))?;
            let closed = guard.remove(pos);
            rehome(&mut guard, &HashMap::from([(closed.id, (closed.split_of, closed.split_dir))]));
            closed.tab
        };
        tab.kill()
    }

    pub fn kill_all(&self) -> Result<()> {
        let tabs: Vec<Arc<Tab>> = {
            let mut guard = self.tabs.lock().map_err(|_| anyhow!("workspace poisoned"))?;
            guard.drain(..).map(|e| e.tab).collect()
        };
        for tab in tabs {
            tab.kill().ok();
        }
        Ok(())
    }

    /// Drop tabs whose child exited and that nobody is watching.
    ///
    /// A finished tab someone is still attached to stays: they may not have
    /// read the last of its output yet.
    pub fn reap(&self) {
        if let Ok(mut guard) = self.tabs.lock() {
            let departed: HashMap<u32, (u32, u8)> = guard
                .iter()
                .filter(|e| e.tab.is_finished() && e.tab.attached_clients() == 0)
                .map(|e| (e.id, (e.split_of, e.split_dir)))
                .collect();
            if departed.is_empty() {
                return;
            }
            guard.retain(|e| !departed.contains_key(&e.id));
            // The same inheritance as `close_tab`, resolved across the whole
            // group: panes whose own shell exited must not take the panes
            // below them out of the arrangement.
            rehome(&mut guard, &departed);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.lock().map(|g| g.is_empty()).unwrap_or(true)
    }
}
