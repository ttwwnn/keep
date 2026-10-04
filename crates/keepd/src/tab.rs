//! A tab: a child process on a PTY, plus the screen state it produced.
//!
//! The daemon owns these. That ownership is the whole point — a client can
//! disconnect and the child keeps running, because nothing about the child's
//! lifetime is tied to whoever is currently looking at it.
//!
//! Tabs are grouped into sessions; see `session.rs`.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use keep_vt::{Format, Terminal};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

/// How much we read from the PTY per syscall.
const READ_CHUNK: usize = 64 * 1024;

/// How many chunks may sit unread for one client before the daemon stops
/// queueing for it.
///
/// The queue has to have a ceiling. A client whose socket has stopped draining
/// — a minimised window, a machine gone to sleep — would otherwise make the
/// daemon buffer everything its tab produces, and a tab can produce output far
/// faster than a stalled client reads it. What replaces the queue is not a hole
/// in the stream but a repaint; see [`Attachment::resync`].
const CLIENT_BACKLOG: usize = 64;

/// Screen state and the clients watching it.
///
/// These live under one lock on purpose: attaching must take a snapshot and
/// subscribe as a single step. Split them and a client either misses output
/// produced between the two, or replays output the snapshot already contains.
struct Subscriber {
    id: u64,
    tx: SyncSender<Arc<[u8]>>,
    /// Set when this client was too far behind to be handed a chunk. It is
    /// repaid with the whole screen rather than a stream missing a piece.
    overflowed: Arc<AtomicBool>,
    /// The size this client is showing, which is a request and not a verdict.
    /// A tab has one size and may have several viewers; see `negotiate`.
    /// Zero in either direction means "not looking" and is left out.
    size: (u16, u16),
}

struct Inner {
    terminal: Terminal,
    subscribers: Vec<Subscriber>,
    next_id: u64,
}

pub struct Tab {
    inner: Arc<Mutex<Inner>>,
    /// Shared with the reader thread, which on Windows answers the terminal
    /// queries nobody else is there to answer; see `Tab::spawn`.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    // Mutex, not a bare Box: MasterPty is Send but not Sync, and the daemon
    // shares each session across connection threads via Arc.
    ///
    /// Taken out and dropped when the shell exits on Windows, where that is
    /// the only way its output reaches its end; see `Tab::spawn`.
    master: Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    finished: Arc<AtomicBool>,
    size: Mutex<(u16, u16)>,
    /// The shell's own pid, kept to tell "sitting at a prompt" apart from
    /// "running something".
    shell_pid: Option<u32>,
    /// When a byte last went either way, in unix milliseconds.
    ///
    /// Both directions, because either one alone lies about a different tab:
    /// counting only input calls a finished build stale the moment you stop
    /// typing, and counting only output calls a REPL you are typing into
    /// stale while it waits for you.
    last_active: Arc<AtomicU64>,
}

fn now_ms() -> u64 {
    // SystemTime rather than Instant: Instant on macOS is CLOCK_UPTIME_RAW,
    // which stops while the machine sleeps. A laptop shut overnight would
    // wake claiming every tab had been active minutes ago.
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Where a process is working, asked of the kernel.
///
/// The same authority `is_busy` trusts, and for the same reason: it needs no
/// cooperation from the user's setup, where OSC 7 needs a shell that emits it.
#[cfg(target_os = "macos")]
fn process_cwd(pid: i32) -> Option<String> {
    let mut info: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_vnodepathinfo>() as libc::c_int;
    let read = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            &mut info as *mut _ as *mut libc::c_void,
            size,
        )
    };
    // A short reply is a failed one: the pid is gone, or belongs to another
    // user and this process may not ask about it.
    if read < size {
        return None;
    }
    // libc types the path as a 32×32 array rather than as [c_char; 1024];
    // it is one buffer either way, so read it as one.
    let path = unsafe {
        std::slice::from_raw_parts(
            info.pvi_cdir.vip_path.as_ptr() as *const u8,
            std::mem::size_of_val(&info.pvi_cdir.vip_path),
        )
    };
    let end = path.iter().position(|&b| b == 0).unwrap_or(path.len());
    String::from_utf8(path[..end].to_vec()).ok().filter(|p| !p.is_empty())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn process_cwd(pid: i32) -> Option<String> {
    std::fs::read_link(format!("/proc/{pid}/cwd"))
        .ok()
        .and_then(|p| p.into_os_string().into_string().ok())
        .filter(|p| !p.is_empty())
}

/// What a process is called: the last component of its executable's path.
///
/// The name and not the path, because this is read in a list beside a title
/// and `claude` is the whole of what the reader wants from
/// `/opt/homebrew/bin/claude`.
#[cfg(target_os = "macos")]
fn process_name(pid: i32) -> Option<String> {
    // PROC_PIDPATHINFO_MAXSIZE. Written out rather than named because libc
    // exports the struct but not this constant.
    const MAX_PATH: usize = 4 * 1024;
    let mut buf = vec![0u8; MAX_PATH];
    let written = unsafe {
        libc::proc_pidpath(pid, buf.as_mut_ptr() as *mut libc::c_void, MAX_PATH as u32)
    };
    if written <= 0 {
        return None;
    }
    buf.truncate(written as usize);
    let path = String::from_utf8(buf).ok()?;
    let name = path.rsplit('/').next()?.to_owned();
    (!name.is_empty()).then_some(name)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn process_name(pid: i32) -> Option<String> {
    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    let name = comm.trim().to_owned();
    (!name.is_empty()).then_some(name)
}

#[cfg(windows)]
fn process_cwd(pid: i32) -> Option<String> {
    crate::winproc::process_cwd(u32::try_from(pid).ok()?)
}

#[cfg(windows)]
fn process_name(pid: i32) -> Option<String> {
    crate::winproc::process_name(u32::try_from(pid).ok()?)
}

/// The bytes with every cursor position query (`CSI 6 n`) taken out.
#[cfg(windows)]
fn without_cursor_queries(bytes: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    const QUERY: &[u8] = b"\x1b[6n";
    if !bytes.windows(QUERY.len()).any(|w| w == QUERY) {
        return std::borrow::Cow::Borrowed(bytes);
    }
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at..].starts_with(QUERY) {
            at += QUERY.len();
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    std::borrow::Cow::Owned(out)
}

/// The cursor position reports (`CSI row ; col R`) among a run of replies.
#[cfg(windows)]
fn cursor_reports(replies: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < replies.len() {
        if replies[at] == 0x1b && replies.get(at + 1) == Some(&b'[') {
            let mut end = at + 2;
            while end < replies.len() && (replies[end].is_ascii_digit() || replies[end] == b';') {
                end += 1;
            }
            if end < replies.len() {
                if replies[end] == b'R' {
                    out.extend_from_slice(&replies[at..=end]);
                }
                at = end + 1;
                continue;
            }
        }
        at += 1;
    }
    out
}

#[cfg(all(test, windows))]
mod cursor_tests {
    use super::*;

    #[test]
    fn cursor_queries_are_taken_out_and_the_rest_kept() {
        assert_eq!(&*without_cursor_queries(b"a\x1b[6nb\x1b[6n"), b"ab");
        assert_eq!(&*without_cursor_queries(b"\x1b[5n\x1b[c"), b"\x1b[5n\x1b[c");
    }

    #[test]
    fn only_cursor_reports_are_kept() {
        assert_eq!(cursor_reports(b"\x1b[0n\x1b[3;7R\x1b[?1;2c"), b"\x1b[3;7R");
        assert!(cursor_reports(b"\x1b[?62;22c").is_empty());
    }
}

/// A live feed of everything the tab writes from the moment of attach.
///
/// Dropping this unsubscribes. Relying on a failed send to notice a departed
/// client is not enough: a quiet session sends nothing, so it would keep
/// reporting a client that left minutes ago.
pub struct Attachment {
    /// Output produced after the snapshot.
    pub output: Receiver<Arc<[u8]>>,
    inner: Arc<Mutex<Inner>>,
    id: u64,
    overflowed: Arc<AtomicBool>,
}

impl Attachment {
    /// Which viewer this is, for telling the tab how big it is.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Whether the daemon had to drop output because this client fell behind.
    pub fn overflowed(&self) -> bool {
        self.overflowed.load(Ordering::Acquire)
    }

    /// Recover from a drop: discard what is queued and return the whole screen
    /// in its place.
    ///
    /// Both steps happen under the tab's lock — the same one the reader holds
    /// while it updates the grid and enqueues. That is what makes this exact
    /// rather than approximate: nothing can be enqueued between the drain and
    /// the snapshot, so the screen returned accounts for every byte the tab has
    /// produced, including the ones this client never received.
    pub fn resync(&self) -> Result<Vec<u8>> {
        let guard = self.inner.lock().map_err(|_| anyhow::anyhow!("tab poisoned"))?;
        while self.output.try_recv().is_ok() {}
        self.overflowed.store(false, Ordering::Release);
        guard
            .terminal
            .snapshot(Format::Vt)
            .map_err(|e| anyhow::anyhow!("snapshot: {e}"))
    }
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

        let writer: Arc<Mutex<Box<dyn Write + Send>>> =
            Arc::new(Mutex::new(pair.master.take_writer().context("pty writer")?));
        let mut reader = pair.master.try_clone_reader().context("pty reader")?;

        #[allow(unused_mut)]
        let mut terminal = Terminal::new(cols, rows).map_err(|e| anyhow::anyhow!("terminal: {e}"))?;
        // ConPTY starts every console by asking where the cursor is, and
        // holds the shell's output back until it hears. A tab opened with
        // nobody watching — which is how the app opens one, before attaching
        // to it — would wait forever. So on Windows the grid answers what is
        // asked while no client is there to.
        #[cfg(windows)]
        terminal.answer_queries().map_err(|e| anyhow::anyhow!("terminal: {e}"))?;

        let inner = Arc::new(Mutex::new(Inner { terminal, subscribers: Vec::new(), next_id: 0 }));
        let master: Arc<Mutex<Option<Box<dyn MasterPty + Send>>>> =
            Arc::new(Mutex::new(Some(pair.master)));

        // A pseudoconsole outlives the shell it was made for: its output
        // stays open until it is closed, so the reader below would wait for
        // an end that never comes and the tab would never be finished. The
        // shell exiting is what closes it, here, as the last process leaving
        // a unix PTY does there.
        #[cfg(windows)]
        if let Some(pid) = shell_pid {
            let slot = Arc::clone(&master);
            std::thread::Builder::new()
                .name("keepd-shell-exit".into())
                .spawn(move || {
                    use windows_sys::Win32::Foundation::CloseHandle;
                    use windows_sys::Win32::System::Threading::{
                        INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
                    };
                    let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
                    if !process.is_null() {
                        unsafe {
                            WaitForSingleObject(process, INFINITE);
                            CloseHandle(process);
                        }
                    }
                    // Out of the lock before it is dropped: closing waits for
                    // the console host to finish, and nothing else should.
                    let closed = slot.lock().ok().and_then(|mut m| m.take());
                    drop(closed);
                })
                .context("spawn exit watcher")?;
        }
        let finished = Arc::new(AtomicBool::new(false));

        // Reader thread. The lock is held only for the parse and fan-out, never
        // across the read syscall, so a chatty tab cannot stall readers of
        // the screen.
        let last_active = Arc::new(AtomicU64::new(now_ms()));

        let sink = Arc::clone(&inner);
        let done = Arc::clone(&finished);
        let touched = Arc::clone(&last_active);
        #[cfg(windows)]
        let answer = Arc::clone(&writer);
        std::thread::Builder::new()
            .name("keepd-pty-reader".into())
            .spawn(move || {
                let mut buf = vec![0u8; READ_CHUNK];
                loop {
                    let n = match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    // Before the lock: this is the tab saying something, and
                    // it is true whether or not the parse gets its turn.
                    touched.store(now_ms(), Ordering::Relaxed);
                    let Ok(mut guard) = sink.lock() else { break };
                    guard.terminal.write(&buf[..n]);
                    // What the grid answered, and what goes on to the clients.
                    //
                    // Where the cursor is, the grid answers always, and the
                    // question goes no further: on Windows it is ConPTY asking
                    // as it opens, and ConPTY holds the shell's output back
                    // until it hears — from whoever is watching, which may be
                    // a client that does not answer, or nobody yet. The grid is
                    // drawn from the same stream as every client's screen, so
                    // its answer is theirs. Everything else it answers only
                    // while nobody is attached: a client is handed those
                    // questions below and answers them itself, and two answers
                    // are one too many.
                    #[cfg(windows)]
                    let (unanswered, forward) = {
                        let replies = guard.terminal.take_replies();
                        let replies = if guard.subscribers.is_empty() {
                            replies
                        } else {
                            cursor_reports(&replies)
                        };
                        ((!replies.is_empty()).then_some(replies), without_cursor_queries(&buf[..n]))
                    };
                    #[cfg(not(windows))]
                    let forward = &buf[..n];
                    // One allocation for the whole fan-out: every client gets a
                    // handle to the same bytes rather than its own copy of a
                    // chunk that can be 64 KiB.
                    let chunk: Arc<[u8]> = Arc::from(&forward[..]);
                    guard.subscribers.retain(|s| match s.tx.try_send(Arc::clone(&chunk)) {
                        Ok(()) => true,
                        // A client that cannot keep up keeps its slot. Blocking
                        // here would stall the parse — and with it every other
                        // client — for the sake of the slowest one.
                        Err(TrySendError::Full(_)) => {
                            s.overflowed.store(true, Ordering::Release);
                            true
                        }
                        // Drop clients whose receiver is gone.
                        Err(TrySendError::Disconnected(_)) => false,
                    });
                    // Written after the lock is let go: the PTY's input can
                    // be full, and the grid must not wait on it.
                    #[cfg(windows)]
                    {
                        drop(guard);
                        if let Some(replies) = unanswered {
                            if let Ok(mut w) = answer.lock() {
                                let _ = w.write_all(&replies);
                                let _ = w.flush();
                            }
                        }
                    }
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
            writer,
            master,
            child: Mutex::new(child),
            finished,
            size: Mutex::new((cols, rows)),
            shell_pid,
            last_active,
        })
    }

    /// Subscribe to the tab.
    ///
    /// Returns the screen as it stands now, plus a feed of everything after
    /// it. Both come from one locked section so no output can slip between.
    pub fn attach(&self, cols: u16, rows: u16) -> Result<(Vec<u8>, Attachment)> {
        let attachment = {
            let mut guard = self.inner.lock().map_err(|_| anyhow::anyhow!("tab poisoned"))?;
            let (tx, rx) = sync_channel(CLIENT_BACKLOG);
            let id = guard.next_id;
            guard.next_id += 1;
            let overflowed = Arc::new(AtomicBool::new(false));
            guard.subscribers.push(Subscriber {
                id,
                tx,
                overflowed: Arc::clone(&overflowed),
                size: (cols, rows),
            });
            Attachment { output: rx, inner: Arc::clone(&self.inner), id, overflowed }
        };
        // Size first, screen second. `resync` drains this client's queue and
        // photographs the grid under one lock, so the screen it returns is the
        // one the new size produced and nothing can slip between the two —
        // which is the same atomicity the old order got by snapshotting before
        // anyone else could write.
        self.negotiate(Some(attachment.id))?;
        let repaint = attachment.resync()?;
        Ok((repaint, attachment))
    }

    /// Fit the tab to the smallest viewer watching it.
    ///
    /// A pty has one size and a tab may have several viewers, so somebody has
    /// to lose. The smallest wins because the two directions are not equally
    /// bad: a viewer *larger* than the grid shows blank margin, while a viewer
    /// *smaller* than it garbles — rows laid out for 200 columns wrap at 80,
    /// absolute cursor moves land on the wrong line, and no repaint fixes it
    /// because the repaint arrives the same shape. Under the minimum, nobody
    /// is ever smaller than the grid.
    ///
    /// `caused_by` is the viewer that asked, and is not repaid: it already
    /// knows. Everyone else is holding a screen drawn for the old size, so
    /// they are marked overflowed, which is how they are already told to ask
    /// for a whole screen — no new message, and the machinery has a test.
    fn negotiate(&self, caused_by: Option<u64>) -> Result<()> {
        let (wanted, viewers) = {
            let guard = self.inner.lock().map_err(|_| anyhow::anyhow!("tab poisoned"))?;
            let looking: Vec<(u16, u16)> = guard
                .subscribers
                .iter()
                .map(|s| s.size)
                .filter(|(c, r)| *c > 0 && *r > 0)
                .collect();
            let Some(cols) = looking.iter().map(|(c, _)| *c).min() else { return Ok(()) };
            let rows = looking.iter().map(|(_, r)| *r).min().unwrap_or(0);
            ((cols, rows), guard.subscribers.len())
        };
        // Nobody looking leaves the size alone: a tab nobody is watching
        // should not snap to some default and reflow itself for no one.
        if wanted == self.size() {
            return Ok(());
        }
        self.resize(wanted.0, wanted.1)?;
        if viewers > 1 {
            if let Ok(guard) = self.inner.lock() {
                for s in guard.subscribers.iter().filter(|s| Some(s.id) != caused_by) {
                    s.overflowed.store(true, Ordering::Release);
                }
            }
        }
        Ok(())
    }

    /// One viewer says how big it is now. The tab still fits the smallest.
    pub fn set_viewer_size(&self, viewer: u64, cols: u16, rows: u16) -> Result<()> {
        {
            let mut guard = self.inner.lock().map_err(|_| anyhow::anyhow!("tab poisoned"))?;
            let Some(s) = guard.subscribers.iter_mut().find(|s| s.id == viewer) else {
                return Ok(());
            };
            if s.size == (cols, rows) {
                return Ok(());
            }
            s.size = (cols, rows);
        }
        self.negotiate(Some(viewer))
    }

    /// A viewer has gone. Whoever is left may have room they did not have.
    pub fn rebalance(&self) {
        let _ = self.negotiate(None);
    }

    /// Forward client input to the child.
    pub fn send(&self, bytes: &[u8]) -> Result<()> {
        let mut w = self.writer.lock().map_err(|_| anyhow::anyhow!("writer poisoned"))?;
        w.write_all(bytes)?;
        w.flush()?;
        self.last_active.store(now_ms(), Ordering::Relaxed);
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
            // Gone only once the shell has exited, when there is nothing left
            // to tell the size to.
            if let Some(master) = master.as_ref() {
                master
                    .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
                    .context("resize pty")?;
            }
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
        match self.foreground_pid() {
            Some(fg) => fg as u32 != shell,
            // No foreground group means nothing is claiming the terminal.
            None => false,
        }
    }

    /// Whoever holds the terminal: the running command, or the shell waiting.
    #[cfg(unix)]
    fn foreground_pid(&self) -> Option<i32> {
        let master = self.master.lock().ok()?;
        master.as_ref()?.process_group_leader()
    }

    /// The same question on Windows, answered by the process tree: the
    /// shell's newest child while it runs one, the shell while it waits.
    /// See `winproc`.
    #[cfg(windows)]
    fn foreground_pid(&self) -> Option<i32> {
        let shell = self.shell_pid?;
        let pid = crate::winproc::foreground(shell).unwrap_or(shell);
        i32::try_from(pid).ok()
    }

    /// Where the tab is working right now.
    ///
    /// The foreground process's directory, which is the command's while one
    /// runs and the shell's at a prompt — so a tab that has `cd`'d somewhere
    /// says so without waiting for the next prompt.
    ///
    /// Asked each time rather than cached: a cache would have to be
    /// invalidated on every `cd`, which is the very thing being asked about.
    /// The call costs microseconds and the list is polled every two seconds.
    pub fn cwd(&self) -> String {
        self.foreground_pid()
            .and_then(process_cwd)
            // Asking the shell is a second attempt and not merely a stand-in
            // for a missing pid: a group leader that exited while its
            // siblings live, and a command running as another user, both
            // still name a pid — one the kernel then refuses to answer for.
            // Those are the cases this exists for, so the fallback has to
            // hang off the failed answer rather than off the missing question.
            .or_else(|| self.shell_pid.and_then(|p| process_cwd(p as i32)))
            // And failing that, whatever the shell reported for itself: OSC 7
            // needs a shell configured to send it, so it is the last resort
            // rather than the source.
            .or_else(|| {
                let pwd = self.inner.lock().ok().map(|g| g.terminal.pwd())?;
                (!pwd.is_empty()).then_some(pwd)
            })
            .unwrap_or_default()
    }

    /// When a byte last went either way, in unix milliseconds.
    pub fn last_active(&self) -> u64 {
        self.last_active.load(Ordering::Relaxed)
    }

    /// What is holding the terminal: the running command, or the shell.
    ///
    /// The same pid `cwd` and `is_busy` ask about, asked a third question. A
    /// tab whose program titles itself something the reader cannot place —
    /// or titles itself the same thing in six tabs at once — still knows how
    /// to say what it is.
    pub fn command(&self) -> String {
        self.foreground_pid()
            .and_then(process_name)
            .or_else(|| self.shell_pid.and_then(|p| process_name(p as i32)))
            .unwrap_or_default()
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
