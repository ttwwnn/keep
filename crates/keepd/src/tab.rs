//! A tab: a child process on a PTY, plus the screen state it produced.
//!
//! The daemon owns these. That ownership is the whole point — a client can
//! disconnect and the child keeps running, because nothing about the child's
//! lifetime is tied to whoever is currently looking at it.
//!
//! Tabs are grouped into sessions; see `session.rs`.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use keep_vt::{Format, Terminal};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

/// How much we read from the PTY per syscall.
const READ_CHUNK: usize = 64 * 1024;

/// Screen state and the clients watching it.
///
/// These live under one lock on purpose: attaching must take a snapshot and
/// subscribe as a single step. Split them and a client either misses output
/// produced between the two, or replays output the snapshot already contains.
struct Subscriber {
    id: u64,
    tx: Sender<Vec<u8>>,
}

struct Inner {
    terminal: Terminal,
    subscribers: Vec<Subscriber>,
    next_id: u64,
}

pub struct Tab {
    inner: Arc<Mutex<Inner>>,
    writer: Mutex<Box<dyn Write + Send>>,
    // Mutex, not a bare Box: MasterPty is Send but not Sync, and the daemon
    // shares each session across connection threads via Arc.
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    finished: Arc<AtomicBool>,
    size: Mutex<(u16, u16)>,
    /// The shell's own pid, kept to tell "sitting at a prompt" apart from
    /// "running something".
    shell_pid: Option<u32>,
}

/// A live feed of everything the tab writes from the moment of attach.
///
/// Dropping this unsubscribes. Relying on a failed send to notice a departed
/// client is not enough: a quiet session sends nothing, so it would keep
/// reporting a client that left minutes ago.
pub struct Attachment {
    /// Output produced after the snapshot.
    pub output: Receiver<Vec<u8>>,
    inner: Arc<Mutex<Inner>>,
    id: u64,
}

impl Drop for Attachment {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.subscribers.retain(|s| s.id != self.id);
        }
    }
}

impl Tab {
    pub fn spawn(command: CommandBuilder, cols: u16, rows: u16) -> Result<Self> {
        let pair = native_pty_system()
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .context("openpty")?;

        let child = pair.slave.spawn_command(command).context("spawn child")?;
        let shell_pid = child.process_id();
        // Drop the slave: otherwise the master never sees EOF when the child
        // exits, because this process would still hold the other end open.
        drop(pair.slave);

        let writer = pair.master.take_writer().context("pty writer")?;
        let mut reader = pair.master.try_clone_reader().context("pty reader")?;

        let inner = Arc::new(Mutex::new(Inner {
            terminal: Terminal::new(cols, rows).map_err(|e| anyhow::anyhow!("terminal: {e}"))?,
            subscribers: Vec::new(),
            next_id: 0,
        }));
        let finished = Arc::new(AtomicBool::new(false));

        // Reader thread. The lock is held only for the parse and fan-out, never
        // across the read syscall, so a chatty tab cannot stall readers of
        // the screen.
        let sink = Arc::clone(&inner);
        let done = Arc::clone(&finished);
        std::thread::Builder::new()
            .name("keepd-pty-reader".into())
            .spawn(move || {
                let mut buf = vec![0u8; READ_CHUNK];
                loop {
                    let n = match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    let Ok(mut guard) = sink.lock() else { break };
                    guard.terminal.write(&buf[..n]);
                    // Drop clients whose receiver is gone.
                    let chunk = &buf[..n];
                    guard.subscribers.retain(|s| s.tx.send(chunk.to_vec()).is_ok());
                }
                done.store(true, Ordering::Release);
                // Dropping the senders lets attached clients notice the end.
                if let Ok(mut guard) = sink.lock() {
                    guard.subscribers.clear();
                }
            })
            .context("spawn reader thread")?;

        Ok(Self {
            inner,
            writer: Mutex::new(writer),
            master: Mutex::new(pair.master),
            child: Mutex::new(child),
            finished,
            size: Mutex::new((cols, rows)),
            shell_pid,
        })
    }

    /// Subscribe to the tab.
    ///
    /// Returns the screen as it stands now, plus a feed of everything after
    /// it. Both come from one locked section so no output can slip between.
    pub fn attach(&self) -> Result<(Vec<u8>, Attachment)> {
        let mut guard = self.inner.lock().map_err(|_| anyhow::anyhow!("tab poisoned"))?;
        let repaint = guard
            .terminal
            .snapshot(Format::Vt)
            .map_err(|e| anyhow::anyhow!("snapshot: {e}"))?;
        let (tx, rx) = channel();
        let id = guard.next_id;
        guard.next_id += 1;
        guard.subscribers.push(Subscriber { id, tx });
        drop(guard);
        Ok((repaint, Attachment { output: rx, inner: Arc::clone(&self.inner), id }))
    }

    /// Forward client input to the child.
    pub fn send(&self, bytes: &[u8]) -> Result<()> {
        let mut w = self.writer.lock().map_err(|_| anyhow::anyhow!("writer poisoned"))?;
        w.write_all(bytes)?;
        w.flush()?;
        Ok(())
    }

    /// What the program inside called itself (OSC 0/2), if anything.
    pub fn title(&self) -> String {
        self.inner.lock().map(|g| g.terminal.title()).unwrap_or_default()
    }

    /// The screen as plain text. Used for previews and tests.
    pub fn screen_text(&self) -> Result<String> {
        let guard = self.inner.lock().map_err(|_| anyhow::anyhow!("tab poisoned"))?;
        guard.terminal.text().map_err(|e| anyhow::anyhow!("snapshot: {e}"))
    }

    /// The screen as VT sequences — what an attaching client is repainted with.
    pub fn repaint(&self) -> Result<Vec<u8>> {
        let guard = self.inner.lock().map_err(|_| anyhow::anyhow!("tab poisoned"))?;
        guard
            .terminal
            .snapshot(Format::Vt)
            .map_err(|e| anyhow::anyhow!("snapshot: {e}"))
    }

    /// Resize both the PTY and the grid.
    ///
    /// These must move together: the child draws for the size the kernel
    /// reports, while the grid interprets what it draws.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        if cols == 0 || rows == 0 {
            return Ok(());
        }
        {
            let mut guard = self.inner.lock().map_err(|_| anyhow::anyhow!("tab poisoned"))?;
            guard
                .terminal
                .resize(cols, rows)
                .map_err(|e| anyhow::anyhow!("resize grid: {e}"))?;
        }
        {
            let master = self.master.lock().map_err(|_| anyhow::anyhow!("master poisoned"))?;
            master
                .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
                .context("resize pty")?;
        }
        if let Ok(mut s) = self.size.lock() {
            *s = (cols, rows);
        }
        Ok(())
    }

    pub fn size(&self) -> (u16, u16) {
        self.size.lock().map(|s| *s).unwrap_or((80, 24))
    }

    pub fn attached_clients(&self) -> usize {
        self.inner.lock().map(|g| g.subscribers.len()).unwrap_or(0)
    }

    /// Whether a command is running, as opposed to a shell waiting at its
    /// prompt.
    ///
    /// The terminal's foreground process group is the shell itself while it
    /// waits, and something else while it runs a command. That is the honest
    /// signal, and unlike shell integration it needs no cooperation from the
    /// user's setup.
    pub fn is_busy(&self) -> bool {
        if self.is_finished() {
            return false;
        }
        let Some(shell) = self.shell_pid else { return false };
        let Ok(master) = self.master.lock() else { return false };
        match master.process_group_leader() {
            Some(fg) => fg as u32 != shell,
            // No foreground group means nothing is claiming the terminal.
            None => false,
        }
    }

    /// True once the child is gone and the PTY hit EOF.
    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    pub fn kill(&self) -> Result<()> {
        let mut c = self.child.lock().map_err(|_| anyhow::anyhow!("child poisoned"))?;
        c.kill()?;
        Ok(())
    }
}
