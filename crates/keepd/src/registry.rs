//! The set of live workspaces, keyed by name.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use keep_proto::{SearchHit, TabInfo, WorkspaceInfo};

use crate::Workspace;

#[derive(Default)]
pub struct Registry {
    workspaces: Mutex<HashMap<String, Arc<Workspace>>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
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
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        if query.is_empty() || limit == 0 {
            return Vec::new();
        }
        let needle = query.to_lowercase();
        let workspaces: Vec<(String, Arc<Workspace>)> = match self.workspaces.lock() {
            Ok(g) => {
                let mut all: Vec<_> = g.iter().map(|(n, w)| (n.clone(), Arc::clone(w))).collect();
                all.sort_by(|a, b| a.0.cmp(&b.0));
                all
            }
            Err(_) => return Vec::new(),
        };

        let mut hits = Vec::new();
        for (name, workspace) in workspaces {
            for entry in workspace.tabs() {
                // Snapshot per tab, not per line: the lock is the tab's, and
                // holding it while matching would stall its reader thread.
                let Ok(text) = entry.tab.screen_text() else { continue };
                for (index, line) in text.lines().enumerate() {
                    if hits.len() >= limit {
                        return hits;
                    }
                    if line.to_lowercase().contains(&needle) {
                        hits.push(SearchHit {
                            workspace: name.clone(),
                            tab: entry.id,
                            line: index as u32,
                            text: line.trim_end().to_string(),
                        });
                    }
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
