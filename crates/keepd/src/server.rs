//! The socket server: a unix socket, or a named pipe on Windows.
//!
//! One thread per connection. After a client attaches, only the pump thread
//! writes to the socket and only the connection thread reads from it, so the
//! two never interleave frames on the same stream.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use keep_proto::net::{Listener, Stream};
use keep_proto::{ClientMsg, ServerMsg, TAB_ANY};

use crate::Registry;

pub struct Server {
    listener: Listener,
    path: PathBuf,
    registry: Arc<Registry>,
}

impl Server {
    /// Bind the socket, refusing to displace a daemon that is already running.
    pub fn bind(path: &Path) -> Result<Self> {
        #[cfg(unix)]
        if path.exists() {
            // A stale socket from a crashed daemon is safe to remove; a live
            // one is not, so probe it before deciding.
            match Stream::connect(path) {
                Ok(_) => anyhow::bail!("a keep daemon is already listening on {}", path.display()),
                Err(_) => {
                    std::fs::remove_file(path).ok();
                }
            }
        }
        // A pipe leaves nothing behind when its daemon dies, so there is no
        // stale file to clear — only a live daemon to stand aside for.
        #[cfg(windows)]
        if Stream::connect(path).is_ok() {
            anyhow::bail!("a keep daemon is already listening on {}", path.display());
        }
        #[cfg(unix)]
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let listener =
            Listener::bind(path).with_context(|| format!("bind {}", path.display()))?;

        Ok(Self { listener, path: path.to_path_buf(), registry: Arc::new(Registry::new()) })
    }

    pub fn registry(&self) -> Arc<Registry> {
        Arc::clone(&self.registry)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn run(&self) -> Result<()> {
        for stream in self.listener.incoming() {
            let stream = match stream {
                Ok(s) => s,
                Err(_) => continue,
            };
            let registry = Arc::clone(&self.registry);
            std::thread::Builder::new()
                .name("keepd-conn".into())
                .spawn(move || {
                    let _ = handle(stream, registry);
                })
                .ok();
        }
        Ok(())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        #[cfg(unix)]
        std::fs::remove_file(&self.path).ok();
    }
}

fn handle(stream: Stream, registry: Arc<Registry>) -> Result<()> {
    let mut reader = stream.try_clone().context("clone socket")?;
    let mut writer = stream;

    let Some(first) = ClientMsg::read(&mut reader)? else { return Ok(()) };

    match first {
        // Both questions are still answered. A client built before `cwd` and
        // `last_active` existed keeps asking the old one, and it keeps
        // working — the daemon outlives every client, so the two are never
        // upgraded together.
        ClientMsg::List => {
            registry.reap();
            ServerMsg::Workspaces(registry.list()).write(&mut writer)?;
            Ok(())
        }
        ClientMsg::List2 => {
            registry.reap();
            ServerMsg::Workspaces2(registry.list()).write(&mut writer)?;
            Ok(())
        }
        ClientMsg::Kill { workspace } => {
            let msg = match registry.kill(&workspace) {
                Ok(()) => ServerMsg::Ok,
                Err(e) => ServerMsg::Error(e.to_string()),
            };
            msg.write(&mut writer)?;
            Ok(())
        }
        ClientMsg::NewTab { workspace, cwd, cols, rows, split_of, split_dir } => {
            let msg = match registry
                .get_or_create(&workspace)
                .and_then(|s| s.new_tab(cwd.as_deref(), cols, rows, split_of, split_dir))
            {
                Ok((tab, _)) => ServerMsg::TabCreated { tab },
                Err(e) => ServerMsg::Error(e.to_string()),
            };
            msg.write(&mut writer)?;
            Ok(())
        }
        ClientMsg::CloseTab { workspace, tab } => {
            let msg = match registry
                .get(&workspace)
                .ok_or_else(|| anyhow::anyhow!("no such workspace: {workspace}"))
                .and_then(|s| s.close_tab(tab))
            {
                Ok(()) => ServerMsg::Ok,
                Err(e) => ServerMsg::Error(e.to_string()),
            };
            msg.write(&mut writer)?;
            Ok(())
        }
        ClientMsg::MoveTab { workspace, tab, to } => {
            match registry.move_tab(&workspace, tab, &to) {
                Ok(root) => ServerMsg::TabCreated { tab: root }.write(&mut writer)?,
                Err(e) => ServerMsg::Error(e.to_string()).write(&mut writer)?,
            }
            Ok(())
        }
        ClientMsg::PreviewVt { workspace, tab } => {
            // The same screen the repaint is made of: text with the colours
            // and styles still on it. Lossy because it goes down a string
            // field, and a screen holds no invalid UTF-8 anyway — every byte
            // in it was decoded as a character before it was stored.
            let vt = registry
                .get(&workspace)
                .and_then(|w| w.tab(tab))
                .and_then(|t| t.repaint().ok())
                .unwrap_or_default();
            ServerMsg::PreviewText(String::from_utf8_lossy(&vt).into_owned()).write(&mut writer)?;
            Ok(())
        }
        ClientMsg::Preview { workspace, tab } => {
            // A tab that has gone previews as nothing: the picker showing it
            // is a list that can lag behind the daemon by a poll, and an
            // error there would be noise, not information.
            let text = registry
                .get(&workspace)
                .and_then(|w| w.tab(tab))
                .and_then(|t| t.screen_text().ok())
                .unwrap_or_default();
            ServerMsg::PreviewText(text).write(&mut writer)?;
            Ok(())
        }
        ClientMsg::Search { query, limit, workspace, tab } => {
            let scope = if workspace.is_empty() { None } else { Some((workspace.as_str(), tab)) };
            let hits = registry.search(&query, limit as usize, scope);
            ServerMsg::SearchHits(hits).write(&mut writer)?;
            Ok(())
        }
        ClientMsg::Rearrange { workspace, moves } => {
            let reply = match registry.get(&workspace) {
                None => ServerMsg::Error(format!("no such workspace: {workspace}")),
                Some(space) => {
                    let moves: Vec<(u32, u32, u8)> =
                        moves.iter().map(|m| (m.tab, m.split_of, m.split_dir)).collect();
                    match space.rearrange(&moves) {
                        Ok(()) => ServerMsg::Ok,
                        Err(e) => ServerMsg::Error(e.to_string()),
                    }
                }
            };
            reply.write(&mut writer)?;
            Ok(())
        }
        ClientMsg::Attach { workspace, tab, cols, rows } => {
            attach(reader, writer, registry, &workspace, tab, cols, rows)
        }
        other => {
            ServerMsg::Error(format!("unexpected opening message: {other:?}")).write(&mut writer)?;
            Ok(())
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn attach(
    mut reader: Stream,
    mut writer: Stream,
    registry: Arc<Registry>,
    name: &str,
    tab_id: u32,
    cols: u16,
    rows: u16,
) -> Result<()> {
    let resolved = registry.get_or_create(name).and_then(|s| {
        if tab_id == TAB_ANY {
            s.first_or_create(cols, rows)
        } else {
            s.tab(tab_id)
                .map(|t| (tab_id, t))
                .ok_or_else(|| anyhow::anyhow!("no tab {tab_id} in workspace {name}"))
        }
    });

    let (tab_id, tab) = match resolved {
        Ok(pair) => pair,
        Err(e) => {
            ServerMsg::Error(e.to_string()).write(&mut writer)?;
            return Ok(());
        }
    };

    // Tell the client which tab it landed on — it may have asked for TAB_ANY.
    ServerMsg::Attached { tab: tab_id }.write(&mut writer)?;

    // The client's geometry joins the reckoning rather than winning it: the
    // tab fits whichever viewer is smallest, and this may be it.
    let (repaint, attachment) = tab.attach(cols, rows)?;
    let viewer = attachment.id();
    ServerMsg::Repaint(repaint).write(&mut writer)?;

    // Pump: session output to the socket. Sole writer for the rest of the
    // connection.
    //
    // It must be told when the client leaves. A quiet session produces no
    // output, so waiting for a failed write would leave this thread parked
    // forever, still holding the attachment and still counted as a watcher.
    let stop = Arc::new(AtomicBool::new(false));
    let pump_stop = Arc::clone(&stop);
    let ended_tab = Arc::clone(&tab);
    let pump = std::thread::Builder::new()
        .name("keepd-pump".into())
        .spawn(move || {
            loop {
                // A client the daemon had to drop output for is handed the
                // screen whole. Forwarding the rest of the stream instead
                // would leave it painting on top of a gap it cannot see.
                if attachment.overflowed() {
                    match attachment.resync() {
                        Ok(screen) => {
                            if ServerMsg::Repaint(screen).write(&mut writer).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                    continue;
                }
                match attachment.output.recv_timeout(Duration::from_millis(200)) {
                    Ok(chunk) => {
                        // Borrowed, not owned: this chunk is shared with every
                        // other client watching the tab.
                        if ServerMsg::write_output(&mut writer, &chunk).is_err() {
                            break;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        if pump_stop.load(Ordering::Acquire) {
                            break;
                        }
                        if ended_tab.is_finished() {
                            let _ = ServerMsg::Ended.write(&mut writer);
                            break;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        let _ = ServerMsg::Ended.write(&mut writer);
                        break;
                    }
                }
            }
            // Unblock the reader side so the connection thread can finish.
            let _ = writer.shutdown(std::net::Shutdown::Both);
        })
        .context("spawn pump")?;

    // Input loop.
    while let Some(msg) = ClientMsg::read(&mut reader).unwrap_or(None) {
        match msg {
            ClientMsg::Input(data) => {
                if tab.send(&data).is_err() {
                    break;
                }
            }
            ClientMsg::Resize { cols, rows } => {
                tab.set_viewer_size(viewer, cols, rows).ok();
            }
            _ => break,
        }
    }

    // The client is gone. The workspace is not: that is the whole point.
    stop.store(true, Ordering::Release);
    drop(reader);
    // After the join, not before: the attachment is dropped on the pump
    // thread, and rebalancing while this viewer is still counted would leave
    // the tab fitted to somebody who has already gone.
    pump.join().ok();
    tab.rebalance();
    Ok(())
}
