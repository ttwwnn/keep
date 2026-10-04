//! The set of live workspaces, keyed by name.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use keep_proto::{DaemonInfo, SearchHit, TabInfo, WorkspaceInfo};

use crate::Workspace;

#[derive(Default)]
pub struct Registry {
    workspaces: Mutex<HashMap<String, Arc<Workspace>>>,
    /// When this daemon started, in unix milliseconds: with its pid, what
    /// tells one daemon's tab 3 from the next one's.
    started_ms: u64,
}

impl Registry {
    pub fn new() -> Self {
        let started_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self { started_ms, ..Self::default() }
    }

    /// Which daemon this is.
    pub fn daemon(&self) -> DaemonInfo {
        DaemonInfo { pid: std::process::id(), started_ms: self.started_ms }
    }

    pub fn list(&self) -> Vec<WorkspaceInfo> {
        let guard = match self.workspaces.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        let mut out: Vec<WorkspaceInfo> = guard
            .iter()
            .map(|(name, workspace)| WorkspaceInfo {
                name: name.clone(),
                tabs: workspace
                    .tabs()
                    .into_iter()
                    .map(|t| {
                        let (cols, rows) = t.tab.size();
                        TabInfo {
                            id: t.id,
                            cols,
                            rows,
                            clients: t.tab.attached_clients() as u32,
                            finished: t.tab.is_finished(),
                            title: t.tab.title(),
                            busy: t.tab.is_busy(),
                            split_of: t.split_of,
                            split_dir: t.split_dir,
                            cwd: t.tab.cwd(),
                            last_active: t.tab.last_active(),
                            command: t.tab.command(),
                            shell_pid: t.tab.shell_pid().unwrap_or(0),
                            pid: t.tab.foreground().unwrap_or(0),
                        }
                    })
                    .collect(),
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Every line of every tab's history that contains `query`, case
    /// insensitively, oldest tab first and oldest line first.
    ///
    /// The search runs here because this is where the text is: the daemon
    /// holds each tab's whole screen, scrollback included, for tabs no client
    /// has ever opened. Asking each client to search its own view would miss
    /// most of what there is to find.
    /// Lines of history containing `query`, case insensitively.
    ///
    /// `scope` of `None` searches every tab of every workspace; naming one
    /// scopes it to that pane. The search runs here because this is where the
    /// text is: the daemon holds each tab's whole screen, scrollback included,
    /// for tabs no client has ever opened.
    pub fn search(
        &self,
        query: &str,
        limit: usize,
        scope: Option<(&str, u32)>,
    ) -> Vec<SearchHit> {
        if query.is_empty() || limit == 0 {
            return Vec::new();
        }
        let needle = query.to_lowercase();
        let mut targets: Vec<(String, Arc<Workspace>)> = match self.workspaces.lock() {
            Ok(g) => g.iter().map(|(n, w)| (n.clone(), Arc::clone(w))).collect(),
            Err(_) => return Vec::new(),
        };
        targets.sort_by(|a, b| a.0.cmp(&b.0));
        if let Some((name, _)) = scope {
            targets.retain(|(n, _)| n == name);
        }

        /// How much of the surrounding output travels with a hit. Enough to
        /// recognise the place without going there; more would be the pane.
        const CONTEXT: usize = 2;

        let mut hits = Vec::new();
        for (name, workspace) in targets {
            for entry in workspace.tabs() {
                if let Some((_, tab)) = scope {
                    if entry.id != tab {
                        continue;
                    }
                }
                // Snapshot per tab, not per line: the lock is the tab's, and
                // holding it while matching would stall its reader thread.
                let Ok(text) = entry.tab.screen_text() else { continue };
                let lines: Vec<&str> = text.lines().collect();
                for (index, line) in lines.iter().enumerate() {
                    if hits.len() >= limit {
                        return hits;
                    }
                    let Some(at) = line.to_lowercase().find(&needle) else { continue };
                    // `find` on the lowercased copy gives a byte offset that
                    // is only meaningful there when case folding changes
                    // length; re-anchor on the original so the caller can
                    // slice it safely.
                    let start = if line.is_char_boundary(at) { at } else { 0 };
                    let len = if line.is_char_boundary(start + query.len()) {
                        query.len()
                    } else {
                        0
                    };
                    hits.push(SearchHit {
                        workspace: name.clone(),
                        tab: entry.id,
                        line: index as u32,
                        total: lines.len() as u32,
                        text: line.trim_end().to_string(),
                        match_start: start as u32,
                        match_len: len as u32,
                        before: lines[index.saturating_sub(CONTEXT)..index]
                            .iter()
                            .map(|l| l.trim_end().to_string())
                            .collect(),
                        after: lines[(index + 1).min(lines.len())
                            ..(index + 1 + CONTEXT).min(lines.len())]
                            .iter()
                            .map(|l| l.trim_end().to_string())
                            .collect(),
                    });
                }
            }
        }
        hits
    }

    pub fn get(&self, name: &str) -> Option<Arc<Workspace>> {
        self.workspaces.lock().ok()?.get(name).cloned()
    }

    /// Workspaces are created on demand: naming one is enough to have it.
    pub fn get_or_create(&self, name: &str) -> Result<Arc<Workspace>> {
        let mut guard = self.workspaces.lock().map_err(|_| anyhow!("registry poisoned"))?;
        if let Some(existing) = guard.get(name) {
            return Ok(Arc::clone(existing));
        }
        let workspace = Arc::new(Workspace::new(name));
        guard.insert(name.to_string(), Arc::clone(&workspace));
        Ok(workspace)
    }

    /// Move a tab — panes and all — from one workspace to another.
    ///
    /// Nothing about the shells changes: the same `Tab`s, the same PTYs, the
    /// same subscribers, filed under a different name with fresh ids. Ids are
    /// per-workspace counters, so the receiver assigns its own; the new root
    /// id comes back for whoever wants to follow the tab there.
    ///
    /// The two workspace locks are taken one at a time — take, then adopt —
    /// so this cannot deadlock against anything, and the worst a crash
    /// between the two could leave is a tab in transit, in a process that
    /// just crashed anyway.
    pub fn move_tab(&self, from: &str, tab: u32, to: &str) -> Result<u32> {
        if from == to {
            return Err(anyhow!("a tab cannot move to where it is"));
        }
        let source =
            self.get(from).ok_or_else(|| anyhow!("no such workspace: {from}"))?;
        let target = self.get_or_create(to)?;
        let tree = source.take_tree(tab)?;
        target.adopt_tree(tree)
    }

    pub fn kill(&self, name: &str) -> Result<()> {
        let workspace = {
            let mut guard = self.workspaces.lock().map_err(|_| anyhow!("registry poisoned"))?;
            guard.remove(name).ok_or_else(|| anyhow!("no such workspace: {name}"))?
        };
        workspace.kill_all()
    }

    /// Drop dead tabs, then workspaces left with nothing in them.
    pub fn reap(&self) {
        let workspaces: Vec<Arc<Workspace>> = match self.workspaces.lock() {
            Ok(g) => g.values().cloned().collect(),
            Err(_) => return,
        };
        for workspace in &workspaces {
            workspace.reap();
        }
        if let Ok(mut guard) = self.workspaces.lock() {
            guard.retain(|_, s| !s.is_empty());
        }
    }
}
