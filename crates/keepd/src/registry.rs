//! The set of live sessions, keyed by name.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use keep_proto::{SessionInfo, TabInfo};

use crate::Session;

#[derive(Default)]
pub struct Registry {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn list(&self) -> Vec<SessionInfo> {
        let guard = match self.sessions.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        let mut out: Vec<SessionInfo> = guard
            .iter()
            .map(|(name, session)| SessionInfo {
                name: name.clone(),
                tabs: session
                    .tabs()
                    .into_iter()
                    .map(|(id, tab)| {
                        let (cols, rows) = tab.size();
                        TabInfo {
                            id,
                            cols,
                            rows,
                            clients: tab.attached_clients() as u32,
                            finished: tab.is_finished(),
                            title: tab.title(),
                        }
                    })
                    .collect(),
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn get(&self, name: &str) -> Option<Arc<Session>> {
        self.sessions.lock().ok()?.get(name).cloned()
    }

    /// Sessions are created on demand: naming one is enough to have it.
    pub fn get_or_create(&self, name: &str) -> Result<Arc<Session>> {
        let mut guard = self.sessions.lock().map_err(|_| anyhow!("registry poisoned"))?;
        if let Some(existing) = guard.get(name) {
            return Ok(Arc::clone(existing));
        }
        let session = Arc::new(Session::new(name));
        guard.insert(name.to_string(), Arc::clone(&session));
        Ok(session)
    }

    pub fn kill(&self, name: &str) -> Result<()> {
        let session = {
            let mut guard = self.sessions.lock().map_err(|_| anyhow!("registry poisoned"))?;
            guard.remove(name).ok_or_else(|| anyhow!("no such session: {name}"))?
        };
        session.kill_all()
    }

    /// Drop dead tabs, then sessions left with nothing in them.
    pub fn reap(&self) {
        let sessions: Vec<Arc<Session>> = match self.sessions.lock() {
            Ok(g) => g.values().cloned().collect(),
            Err(_) => return,
        };
        for session in &sessions {
            session.reap();
        }
        if let Ok(mut guard) = self.sessions.lock() {
            guard.retain(|_, s| !s.is_empty());
        }
    }
}
