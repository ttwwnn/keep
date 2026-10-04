//! Attached tabs: one connection to the daemon per terminal on screen.
//!
//! What a tab writes reaches the page through a channel, as raw bytes with one
//! leading byte saying what they are; what the page types goes back down the
//! same connection. The page's terminal answers the questions programs ask a
//! terminal — where the cursor is, what colour the background is — the way
//! the macOS app's surfaces do, so the daemon forwards those to it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow};
use keep_proto::net::Stream;
use keep_proto::{ClientMsg, ServerMsg};
use tauri::ipc::{Channel, InvokeResponseBody};

/// The first byte of every message on a tab's channel.
pub mod kind {
    /// The tab's id, as a little-endian u32: which tab the daemon put us on.
    pub const ATTACHED: u8 = 1;
    /// The whole screen, to be painted from the top.
    pub const REPAINT: u8 = 2;
    /// More of what the tab wrote.
    pub const OUTPUT: u8 = 3;
    /// The shell ended.
    pub const ENDED: u8 = 4;
    /// The daemon refused or went away; the rest is the reason in UTF-8.
    pub const ERROR: u8 = 5;
}

struct Session {
    writer: Mutex<Stream>,
    /// For shutting the connection down from outside the reader thread.
    control: Stream,
}

#[derive(Default)]
pub struct Sessions {
    next: AtomicU64,
    open: Mutex<HashMap<u64, Arc<Session>>>,
}

fn frame(kind: u8, payload: &[u8]) -> InvokeResponseBody {
    let mut out = Vec::with_capacity(payload.len() + 1);
    out.push(kind);
    out.extend_from_slice(payload);
    InvokeResponseBody::Raw(out)
}

impl Sessions {
    /// Attach to a tab and start forwarding what it writes. Returns the
    /// handle the page uses for input, size and detaching.
    pub fn attach(
        self: &Arc<Self>,
        workspace: String,
        tab: u32,
        cols: u16,
        rows: u16,
        channel: Channel<InvokeResponseBody>,
    ) -> Result<u64> {
        let mut sock = Stream::connect(crate::daemon::address()).context("conectar ao keepd")?;
        ClientMsg::Attach { workspace, tab, cols, rows }.write(&mut sock)?;
        let reader = sock.try_clone()?;
        let control = sock.try_clone()?;
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let session = Arc::new(Session { writer: Mutex::new(sock), control });
        self.open.lock().map_err(|_| anyhow!("sessões envenenadas"))?.insert(id, session);

        let sessions = Arc::clone(self);
        std::thread::Builder::new()
            .name(format!("keep-tab-{id}"))
            .spawn(move || {
                let mut reader = reader;
                loop {
                    let message = match ServerMsg::read(&mut reader) {
                        Ok(Some(ServerMsg::Attached { tab })) => frame(kind::ATTACHED, &tab.to_le_bytes()),
                        Ok(Some(ServerMsg::Repaint(data))) => frame(kind::REPAINT, &data),
                        Ok(Some(ServerMsg::Output(data))) => frame(kind::OUTPUT, &data),
                        Ok(Some(ServerMsg::Ended)) => {
                            let _ = channel.send(frame(kind::ENDED, &[]));
                            break;
                        }
                        Ok(Some(ServerMsg::Error(reason))) => {
                            let _ = channel.send(frame(kind::ERROR, reason.as_bytes()));
                            break;
                        }
                        Ok(Some(_)) => continue,
                        // The connection ended: detached on purpose, or the
                        // daemon went away. Only the second is news.
                        Ok(None) | Err(_) => {
                            if sessions.is_open(id) {
                                let _ = channel.send(frame(kind::ERROR, "conexão encerrada".as_bytes()));
                            }
                            break;
                        }
                    };
                    if channel.send(message).is_err() {
                        break;
                    }
                }
                sessions.forget(id);
            })
            .context("iniciar leitura da aba")?;
        Ok(id)
    }

    fn is_open(&self, id: u64) -> bool {
        self.open.lock().map(|m| m.contains_key(&id)).unwrap_or(false)
    }

    fn forget(&self, id: u64) {
        if let Ok(mut map) = self.open.lock() {
            map.remove(&id);
        }
    }

    fn get(&self, id: u64) -> Result<Arc<Session>> {
        self.open
            .lock()
            .map_err(|_| anyhow!("sessões envenenadas"))?
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow!("aba {id} não está anexada"))
    }

    pub fn input(&self, id: u64, data: &[u8]) -> Result<()> {
        let session = self.get(id)?;
        let mut writer = session.writer.lock().map_err(|_| anyhow!("escrita envenenada"))?;
        ClientMsg::write_input(&mut *writer, data)?;
        Ok(())
    }

    /// The viewer's size. Zero by zero says it is not looking, and the daemon
    /// leaves it out of the reckoning.
    pub fn resize(&self, id: u64, cols: u16, rows: u16) -> Result<()> {
        let session = self.get(id)?;
        let mut writer = session.writer.lock().map_err(|_| anyhow!("escrita envenenada"))?;
        ClientMsg::Resize { cols, rows }.write(&mut *writer)?;
        Ok(())
    }

    /// Let go of a tab. The tab runs on; only this viewer leaves.
    pub fn detach(&self, id: u64) {
        let session = match self.open.lock() {
            Ok(mut map) => map.remove(&id),
            Err(_) => None,
        };
        if let Some(session) = session {
            let _ = session.control.shutdown(std::net::Shutdown::Both);
        }
    }

    pub fn detach_all(&self) {
        let all: Vec<u64> = self.open.lock().map(|m| m.keys().copied().collect()).unwrap_or_default();
        for id in all {
            self.detach(id);
        }
    }
}
