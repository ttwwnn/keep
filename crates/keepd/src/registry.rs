//! The set of live sessions, keyed by name.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use keep_proto::SessionInfo;
use portable_pty::CommandBuilder;

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
            .map(|(name, s)| {
                let (cols, rows) = s.size();
                SessionInfo {
                    name: name.clone(),
                    cols,
                    rows,
                    clients: s.attached_clients() as u32,
                    finished: s.is_finished(),
                }
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn get(&self, name: &str) -> Option<Arc<Session>> {
        self.sessions.lock().ok()?.get(name).cloned()
    }

    /// Attach semantics: an existing session is reused, a missing one is
    /// created. Reconnecting to a name must never silently start a second
    /// shell and orphan the first.
    pub fn get_or_create(
        &self,
        name: &str,
        cwd: Option<&str>,
        cols: u16,
        rows: u16,
    ) -> Result<Arc<Session>> {
        let mut guard = self.sessions.lock().map_err(|_| anyhow!("registry poisoned"))?;

        // A finished session is a corpse; replace it rather than attach to it.
        if let Some(existing) = guard.get(name) {
            if !existing.is_finished() {
                return Ok(Arc::clone(existing));
            }
            guard.remove(name);
        }

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let mut cmd = CommandBuilder::new(shell);
        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }
        // Programs expect these; without TERM many refuse to draw at all.
        cmd.env("TERM", "xterm-256color");
        cmd.env("KEEP_SESSION", name);

        let session = Arc::new(Session::spawn(cmd, cols, rows)?);
        guard.insert(name.to_string(), Arc::clone(&session));
        Ok(session)
    }

    pub fn kill(&self, name: &str) -> Result<()> {
        let session = {
            let mut guard = self.sessions.lock().map_err(|_| anyhow!("registry poisoned"))?;
            guard.remove(name).ok_or_else(|| anyhow!("no such session: {name}"))?
        };
        session.kill()
    }

    /// Drop sessions whose child exited and that nobody is watching.
    pub fn reap(&self) {
        if let Ok(mut guard) = self.sessions.lock() {
            guard.retain(|_, s| !(s.is_finished() && s.attached_clients() == 0));
        }
    }
}
