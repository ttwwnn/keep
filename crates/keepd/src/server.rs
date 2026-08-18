//! The unix socket server.
//!
//! One thread per connection. After a client attaches, only the pump thread
//! writes to the socket and only the connection thread reads from it, so the
//! two never interleave frames on the same stream.

use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use keep_proto::{ClientMsg, ServerMsg};

use crate::Registry;

pub struct Server {
    listener: UnixListener,
    path: PathBuf,
    registry: Arc<Registry>,
}

impl Server {
    /// Bind the socket, refusing to displace a daemon that is already running.
    pub fn bind(path: &Path) -> Result<Self> {
        if path.exists() {
            // A stale socket from a crashed daemon is safe to remove; a live
            // one is not, so probe it before deciding.
            match UnixStream::connect(path) {
                Ok(_) => anyhow::bail!("a keep daemon is already listening on {}", path.display()),
                Err(_) => {
                    std::fs::remove_file(path).ok();
                }
            }
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let listener = UnixListener::bind(path)
            .with_context(|| format!("bind {}", path.display()))?;

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
        std::fs::remove_file(&self.path).ok();
    }
}

fn handle(stream: UnixStream, registry: Arc<Registry>) -> Result<()> {
    let mut reader = stream.try_clone().context("clone socket")?;
    let mut writer = stream;

    let Some(first) = ClientMsg::read(&mut reader)? else { return Ok(()) };

    match first {
        ClientMsg::List => {
            registry.reap();
            ServerMsg::Sessions(registry.list()).write(&mut writer)?;
            Ok(())
        }
        ClientMsg::Kill { name } => {
            let msg = match registry.kill(&name) {
                Ok(()) => ServerMsg::Ok,
                Err(e) => ServerMsg::Error(e.to_string()),
            };
            msg.write(&mut writer)?;
            Ok(())
        }
        ClientMsg::Create { name, cwd, cols, rows } => {
            let msg = match registry.get_or_create(&name, cwd.as_deref(), cols, rows) {
                Ok(_) => ServerMsg::Ok,
                Err(e) => ServerMsg::Error(e.to_string()),
            };
            msg.write(&mut writer)?;
            Ok(())
        }
        ClientMsg::Attach { name, cols, rows } => {
            attach(reader, writer, registry, &name, cols, rows)
        }
        other => {
            ServerMsg::Error(format!("unexpected opening message: {other:?}")).write(&mut writer)?;
            Ok(())
        }
    }
}

fn attach(
    mut reader: UnixStream,
    mut writer: UnixStream,
    registry: Arc<Registry>,
    name: &str,
    cols: u16,
    rows: u16,
) -> Result<()> {
    let session = match registry.get_or_create(name, None, cols, rows) {
        Ok(s) => s,
        Err(e) => {
            ServerMsg::Error(e.to_string()).write(&mut writer)?;
            return Ok(());
        }
    };

    // The client's geometry wins: it is the thing actually displaying this.
    session.resize(cols, rows).ok();

    let (repaint, attachment) = session.attach()?;
    ServerMsg::Repaint(repaint).write(&mut writer)?;

    // Pump: session output to the socket. Sole writer for the rest of the
    // connection.
    //
    // It must be told when the client leaves. A quiet session produces no
    // output, so waiting for a failed write would leave this thread parked
    // forever, still holding the attachment and still counted as a watcher.
    let stop = Arc::new(AtomicBool::new(false));
    let pump_stop = Arc::clone(&stop);
    let ended_session = Arc::clone(&session);
    let pump = std::thread::Builder::new()
        .name("keepd-pump".into())
        .spawn(move || {
            loop {
                match attachment.output.recv_timeout(Duration::from_millis(200)) {
                    Ok(chunk) => {
                        if ServerMsg::Output(chunk).write(&mut writer).is_err() {
                            break;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        if pump_stop.load(Ordering::Acquire) {
                            break;
                        }
                        if ended_session.is_finished() {
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
                if session.send(&data).is_err() {
                    break;
                }
            }
            ClientMsg::Resize { cols, rows } => {
                session.resize(cols, rows).ok();
            }
            _ => break,
        }
    }

    // The client is gone. The session is not: that is the whole point.
    stop.store(true, Ordering::Release);
    drop(reader);
    pump.join().ok();
    Ok(())
}
