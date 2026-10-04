//! Finding the daemon, starting it, and asking it things.
//!
//! The app is a viewer, as on macOS: the daemon owns every shell and outlives
//! the window, so closing the app loses nothing. When it is not running — the
//! first launch after a reboot — the app starts it, detached, and from then on
//! it belongs to nobody.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use keep_proto::net::Stream;
use keep_proto::{ClientMsg, PaneMove, SearchHit, ServerMsg, TabInfo, WorkspaceInfo};
use serde::Serialize;

/// Where the daemon listens: `KEEP_SOCKET` if set, else the user's own pipe
/// (Windows) or socket (elsewhere).
pub fn address() -> PathBuf {
    keep_proto::socket_path()
}

pub fn is_running() -> bool {
    Stream::connect(address()).is_ok()
}

/// The daemon's executable: `KEEPD_BIN`, then the copy installed with the
/// app, then one beside the app's own executable.
pub fn binary(resources: Option<&Path>) -> Option<PathBuf> {
    if let Ok(path) = std::env::var("KEEPD_BIN") {
        return Some(PathBuf::from(path));
    }
    let name = format!("keepd{}", std::env::consts::EXE_SUFFIX);
    let mut candidates = Vec::new();
    if let Some(dir) = resources {
        candidates.push(dir.join("bin").join(&name));
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        candidates.push(dir.join("bin").join(&name));
        candidates.push(dir.join(&name));
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// Start the daemon unless it already answers, and wait until it does.
pub fn ensure(resources: Option<&Path>) -> Result<()> {
    if is_running() {
        return Ok(());
    }
    let exe = binary(resources).ok_or_else(|| anyhow!("keepd não encontrado junto do app"))?;
    spawn(&exe).with_context(|| format!("iniciar {}", exe.display()))?;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        if is_running() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    match last_failure() {
        Some(reason) => bail!("o keepd não respondeu: {reason}"),
        None => bail!("o keepd não respondeu em {}", address().display()),
    }
}

fn command(exe: &Path) -> std::process::Command {
    let mut cmd = std::process::Command::new(exe);
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    cmd
}

/// Detached from the app's console and job, so it outlives the app.
#[cfg(windows)]
fn spawn(exe: &Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    match command(exe).creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB).spawn() {
        Ok(_) => Ok(()),
        Err(_) => command(exe).creation_flags(flags).spawn().map(|_| ()),
    }
}

#[cfg(not(windows))]
fn spawn(exe: &Path) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    // Its own session: a terminal or a launcher that started the app does
    // not take the daemon with it when it goes.
    unsafe {
        command(exe)
            .pre_exec(|| {
                libc_setsid();
                Ok(())
            })
            .spawn()
            .map(|_| ())
    }
}

#[cfg(not(windows))]
fn libc_setsid() {
    unsafe extern "C" {
        fn setsid() -> i32;
    }
    unsafe {
        setsid();
    }
}

/// Why the daemon last refused to start, as it wrote it down.
fn last_failure() -> Option<String> {
    let dir = std::env::var("LOCALAPPDATA").ok()?;
    let text = std::fs::read_to_string(Path::new(&dir).join("Keep").join("keepd.log")).ok()?;
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// One question, one connection, one answer.
fn ask(question: ClientMsg) -> Result<ServerMsg> {
    let mut sock = Stream::connect(address()).context("conectar ao keepd")?;
    sock.set_read_timeout(Some(Duration::from_secs(10))).ok();
    question.write(&mut sock)?;
    ServerMsg::read(&mut sock)?.ok_or_else(|| anyhow!("o keepd fechou sem responder"))
}

fn ok_or_error(reply: ServerMsg) -> Result<()> {
    match reply {
        ServerMsg::Ok => Ok(()),
        ServerMsg::Error(e) => bail!(e),
        other => bail!("resposta inesperada: {other:?}"),
    }
}

/// What the interface is told about a tab. The protocol's own type with the
/// names spelled the way the JavaScript side reads them.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Tab {
    pub id: u32,
    pub cols: u16,
    pub rows: u16,
    pub clients: u32,
    pub finished: bool,
    pub title: String,
    pub busy: bool,
    pub split_of: u32,
    pub split_dir: u8,
    pub cwd: String,
    pub last_active: u64,
    pub command: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Workspace {
    pub name: String,
    pub tabs: Vec<Tab>,
}

impl From<TabInfo> for Tab {
    fn from(t: TabInfo) -> Self {
        Tab {
            id: t.id,
            cols: t.cols,
            rows: t.rows,
            clients: t.clients,
            finished: t.finished,
            title: t.title,
            busy: t.busy,
            split_of: t.split_of,
            split_dir: t.split_dir,
            cwd: t.cwd,
            last_active: t.last_active,
            command: t.command,
        }
    }
}

impl From<WorkspaceInfo> for Workspace {
    fn from(w: WorkspaceInfo) -> Self {
        Workspace { name: w.name, tabs: w.tabs.into_iter().map(Tab::from).collect() }
    }
}

pub fn list() -> Result<Vec<Workspace>> {
    match ask(ClientMsg::List2)? {
        ServerMsg::Workspaces2(list) | ServerMsg::Workspaces(list) => {
            Ok(list.into_iter().map(Workspace::from).collect())
        }
        ServerMsg::Error(e) => bail!(e),
        other => bail!("resposta inesperada: {other:?}"),
    }
}

pub fn new_tab(
    workspace: &str,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    split_of: u32,
    split_dir: u8,
) -> Result<u32> {
    let cwd = cwd.filter(|c| !c.trim().is_empty());
    match ask(ClientMsg::NewTab { workspace: workspace.into(), cwd, cols, rows, split_of, split_dir })? {
        ServerMsg::TabCreated { tab } => Ok(tab),
        ServerMsg::Error(e) => bail!(e),
        other => bail!("resposta inesperada: {other:?}"),
    }
}

pub fn close_tab(workspace: &str, tab: u32) -> Result<()> {
    ok_or_error(ask(ClientMsg::CloseTab { workspace: workspace.into(), tab })?)
}

pub fn kill(workspace: &str) -> Result<()> {
    ok_or_error(ask(ClientMsg::Kill { workspace: workspace.into() })?)
}

pub fn move_tab(workspace: &str, tab: u32, to: &str) -> Result<u32> {
    match ask(ClientMsg::MoveTab { workspace: workspace.into(), tab, to: to.into() })? {
        ServerMsg::TabCreated { tab } => Ok(tab),
        ServerMsg::Error(e) => bail!(e),
        other => bail!("resposta inesperada: {other:?}"),
    }
}

pub fn rearrange(workspace: &str, moves: Vec<PaneMove>) -> Result<()> {
    ok_or_error(ask(ClientMsg::Rearrange { workspace: workspace.into(), moves })?)
}

pub fn preview(workspace: &str, tab: u32) -> Result<String> {
    match ask(ClientMsg::Preview { workspace: workspace.into(), tab })? {
        ServerMsg::PreviewText(text) => Ok(text),
        ServerMsg::Error(e) => bail!(e),
        other => bail!("resposta inesperada: {other:?}"),
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub workspace: String,
    pub tab: u32,
    pub line: u32,
    pub total: u32,
    pub text: String,
    pub match_start: u32,
    pub match_len: u32,
    pub before: Vec<String>,
    pub after: Vec<String>,
}

impl From<SearchHit> for Hit {
    fn from(h: SearchHit) -> Self {
        Hit {
            workspace: h.workspace,
            tab: h.tab,
            line: h.line,
            total: h.total,
            text: h.text,
            match_start: h.match_start,
            match_len: h.match_len,
            before: h.before,
            after: h.after,
        }
    }
}

pub fn search(query: &str, limit: u32, workspace: &str, tab: u32) -> Result<Vec<Hit>> {
    match ask(ClientMsg::Search { query: query.into(), limit, workspace: workspace.into(), tab })? {
        ServerMsg::SearchHits(hits) => Ok(hits.into_iter().map(Hit::from).collect()),
        ServerMsg::Error(e) => bail!(e),
        other => bail!("resposta inesperada: {other:?}"),
    }
}
