//! A workspace: a named group of tabs.
//!
//! The workspace is what a person thinks of as "a project". It holds one or
//! more tabs, and it is the unit that survives clients coming and going. Tabs
//! live here rather than in the app so that closing the window loses nothing:
//! reopening finds the same tabs, still running.

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
            // Anything split off the closed tab takes its place in the
            // arrangement. Without this they are orphans, and an orphan is
            // reported standalone — so closing one pane of a split would
            // scatter the panes below it into tabs of their own.
            for entry in guard.iter_mut() {
                if entry.split_of == id {
                    entry.split_of = closed.split_of;
                }
            }
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
            let gone: Vec<(u32, u32)> = guard
                .iter()
                .filter(|e| e.tab.is_finished() && e.tab.attached_clients() == 0)
                .map(|e| (e.id, e.split_of))
                .collect();
            guard.retain(|e| !(e.tab.is_finished() && e.tab.attached_clients() == 0));
            // Same inheritance as `close_tab`: a pane whose own shell exited
            // must not take the panes below it out of the arrangement.
            for (id, parent) in gone {
                for entry in guard.iter_mut() {
                    if entry.split_of == id {
                        entry.split_of = parent;
                    }
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.lock().map(|g| g.is_empty()).unwrap_or(true)
    }
}
