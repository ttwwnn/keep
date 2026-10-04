//! Wire protocol between the daemon and its clients.
//!
//! Frames are `[tag: u8][len: u32 be][payload]`. The encoding is hand-rolled
//! and deliberately dull: this runs on a local socket carrying keystrokes and
//! screen bytes, so the useful properties are a small dependency surface and
//! framing that cannot desynchronise, not extensibility.

use std::io::{self, IoSlice, Read, Write};

pub mod net;
#[cfg(windows)]
mod pipe;

/// Refuse absurd frames rather than trying to allocate them.
pub const MAX_FRAME: usize = 16 * 1024 * 1024;

const T_LIST: u8 = 0x01;
const T_ATTACH: u8 = 0x02;
const T_NEW_TAB: u8 = 0x03;
const T_RESIZE: u8 = 0x05;
const T_KILL: u8 = 0x06;
const T_CLOSE_TAB: u8 = 0x07;
const T_PREVIEW: u8 = 0x08;
const T_SEARCH: u8 = 0x09;
const T_REARRANGE: u8 = 0x0a;
const T_PREVIEW_VT: u8 = 0x0b;
const T_MOVE_TAB: u8 = 0x0c;
// The same question as `T_LIST`, asked by a client that can read the two
// fields `TabInfo` grew. Renumbered rather than extended for the reason the
// blob tags below were: tab records sit back to back inside one payload with
// no length of their own, so a decoder that stops short of the new fields
// reads the next tab's id out of the middle of this one.
const T_LIST2: u8 = 0x0d;

const T_WORKSPACES: u8 = 0x81;
const T_ERROR: u8 = 0x84;
const T_OK: u8 = 0x85;
const T_ENDED: u8 = 0x86;
const T_ATTACHED: u8 = 0x87;
const T_TAB_CREATED: u8 = 0x88;
const T_PREVIEW_TEXT: u8 = 0x89;
// Renumbered when hits gained their history's length: a client that
// expects the field must not read a hit that predates it.
const T_SEARCH_HITS: u8 = 0x9a;
/// The answer to [`T_LIST2`], with `cwd` and `last_active` on every tab.
const T_WORKSPACES2: u8 = 0x9b;

// Blob frames carry their payload raw, with no length inside it — the frame
// header already has one. They were renumbered when that redundant length was
// dropped, and the numbers are the version gate: the daemon outlives every
// client, so a client built before the change will meet a daemon built after
// it. Reusing 0x04/0x82/0x83 would have had each side read the other's first
// four payload bytes as a length and desynchronise the stream silently.
// A tag nobody knows is refused at the frame, which is a clean failure.
const T_INPUT: u8 = 0x14;
const T_REPAINT: u8 = 0x92;
const T_OUTPUT: u8 = 0x93;

/// Ask for whichever tab the session lands on, rather than a specific one.
pub const TAB_ANY: u32 = 0;

/// Split placement for a new tab.
pub const SPLIT_NONE: u8 = 0;
pub const SPLIT_RIGHT: u8 = 1;
pub const SPLIT_DOWN: u8 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientMsg {
    List,
    /// [`ClientMsg::List`] from a client that wants `cwd` and `last_active`
    /// too. A daemon that predates them answers "unknown client tag" and
    /// closes, which is the caller's cue to ask the old question instead.
    List2,
    /// Attach to a tab. `tab` may be [`TAB_ANY`], meaning "the first live tab,
    /// creating one if the workspace has none". The workspace itself is
    /// created on demand.
    Attach { workspace: String, tab: u32, cols: u16, rows: u16 },
    /// `split_of` other than [`TAB_ANY`] makes this tab a pane of that tab,
    /// placed per `split_dir`. The arrangement lives in the daemon so a
    /// reattaching client rebuilds the same layout.
    NewTab {
        workspace: String,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
        split_of: u32,
        split_dir: u8,
    },
    CloseTab { workspace: String, tab: u32 },
    /// The tab's screen as plain text. For showing what a tab is doing
    /// without attaching to it — the daemon already holds the grid, so this
    /// costs a snapshot rather than a client.
    Preview { workspace: String, tab: u32 },
    /// The same screen, as VT sequences rather than as text.
    ///
    /// For an embedder that draws the preview itself and wants it to look
    /// like the terminal it is a picture of. A daemon that predates this
    /// answers "unknown client tag", which is the caller's cue to ask for
    /// the plain one instead.
    PreviewVt { workspace: String, tab: u32 },
    /// Look for `query` in history. The daemon holds the text — including for
    /// tabs no client has ever opened — so it is the only place that can
    /// answer this once rather than once per client.
    ///
    /// An empty `workspace` searches everything; naming one with a tab scopes
    /// the search to that pane, which is the difference between "find it
    /// anywhere" and "find it here".
    Search { query: String, limit: u32, workspace: String, tab: u32 },
    /// Put panes somewhere else in the arrangement, all at once.
    ///
    /// The layout lives here rather than in the app so that it survives every
    /// client, which means moving a pane is something only the daemon can do.
    Rearrange { workspace: String, moves: Vec<PaneMove> },
    Input(Vec<u8>),
    Resize { cols: u16, rows: u16 },
    /// End a whole workspace, tabs and all.
    Kill { workspace: String },
    /// Move a tab — panes and all — to another workspace. The shells behind
    /// it carry on untouched; only the filing changes. Answered with
    /// [`ServerMsg::TabCreated`] carrying the tab's id in its new home.
    MoveTab { workspace: String, tab: u32, to: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabInfo {
    pub id: u32,
    pub cols: u16,
    pub rows: u16,
    pub clients: u32,
    pub finished: bool,
    /// What the program inside called itself (OSC 0/2). Empty when it has not
    /// said, which is why callers need a fallback label.
    pub title: String,
    /// A command is running, as opposed to a shell waiting at its prompt.
    pub busy: bool,
    /// The tab this one is a pane of, or [`TAB_ANY`] for a standalone tab.
    pub split_of: u32,
    pub split_dir: u8,
    /// Where the tab's foreground process is working: the command's directory
    /// while one runs, the shell's at a prompt. Empty when it could not be
    /// read — a process that exited between the two calls, or one owned by
    /// another user. Only travels on [`T_WORKSPACES2`].
    pub cwd: String,
    /// When a byte last went either way through the tab, in unix
    /// milliseconds. Zero means unknown, which is what a client reading a
    /// [`T_WORKSPACES`] frame gets.
    pub last_active: u64,
    /// What is holding the terminal: the running command's name, or the
    /// shell's at a prompt. Empty when it could not be read.
    ///
    /// A title says what a program calls itself, which it may not do, and
    /// which several tabs may say identically. This says what it *is*.
    pub command: String,
}

/// One line of history that matched, where it lives, and enough around it to
/// recognise the place without going there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub workspace: String,
    pub tab: u32,
    /// Line number within that tab's history, counting from its oldest.
    pub line: u32,
    /// How many lines that history holds, so a client can turn `line` into a
    /// distance from the newest line. The old end of a history is where two
    /// terminals disagree — one trims what the other still has — so the end
    /// they share is the only sound thing to count from.
    pub total: u32,
    pub text: String,
    /// Where the match sits inside `text`, in bytes, so the caller can mark
    /// exactly what matched rather than guessing by searching again.
    pub match_start: u32,
    pub match_len: u32,
    /// The lines immediately before and after, oldest first.
    pub before: Vec<String>,
    pub after: Vec<String>,
}

/// Where one pane should end up: the tab it hangs off (0 = a tab of its own)
/// and how it sits against it. The unit of rearranging, plural because a
/// rearrangement that lands halfway is a layout nobody asked for — dropping a
/// pane to the left of another moves both, and either both land or neither
/// does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneMove {
    pub tab: u32,
    pub split_of: u32,
    pub split_dir: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInfo {
    pub name: String,
    pub tabs: Vec<TabInfo>,
}

impl WorkspaceInfo {
    pub fn clients(&self) -> u32 {
        self.tabs.iter().map(|t| t.clients).sum()
    }

    /// Whether any tab is running something.
    pub fn busy(&self) -> bool {
        self.tabs.iter().any(|t| t.busy && !t.finished)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerMsg {
    Workspaces(Vec<WorkspaceInfo>),
    /// The same list with each tab's `cwd` and `last_active` included. Sent
    /// only in answer to [`ClientMsg::List2`], so a client that cannot read
    /// them never meets one.
    Workspaces2(Vec<WorkspaceInfo>),
    /// Which tab the attach landed on. Sent before the repaint, because a
    /// client that asked for [`TAB_ANY`] does not know yet.
    Attached { tab: u32 },
    TabCreated { tab: u32 },
    /// The screen as it stood at attach time.
    Repaint(Vec<u8>),
    Output(Vec<u8>),
    Error(String),
    Ok,
    /// The tab's child exited.
    Ended,
    /// The screen a [`ClientMsg::Preview`] asked for. Empty when the tab is
    /// gone, which the caller shows as nothing rather than as an error.
    PreviewText(String),
    SearchHits(Vec<SearchHit>),
}

// ---------------------------------------------------------------- encoding

struct Buf(Vec<u8>);

impl Buf {
    fn new() -> Self {
        Self(Vec::new())
    }
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    fn bytes(&mut self, v: &[u8]) {
        self.u32(v.len() as u32);
        self.0.extend_from_slice(v);
    }
    fn str(&mut self, v: &str) {
        self.bytes(v.as_bytes());
    }
    fn bool(&mut self, v: bool) {
        self.0.push(v as u8);
    }
}

struct Cursor<'a>(&'a [u8]);

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        if self.0.len() < n {
            return Err(bad("truncated frame"));
        }
        let (head, tail) = self.0.split_at(n);
        self.0 = tail;
        Ok(head)
    }
    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> io::Result<u16> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn bytes(&mut self) -> io::Result<Vec<u8>> {
        let n = self.u32()? as usize;
        Ok(self.take(n)?.to_vec())
    }
    fn str(&mut self) -> io::Result<String> {
        String::from_utf8(self.bytes()?).map_err(|_| bad("invalid utf-8"))
    }
    fn bool(&mut self) -> io::Result<bool> {
        Ok(self.u8()? != 0)
    }
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

/// The workspace list, in one of its two layouts.
///
/// Written once rather than twice: the layouts differ by two fields at the
/// tail of each tab record, and two copies of this loop would be two places
/// for the next field to be added to only one of.
fn write_workspaces(b: &mut Buf, list: &[WorkspaceInfo], with_cwd: bool) {
    b.u32(list.len() as u32);
    for s in list {
        b.str(&s.name);
        b.u32(s.tabs.len() as u32);
        for t in &s.tabs {
            b.u32(t.id);
            b.u16(t.cols);
            b.u16(t.rows);
            b.u32(t.clients);
            b.bool(t.finished);
            b.str(&t.title);
            b.bool(t.busy);
            b.u32(t.split_of);
            b.u8(t.split_dir);
            if with_cwd {
                b.str(&t.cwd);
                b.u64(t.last_active);
                b.str(&t.command);
            }
        }
    }
}

fn read_workspaces(c: &mut Cursor, with_cwd: bool) -> io::Result<Vec<WorkspaceInfo>> {
    let n = c.u32()? as usize;
    let mut list = Vec::with_capacity(n.min(1024));
    for _ in 0..n {
        let name = c.str()?;
        let tab_count = c.u32()? as usize;
        let mut tabs = Vec::with_capacity(tab_count.min(1024));
        for _ in 0..tab_count {
            let mut tab = TabInfo {
                id: c.u32()?,
                cols: c.u16()?,
                rows: c.u16()?,
                clients: c.u32()?,
                finished: c.bool()?,
                title: c.str()?,
                busy: c.bool()?,
                split_of: c.u32()?,
                split_dir: c.u8()?,
                cwd: String::new(),
                last_active: 0,
                command: String::new(),
            };
            if with_cwd {
                tab.cwd = c.str()?;
                tab.last_active = c.u64()?;
                tab.command = c.str()?;
            }
            tabs.push(tab);
        }
        list.push(WorkspaceInfo { name, tabs });
    }
    Ok(list)
}

fn write_frame(w: &mut impl Write, tag: u8, payload: &[u8]) -> io::Result<()> {
    let mut head = [0u8; 5];
    head[0] = tag;
    head[1..].copy_from_slice(&(payload.len() as u32).to_be_bytes());
    if payload.is_empty() {
        w.write_all(&head)?;
        return w.flush();
    }
    // Header and payload leave in one syscall. Two `write_all`s would double
    // the syscall count on the path that carries every byte of terminal
    // output. A vectored write may still go short, so finish it by hand
    // rather than assuming it wrote everything.
    let total = head.len() + payload.len();
    let mut done = 0usize;
    while done < total {
        let result = if done < head.len() {
            w.write_vectored(&[IoSlice::new(&head[done..]), IoSlice::new(payload)])
        } else {
            w.write(&payload[done - head.len()..])
        };
        match result {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(n) => done += n,
            // `write_all` retries this and we replaced it, so retry it here
            // too: a signal arriving mid-write must not tear the connection
            // down.
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    w.flush()
}

/// Read one frame. `Ok(None)` means the peer closed cleanly between frames.
fn read_frame(r: &mut impl Read) -> io::Result<Option<(u8, Vec<u8>)>> {
    let mut head = [0u8; 5];
    match r.read_exact(&mut head) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_be_bytes(head[1..].try_into().unwrap()) as usize;
    if len > MAX_FRAME {
        return Err(bad("frame too large"));
    }
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)?;
    Ok(Some((head[0], payload)))
}

impl ClientMsg {
    /// Write an input frame straight from borrowed bytes.
    ///
    /// Equivalent to writing [`ClientMsg::Input`], without owning the bytes to
    /// do it — which on a paste means not copying the whole clipboard first.
    pub fn write_input(w: &mut impl Write, data: &[u8]) -> io::Result<()> {
        write_frame(w, T_INPUT, data)
    }

    pub fn write(&self, w: &mut impl Write) -> io::Result<()> {
        let mut b = Buf::new();
        let tag = match self {
            ClientMsg::List => T_LIST,
            ClientMsg::List2 => T_LIST2,
            ClientMsg::Attach { workspace, tab, cols, rows } => {
                b.str(workspace);
                b.u32(*tab);
                b.u16(*cols);
                b.u16(*rows);
                T_ATTACH
            }
            ClientMsg::NewTab { workspace, cwd, cols, rows, split_of, split_dir } => {
                b.str(workspace);
                b.str(cwd.as_deref().unwrap_or(""));
                b.u16(*cols);
                b.u16(*rows);
                b.u32(*split_of);
                b.0.push(*split_dir);
                T_NEW_TAB
            }
            ClientMsg::CloseTab { workspace, tab } => {
                b.str(workspace);
                b.u32(*tab);
                T_CLOSE_TAB
            }
            ClientMsg::Preview { workspace, tab } => {
                b.str(workspace);
                b.u32(*tab);
                T_PREVIEW
            }
            ClientMsg::PreviewVt { workspace, tab } => {
                b.str(workspace);
                b.u32(*tab);
                T_PREVIEW_VT
            }
            ClientMsg::Search { query, limit, workspace, tab } => {
                b.str(query);
                b.u32(*limit);
                b.str(workspace);
                b.u32(*tab);
                T_SEARCH
            }
            // The frame header already carries the length, so a blob payload
            // goes straight out instead of through `Buf`. That spare copy
            // would otherwise land on every byte typed or pasted.
            ClientMsg::Input(data) => return write_frame(w, T_INPUT, data),
            ClientMsg::Rearrange { workspace, moves } => {
                b.str(workspace);
                b.u32(moves.len() as u32);
                for m in moves {
                    b.u32(m.tab);
                    b.u32(m.split_of);
                    b.u8(m.split_dir);
                }
                T_REARRANGE
            }
            ClientMsg::Resize { cols, rows } => {
                b.u16(*cols);
                b.u16(*rows);
                T_RESIZE
            }
            ClientMsg::Kill { workspace } => {
                b.str(workspace);
                T_KILL
            }
            ClientMsg::MoveTab { workspace, tab, to } => {
                b.str(workspace);
                b.u32(*tab);
                b.str(to);
                T_MOVE_TAB
            }
        };
        write_frame(w, tag, &b.0)
    }

    pub fn read(r: &mut impl Read) -> io::Result<Option<Self>> {
        let Some((tag, payload)) = read_frame(r)? else { return Ok(None) };
        // A blob payload *is* the frame, so move the buffer rather than
        // copying it out of itself.
        if tag == T_INPUT {
            return Ok(Some(ClientMsg::Input(payload)));
        }
        let mut c = Cursor(&payload);
        let msg = match tag {
            T_LIST => ClientMsg::List,
            T_LIST2 => ClientMsg::List2,
            T_ATTACH => ClientMsg::Attach {
                workspace: c.str()?,
                tab: c.u32()?,
                cols: c.u16()?,
                rows: c.u16()?,
            },
            T_NEW_TAB => {
                let workspace = c.str()?;
                let cwd = c.str()?;
                ClientMsg::NewTab {
                    workspace,
                    cwd: if cwd.is_empty() { None } else { Some(cwd) },
                    cols: c.u16()?,
                    rows: c.u16()?,
                    split_of: c.u32()?,
                    split_dir: c.u8()?,
                }
            }
            T_CLOSE_TAB => ClientMsg::CloseTab { workspace: c.str()?, tab: c.u32()? },
            T_PREVIEW => ClientMsg::Preview { workspace: c.str()?, tab: c.u32()? },
            T_PREVIEW_VT => ClientMsg::PreviewVt { workspace: c.str()?, tab: c.u32()? },
            T_MOVE_TAB => {
                ClientMsg::MoveTab { workspace: c.str()?, tab: c.u32()?, to: c.str()? }
            }
            T_SEARCH => ClientMsg::Search {
                query: c.str()?,
                limit: c.u32()?,
                workspace: c.str()?,
                tab: c.u32()?,
            },
            T_REARRANGE => {
                let workspace = c.str()?;
                let count = c.u32()?;
                let mut moves = Vec::with_capacity((count as usize).min(4096));
                for _ in 0..count {
                    moves.push(PaneMove {
                        tab: c.u32()?,
                        split_of: c.u32()?,
                        split_dir: c.u8()?,
                    });
                }
                ClientMsg::Rearrange { workspace, moves }
            }
            T_RESIZE => ClientMsg::Resize { cols: c.u16()?, rows: c.u16()? },
            T_KILL => ClientMsg::Kill { workspace: c.str()? },
            _ => return Err(bad("unknown client tag")),
        };
        Ok(Some(msg))
    }
}

impl ServerMsg {
    /// Write an output frame straight from borrowed bytes.
    ///
    /// The daemon hands one chunk to every client watching a tab, all sharing
    /// the same allocation. Building a [`ServerMsg::Output`] to write it would
    /// undo that by copying the chunk once per client — which is the cost the
    /// sharing exists to avoid.
    pub fn write_output(w: &mut impl Write, data: &[u8]) -> io::Result<()> {
        write_frame(w, T_OUTPUT, data)
    }

    pub fn write(&self, w: &mut impl Write) -> io::Result<()> {
        let mut b = Buf::new();
        let tag = match self {
            ServerMsg::Workspaces(list) => {
                write_workspaces(&mut b, list, false);
                T_WORKSPACES
            }
            ServerMsg::Workspaces2(list) => {
                write_workspaces(&mut b, list, true);
                T_WORKSPACES2
            }
            ServerMsg::Attached { tab } => {
                b.u32(*tab);
                T_ATTACHED
            }
            ServerMsg::TabCreated { tab } => {
                b.u32(*tab);
                T_TAB_CREATED
            }
            // As with `ClientMsg::Input`: the frame is the payload, and this
            // is the path every byte the terminal produces travels down.
            ServerMsg::Repaint(data) => return write_frame(w, T_REPAINT, data),
            ServerMsg::Output(data) => return write_frame(w, T_OUTPUT, data),
            ServerMsg::Error(msg) => {
                b.str(msg);
                T_ERROR
            }
            ServerMsg::PreviewText(text) => {
                b.str(text);
                T_PREVIEW_TEXT
            }
            ServerMsg::SearchHits(hits) => {
                b.u32(hits.len() as u32);
                for hit in hits {
                    b.str(&hit.workspace);
                    b.u32(hit.tab);
                    b.u32(hit.line);
                    b.u32(hit.total);
                    b.str(&hit.text);
                    b.u32(hit.match_start);
                    b.u32(hit.match_len);
                    b.u32(hit.before.len() as u32);
                    for line in &hit.before {
                        b.str(line);
                    }
                    b.u32(hit.after.len() as u32);
                    for line in &hit.after {
                        b.str(line);
                    }
                }
                T_SEARCH_HITS
            }
            ServerMsg::Ok => T_OK,
            ServerMsg::Ended => T_ENDED,
        };
        write_frame(w, tag, &b.0)
    }

    pub fn read(r: &mut impl Read) -> io::Result<Option<Self>> {
        let Some((tag, payload)) = read_frame(r)? else { return Ok(None) };
        if tag == T_OUTPUT {
            return Ok(Some(ServerMsg::Output(payload)));
        }
        if tag == T_REPAINT {
            return Ok(Some(ServerMsg::Repaint(payload)));
        }
        let mut c = Cursor(&payload);
        let msg = match tag {
            T_WORKSPACES => ServerMsg::Workspaces(read_workspaces(&mut c, false)?),
            T_WORKSPACES2 => ServerMsg::Workspaces2(read_workspaces(&mut c, true)?),
            T_ATTACHED => ServerMsg::Attached { tab: c.u32()? },
            T_TAB_CREATED => ServerMsg::TabCreated { tab: c.u32()? },
            T_ERROR => ServerMsg::Error(c.str()?),
            T_PREVIEW_TEXT => ServerMsg::PreviewText(c.str()?),
            T_SEARCH_HITS => {
                let n = c.u32()? as usize;
                let mut hits = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    let workspace = c.str()?;
                    let tab = c.u32()?;
                    let line = c.u32()?;
                    let total = c.u32()?;
                    let text = c.str()?;
                    let match_start = c.u32()?;
                    let match_len = c.u32()?;
                    let mut before = Vec::new();
                    for _ in 0..c.u32()? {
                        before.push(c.str()?);
                    }
                    let mut after = Vec::new();
                    for _ in 0..c.u32()? {
                        after.push(c.str()?);
                    }
                    hits.push(SearchHit {
                        workspace, tab, line, total, text, match_start, match_len, before, after,
                    });
                }
                ServerMsg::SearchHits(hits)
            }
            T_OK => ServerMsg::Ok,
            T_ENDED => ServerMsg::Ended,
            _ => return Err(bad("unknown server tag")),
        };
        Ok(Some(msg))
    }
}

/// Where the daemon listens. Honours `KEEP_SOCKET` so tests never collide
/// with a real daemon the developer is running.
pub fn socket_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("KEEP_SOCKET") {
        // On Windows the address is a pipe name, and anything else given
        // here is folded into one; see `pipe::name_for`.
        #[cfg(windows)]
        return pipe::name_for(std::path::Path::new(&p));
        #[cfg(not(windows))]
        return p.into();
    }
    default_socket_path()
}

/// A named pipe rather than a file: see `net`. The name carries the user's
/// SID for the same reason the socket elsewhere carries their name.
#[cfg(windows)]
fn default_socket_path() -> std::path::PathBuf {
    pipe::default_name()
}

#[cfg(not(windows))]
fn default_socket_path() -> std::path::PathBuf {
    let base = std::env::var("XDG_RUNTIME_DIR")
        .or_else(|_| std::env::var("TMPDIR"))
        .unwrap_or_else(|_| "/tmp".into());
    // Keyed by user so two accounts on one machine never share a socket.
    // $UID is not exported by most shells, so use the name instead.
    let who = std::env::var("USER").unwrap_or_else(|_| "default".into());
    std::path::Path::new(&base).join(format!("keep-{who}.sock"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip_client(msg: ClientMsg) {
        let mut buf = Vec::new();
        msg.write(&mut buf).unwrap();
        let back = ClientMsg::read(&mut buf.as_slice()).unwrap().unwrap();
        assert_eq!(msg, back);
    }

    fn roundtrip_server(msg: ServerMsg) {
        let mut buf = Vec::new();
        msg.write(&mut buf).unwrap();
        let back = ServerMsg::read(&mut buf.as_slice()).unwrap().unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn client_messages_round_trip() {
        roundtrip_client(ClientMsg::List);
        roundtrip_client(ClientMsg::Rearrange { workspace: "a".into(), moves: vec![] });
        roundtrip_client(ClientMsg::Rearrange {
            workspace: "b é".into(),
            moves: vec![
                PaneMove { tab: 4, split_of: 0, split_dir: 0 },
                PaneMove { tab: 7, split_of: 4, split_dir: 2 },
            ],
        });
        roundtrip_client(ClientMsg::Attach {
            workspace: "www".into(),
            tab: 3,
            cols: 120,
            rows: 40,
        });
        roundtrip_client(ClientMsg::Attach {
            workspace: "www".into(),
            tab: TAB_ANY,
            cols: 80,
            rows: 24,
        });
        roundtrip_client(ClientMsg::NewTab {
            workspace: "proj".into(),
            cwd: Some("/Users/x/y z".into()),
            cols: 80,
            rows: 24,
            split_of: TAB_ANY,
            split_dir: SPLIT_NONE,
        });
        roundtrip_client(ClientMsg::NewTab {
            workspace: "n".into(),
            cwd: None,
            cols: 1,
            rows: 1,
            split_of: 7,
            split_dir: SPLIT_DOWN,
        });
        roundtrip_client(ClientMsg::CloseTab { workspace: "proj".into(), tab: 7 });
        roundtrip_client(ClientMsg::Preview { workspace: "proj".into(), tab: 2 });
        roundtrip_client(ClientMsg::PreviewVt { workspace: "proj".into(), tab: 2 });
        roundtrip_client(ClientMsg::MoveTab {
            workspace: "proj".into(),
            tab: 2,
            to: "other".into(),
        });
        roundtrip_client(ClientMsg::Search {
            query: "error".into(), limit: 200, workspace: String::new(), tab: 0,
        });
        roundtrip_client(ClientMsg::Search {
            query: "é".into(), limit: 1, workspace: "www".into(), tab: 4,
        });
        roundtrip_client(ClientMsg::Input(vec![0x1b, b'[', b'A', 0x00, 0xff]));
        roundtrip_client(ClientMsg::Resize { cols: 65535, rows: 1 });
        roundtrip_client(ClientMsg::Kill { workspace: "gone".into() });
    }

    #[test]
    fn server_messages_round_trip() {
        roundtrip_server(ServerMsg::Ok);
        roundtrip_server(ServerMsg::Ended);
        roundtrip_server(ServerMsg::Error("no such session".into()));
        roundtrip_server(ServerMsg::PreviewText("$ cargo test\n   ok".into()));
        roundtrip_server(ServerMsg::PreviewText(String::new()));
        roundtrip_server(ServerMsg::SearchHits(vec![]));
        roundtrip_server(ServerMsg::SearchHits(vec![
            SearchHit {
                workspace: "www".into(), tab: 3, line: 812, total: 4096, text: "error: é".into(),
                match_start: 0, match_len: 5,
                before: vec!["one".into(), "two".into()],
                after: vec!["three".into()],
            },
            SearchHit {
                workspace: "a".into(), tab: 1, line: 0, total: 1, text: String::new(),
                match_start: 0, match_len: 0, before: vec![], after: vec![],
            },
        ]));
        roundtrip_server(ServerMsg::Output(vec![0; 1000]));
        roundtrip_server(ServerMsg::Repaint(b"\x1b[2J\x1b[Hhi".to_vec()));
        roundtrip_server(ServerMsg::Attached { tab: 4 });
        roundtrip_server(ServerMsg::TabCreated { tab: 9 });
        roundtrip_server(ServerMsg::Workspaces(vec![
            WorkspaceInfo { name: "a".into(), tabs: vec![] },
            WorkspaceInfo {
                name: "b é".into(),
                tabs: vec![
                    TabInfo {
                        id: 1,
                        cols: 80,
                        rows: 24,
                        clients: 0,
                        finished: false,
                        title: String::new(),
                        busy: false,
                        split_of: TAB_ANY,
                        split_dir: SPLIT_NONE,
                        cwd: String::new(),
                        last_active: 0,
                        command: String::new(),
                    },
                    TabInfo {
                        id: 2,
                        cols: 100,
                        rows: 30,
                        clients: 2,
                        finished: true,
                        title: "nvim src/main.rs".into(),
                        busy: true,
                        split_of: 1,
                        split_dir: SPLIT_RIGHT,
                        cwd: String::new(),
                        last_active: 0,
                        command: String::new(),
                    },
                ],
            },
        ]));
    }

    /// The v2 list carries the three fields the v1 one drops.
    ///
    /// Both directions matter and neither is the other's mirror: a v2 frame
    /// must bring `cwd`, `last_active` and `command` home intact, and a v1
    /// frame must arrive with them emptied rather than read short.
    #[test]
    fn workspaces2_carries_cwd_and_last_active() {
        let tabs = vec![
            TabInfo {
                id: 1,
                cols: 80,
                rows: 24,
                clients: 1,
                finished: false,
                title: "✳ building".into(),
                busy: true,
                split_of: TAB_ANY,
                split_dir: SPLIT_NONE,
                cwd: "/Users/someone/www/projeto/api".into(),
                last_active: 1_756_600_000_123,
                command: "claude".into(),
            },
            TabInfo {
                id: 2,
                cols: 80,
                rows: 24,
                clients: 0,
                finished: false,
                title: String::new(),
                busy: false,
                split_of: 1,
                split_dir: SPLIT_DOWN,
                cwd: String::new(),
                last_active: u64::MAX,
                command: String::new(),
            },
        ];
        let list = vec![WorkspaceInfo { name: "projeto".into(), tabs: tabs.clone() }];
        roundtrip_server(ServerMsg::Workspaces2(list.clone()));

        // Down the old tag, the same tabs come back stripped of both.
        let mut buf = Vec::new();
        ServerMsg::Workspaces(list).write(&mut buf).unwrap();
        let mut r = &buf[..];
        let Some(ServerMsg::Workspaces(back)) = ServerMsg::read(&mut r).unwrap() else {
            panic!("expected a v1 workspace list")
        };
        assert_eq!(back[0].tabs.len(), 2);
        assert_eq!(back[0].tabs[0].title, "✳ building");
        assert_eq!(back[0].tabs[0].split_dir, SPLIT_NONE);
        assert_eq!(back[0].tabs[1].id, 2, "a short read would find this in the wrong place");
        for tab in &back[0].tabs {
            assert_eq!(tab.command, "");
            assert_eq!(tab.cwd, "");
            assert_eq!(tab.last_active, 0);
        }
    }

    #[test]
    fn workspace_client_count_sums_its_tabs() {
        let s = WorkspaceInfo {
            name: "x".into(),
            tabs: vec![
                TabInfo {
                    id: 1,
                    cols: 80,
                    rows: 24,
                    clients: 2,
                    finished: false,
                    title: String::new(),
                    busy: false,
                    split_of: TAB_ANY,
                    split_dir: SPLIT_NONE,
                    cwd: String::new(),
                    last_active: 0,
                    command: String::new(),
                },
                TabInfo {
                    id: 2,
                    cols: 80,
                    rows: 24,
                    clients: 1,
                    finished: false,
                    title: String::new(),
                    busy: false,
                    split_of: TAB_ANY,
                    split_dir: SPLIT_NONE,
                    cwd: String::new(),
                    last_active: 0,
                    command: String::new(),
                },
            ],
        };
        assert_eq!(s.clients(), 3);
    }

    /// A blob payload is the frame and nothing else.
    ///
    /// The frame header already carries the length, so a second one inside the
    /// payload buys nothing and costs a copy of every byte of terminal output.
    /// This pins that: 5 bytes of header, then the data.
    #[test]
    fn blob_frames_carry_no_second_length() {
        let mut buf = Vec::new();
        ServerMsg::Output(vec![7u8; 1000]).write(&mut buf).unwrap();
        assert_eq!(buf.len(), 5 + 1000, "output frame is not header + payload");
        assert_eq!(&buf[5..], &[7u8; 1000][..]);

        let mut buf = Vec::new();
        ClientMsg::Input(vec![9u8; 40]).write(&mut buf).unwrap();
        assert_eq!(buf.len(), 5 + 40, "input frame is not header + payload");

        // The borrowed writers must produce the very same bytes.
        let mut borrowed = Vec::new();
        ClientMsg::write_input(&mut borrowed, &[9u8; 40]).unwrap();
        assert_eq!(borrowed, buf);
    }

    /// The blob tags must never go back to the values that meant the old
    /// layout. A daemon outlives its clients, so the two formats meet.
    #[test]
    fn blob_tags_do_not_reuse_the_old_numbers() {
        for (new, old) in [(T_INPUT, 0x04), (T_REPAINT, 0x82), (T_OUTPUT, 0x83)] {
            assert_ne!(new, old, "a blob tag was reset to its pre-change value");
        }
        // And an old peer's frame is refused rather than misread.
        let mut buf = vec![0x04u8];
        buf.extend_from_slice(&8u32.to_be_bytes());
        buf.extend_from_slice(&[0, 0, 0, 4, b'a', b'b', b'c', b'd']);
        let err = ClientMsg::read(&mut buf.as_slice()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    /// Framing must survive several messages back to back on one stream.
    #[test]
    fn frames_do_not_desynchronise() {
        let mut buf = Vec::new();
        ClientMsg::Input(vec![1, 2, 3]).write(&mut buf).unwrap();
        ClientMsg::Resize { cols: 10, rows: 20 }.write(&mut buf).unwrap();
        ClientMsg::List.write(&mut buf).unwrap();

        let mut r = buf.as_slice();
        assert_eq!(ClientMsg::read(&mut r).unwrap().unwrap(), ClientMsg::Input(vec![1, 2, 3]));
        assert_eq!(
            ClientMsg::read(&mut r).unwrap().unwrap(),
            ClientMsg::Resize { cols: 10, rows: 20 }
        );
        assert_eq!(ClientMsg::read(&mut r).unwrap().unwrap(), ClientMsg::List);
        assert_eq!(ClientMsg::read(&mut r).unwrap(), None, "clean end of stream");
    }

    #[test]
    fn oversized_frame_is_refused_not_allocated() {
        let mut buf = vec![T_INPUT];
        buf.extend_from_slice(&(u32::MAX).to_be_bytes());
        let err = ClientMsg::read(&mut buf.as_slice()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
