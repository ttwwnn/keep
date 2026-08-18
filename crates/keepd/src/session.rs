//! A session: a child process on a PTY, plus the screen state it produced.
//!
//! The daemon owns these. That ownership is the whole point — a client can
//! disconnect and the child keeps running, because nothing about the child's
//! lifetime is tied to whoever is currently looking at it.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use keep_vt::{Format, Terminal};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

/// How much we read from the PTY per syscall.
const READ_CHUNK: usize = 64 * 1024;

pub struct Session {
    /// Screen state. libghostty-vt forbids concurrent writes, and the reader
    /// thread writes to it, so every access goes through this lock.
    terminal: Arc<Mutex<Terminal>>,
    writer: Mutex<Box<dyn Write + Send>>,
    master: Box<dyn MasterPty + Send>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    /// Set when the PTY reaches EOF, i.e. the child is gone.
    finished: Arc<AtomicBool>,
}

impl Session {
    pub fn spawn(command: CommandBuilder, cols: u16, rows: u16) -> Result<Self> {
        let pair = native_pty_system()
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .context("openpty")?;

        let child = pair.slave.spawn_command(command).context("spawn child")?;
        // Drop the slave: otherwise the master never sees EOF when the child
        // exits, because this process would still hold the other end open.
        drop(pair.slave);

        let writer = pair.master.take_writer().context("pty writer")?;
        let mut reader = pair.master.try_clone_reader().context("pty reader")?;

        let terminal = Arc::new(Mutex::new(
            Terminal::new(cols, rows).map_err(|e| anyhow::anyhow!("terminal: {e}"))?,
        ));
        let finished = Arc::new(AtomicBool::new(false));

        // Reader thread. The lock is held only for the parse, never across the
        // read syscall, so a chatty session cannot block readers of the screen.
        let sink = Arc::clone(&terminal);
        let done = Arc::clone(&finished);
        std::thread::Builder::new()
            .name("keepd-pty-reader".into())
            .spawn(move || {
                let mut buf = vec![0u8; READ_CHUNK];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if let Ok(mut t) = sink.lock() {
                                t.write(&buf[..n]);
                            } else {
                                break;
                            }
                        }
                    }
                }
                done.store(true, Ordering::Release);
            })
            .context("spawn reader thread")?;

        Ok(Self {
            terminal,
            writer: Mutex::new(writer),
            master: pair.master,
            child: Mutex::new(child),
            finished,
        })
    }

    /// Forward client input to the child.
    pub fn send(&self, bytes: &[u8]) -> Result<()> {
        let mut w = self.writer.lock().map_err(|_| anyhow::anyhow!("writer poisoned"))?;
        w.write_all(bytes)?;
        w.flush()?;
        Ok(())
    }

    /// The screen as plain text.
    pub fn screen_text(&self) -> Result<String> {
        let t = self.terminal.lock().map_err(|_| anyhow::anyhow!("terminal poisoned"))?;
        t.text().map_err(|e| anyhow::anyhow!("snapshot: {e}"))
    }

    /// The screen as VT sequences — what an attaching client gets repainted with.
    pub fn repaint(&self) -> Result<Vec<u8>> {
        let t = self.terminal.lock().map_err(|_| anyhow::anyhow!("terminal poisoned"))?;
        t.snapshot(Format::Vt).map_err(|e| anyhow::anyhow!("snapshot: {e}"))
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.master
            .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .context("resize pty")?;
        // TODO: the terminal grid must be resized to match; libghostty-vt
        // exposes ghostty_terminal_resize and keep-vt does not wrap it yet.
        Ok(())
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
