//! A workspace: a named group of tabs.
//!
//! The workspace is what a person thinks of as "a project". It holds one or
//! more tabs, and it is the unit that survives clients coming and going. Tabs
//! live here rather than in the app so that closing the window loses nothing:
//! reopening finds the same tabs, still running.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use portable_pty::CommandBuilder;

use crate::Tab;

struct Entry {
    id: u32,
    tab: Arc<Tab>,
    /// The tab this one is a pane of (0 = standalone), and where it sits.
    /// Kept here, not in the app: the layout must survive every client.
    split_of: u32,
    split_dir: u8,
}

/// A snapshot of one tab and its place in the layout.
pub struct TabRef {
    pub id: u32,
    pub split_of: u32,
    pub split_dir: u8,
    pub tab: Arc<Tab>,
}

pub struct Workspace {
    name: String,
    tabs: Mutex<Vec<Entry>>,
    next_id: AtomicU32,
}

/// Give a departed tab's panes somewhere to belong.
///
/// A pane records the tab it was split from, and a pane whose record points at
/// nothing is reported standalone — so left alone, closing one pane would
/// scatter everything below it into tabs of its own.
///
/// One pane takes the departed's place: its parent and its orientation, so the
/// survivor lands in the slot the arrangement already had rather than halving
/// the parent afresh. The rest hang off that heir. A departed root has no slot
/// to take, so the heir becomes the root.
///
/// Returns the heir, where there was one to promote.
fn inherit(tabs: &mut [Entry], departed: u32, parent: u32, dir: u8) -> Option<u32> {
    // Creation order, so the oldest pane is the one promoted.
    let heir = tabs.iter().position(|e| e.split_of == departed)?;
    let heir_id = tabs[heir].id;
    tabs[heir].split_of = parent;
    tabs[heir].split_dir = dir;
    for entry in tabs.iter_mut() {
        if entry.split_of == departed && entry.id != heir_id {
            entry.split_of = heir_id;
        }
    }
    Some(heir_id)
}

/// Rehome the panes of everything that left, in one pass.
///
/// Departures arrive in groups — a shell exiting takes the shells of its panes
/// with it — and a group can hold both a pane and the pane it was split from.
/// Rehoming each against the record it kept would hand a survivor the id of
/// something else that left in the same pass, and a pane whose parent does not
/// exist is reported standalone: the arrangement would come apart at exactly
/// the moment a whole split finishes at once.
///
/// So each departed slot is resolved *through* the others first, up to the
/// first tab that is actually still there. Everything whose chain ends at the
/// same dead root is kept together under one heir instead of each orphan
/// becoming a tab.
fn rehome(tabs: &mut [Entry], departed: &HashMap<u32, (u32, u8)>) {
    let mut ids: Vec<u32> = departed.keys().copied().collect();
    ids.sort_unstable();

    // Dead root -> the pane promoted to stand in for it.
    let mut promoted: HashMap<u32, u32> = HashMap::new();

    for id in ids {
        let (own_parent, own_dir) = departed[&id];
        let (mut parent, mut dir) = (own_parent, own_dir);
        // The chain's last departed link, whose place the heir ends up taking.
        let mut top = id;
        // Ids only ever point backwards, so this cannot loop; bound it anyway
        // rather than trust a record to be well formed.
        let mut hops = departed.len();
        while let Some(&(grandparent, grandparent_dir)) = departed.get(&parent) {
            if hops == 0 {
                break;
            }
            hops -= 1;
            top = parent;
            parent = grandparent;
            dir = grandparent_dir;
        }

        if parent != 0 {
            inherit(tabs, id, parent, dir);
        } else if let Some(&heir) = promoted.get(&top) {
            // The root of this chain already has a stand-in; join it there,
            // keeping the orientation this pane was split with.
            inherit(tabs, id, heir, own_dir);
        } else if let Some(heir) = inherit(tabs, id, 0, 0) {
            promoted.insert(top, heir);
        }
    }
}

/// The shell a new tab runs.
#[cfg(unix)]
fn shell_command() -> CommandBuilder {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    CommandBuilder::new(shell)
}

/// The shell a new tab runs on Windows: the one named in `KEEP_SHELL`, or
/// in the file the app writes when it is chosen there, or else the best
/// PowerShell installed, or else `cmd`.
///
/// The file is read for every tab rather than once, so a shell chosen in the
/// app applies to the next tab without restarting the daemon that holds the
/// tabs already open.
#[cfg(windows)]
fn shell_command() -> CommandBuilder {
    let chosen = std::env::var("KEEP_SHELL")
        .ok()
        .or_else(|| std::fs::read_to_string(shell_setting_path()?).ok())
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty());
    let (program, args) = chosen
        .map(|line| crate::winproc::split_command_line(&line))
        .and_then(|args| {
            let (program, rest) = args.split_first()?;
            Some((program.clone(), rest.to_vec()))
        })
        .unwrap_or_else(default_shell);
    let mut cmd = CommandBuilder::new(&program);
    cmd.args(&args);
    if is_powershell(&program) && !args.iter().any(|a| runs_something(a)) {
        cmd.args(["-NoExit", "-EncodedCommand", &powershell_hook()]);
    }
    cmd
}

#[cfg(windows)]
fn default_shell() -> (String, Vec<String>) {
    let no_logo = vec!["-NoLogo".to_string()];
    if let Some(pwsh) = find_on_path("pwsh.exe").or_else(|| {
        let program_files = std::env::var("ProgramFiles").ok()?;
        let path = std::path::Path::new(&program_files).join(r"PowerShell\7\pwsh.exe");
        path.is_file().then_some(path)
    }) {
        return (pwsh.display().to_string(), no_logo);
    }
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let powershell =
        std::path::Path::new(&root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    if powershell.is_file() {
        return (powershell.display().to_string(), no_logo);
    }
    (std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".into()), Vec::new())
}

#[cfg(windows)]
fn is_powershell(program: &str) -> bool {
    let file = program.rsplit(['\\', '/']).next().unwrap_or(program).to_ascii_lowercase();
    matches!(file.as_str(), "pwsh" | "pwsh.exe" | "powershell" | "powershell.exe")
}

/// Whether a PowerShell argument already gives it something to run — a
/// command, a script or an encoded command — in any of the abbreviations
/// PowerShell accepts. Such a shell is left as it was asked for.
#[cfg(windows)]
fn runs_something(arg: &str) -> bool {
    let Some(name) = arg.strip_prefix('-').or_else(|| arg.strip_prefix('/')) else { return false };
    let name = name.to_ascii_lowercase();
    name == "c"
        || name == "f"
        || name == "e"
        || name == "ec"
        || name.starts_with("com")
        || name.starts_with("fil")
        || name.starts_with("enc")
}

/// What PowerShell runs before its first prompt: the prompt it already has,
/// wrapped so that every prompt also says where the shell is.
///
/// PowerShell keeps its location to itself — `Set-Location` changes it
/// without changing the process's working directory, which is the one thing
/// the daemon can read from outside. So each prompt sets the process's
/// directory to the shell's, and announces it the way Windows Terminal
/// listens for it (OSC 9;9), which the grid keeps as well. The person's own
/// prompt, from their profile or a prompt theme, is called as before.
#[cfg(windows)]
fn powershell_hook() -> String {
    const SCRIPT: &str = r#"$global:__KeepPrompt = $function:prompt
function global:prompt {
  $l = $executionContext.SessionState.Path.CurrentLocation
  $s = ''
  if ($l.Provider.Name -eq 'FileSystem') {
    try { [Environment]::CurrentDirectory = $l.ProviderPath } catch {}
    $s = [char]27 + ']9;9;"' + $l.ProviderPath + '"' + [char]27 + '\'
  }
  if ($global:__KeepPrompt) { $s + (& $global:__KeepPrompt) } else { $s + "PS $l$('>' * ($nestedPromptLevel + 1)) " }
}
"#;
    let utf16: Vec<u8> = SCRIPT.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    base64(&utf16)
}

#[cfg(windows)]
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(all(test, windows))]
mod shell_tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn a_powershell_already_running_something_is_left_alone() {
        assert!(runs_something("-Command"));
        assert!(runs_something("-c"));
        assert!(runs_something("-File"));
        assert!(runs_something("-EncodedCommand"));
        assert!(runs_something("-enc"));
        assert!(runs_something("/Command"));
        assert!(!runs_something("-NoLogo"));
        assert!(!runs_something("-NoProfile"));
        assert!(!runs_something("-ExecutionPolicy"));
        assert!(!runs_something("-ep"));
        assert!(is_powershell(r"C:\Program Files\PowerShell\7\pwsh.exe"));
        assert!(is_powershell("powershell.exe"));
        assert!(!is_powershell("cmd.exe"));
    }
}

/// Where the app keeps the shell chosen in it: `%APPDATA%\Keep\shell.txt`.
#[cfg(windows)]
pub fn shell_setting_path() -> Option<std::path::PathBuf> {
    let appdata = std::env::var("APPDATA").ok()?;
    Some(std::path::Path::new(&appdata).join("Keep").join("shell.txt"))
}

#[cfg(windows)]
fn find_on_path(exe: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join(exe)).find(|p| p.is_file())
}

/// The terminal to call ourselves, if the system can look it up.
///
/// Checked rather than assumed: ghostty's description is installed with
/// ghostty, and on a machine without it every program that consults terminfo
/// would fail to find the terminal it was just told it is in.
#[cfg(unix)]
fn term_name() -> &'static str {
    const GHOSTTY: &str = "xterm-ghostty";
    static RESOLVED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let installed = *RESOLVED.get_or_init(|| {
        let mut roots: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(dir) = std::env::var("TERMINFO") {
            roots.push(dir.into());
        }
        if let Ok(home) = std::env::var("HOME") {
            roots.push(std::path::Path::new(&home).join(".terminfo"));
        }
        for dir in [
            "/usr/share/terminfo",
            "/opt/homebrew/share/terminfo",
            "/usr/local/share/terminfo",
        ] {
            roots.push(dir.into());
        }
        // ncurses files a description under the first letter of its name, as
        // the letter itself or as that letter's hex — both are in the wild.
        roots.iter().any(|root| {
            root.join("x").join(GHOSTTY).exists() || root.join("78").join(GHOSTTY).exists()
        })
    });
    if installed { GHOSTTY } else { "xterm-256color" }
}


impl Workspace {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), tabs: Mutex::new(Vec::new()), next_id: AtomicU32::new(1) }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Start a tab running the user's shell.
    ///
    /// `split_of` other than 0 makes it a pane of that tab; the target must
    /// exist, or a stale client could quietly create an orphan layout.
    pub fn new_tab(
        &self,
        cwd: Option<&str>,
        cols: u16,
        rows: u16,
        split_of: u32,
        split_dir: u8,
    ) -> Result<(u32, Arc<Tab>)> {
        let mut cmd = shell_command();
        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }
        // A tab nobody gave a directory starts at home. On unix the shell
        // goes there by itself; a Windows process starts wherever its parent
        // stands, and the daemon's directory means nothing to anyone.
        #[cfg(windows)]
        if cwd.is_none() {
            if let Ok(home) = std::env::var("USERPROFILE") {
                cmd.cwd(home);
            }
        }
        // Programs expect these; without TERM many refuse to draw at all.
        //
        // What is drawing is ghostty, so that is what a program is told it is
        // talking to. Saying `xterm-256color` was describing a different
        // terminal: a program looks its keys and capabilities up under this
        // name, and one told it is in a plain xterm asks for, and handles,
        // a plain xterm's keyboard. The name is only worth giving if the
        // description behind it is installed, which is not something a daemon
        // may assume — a terminal nobody can look up is worse than a modest
        // one that everybody can.
        #[cfg(unix)]
        cmd.env("TERM", term_name());
        // What draws a tab on Windows is the app's own terminal, xterm.js,
        // and no program there looks a terminal up by name: the name that
        // says the most to the ones that read it at all is xterm's.
        #[cfg(windows)]
        cmd.env("TERM", "xterm-256color");
        // Said outright rather than inherited. What renders here is ghostty,
        // and it renders in twenty-four bit colour — but a daemon is started
        // once and may be started from anywhere, including somewhere with no
        // terminal at all, so nothing about the terminal can be picked up
        // from the environment it happened to be launched in. Left to
        // inheritance, these are right until the day the daemon is started
        // from a login script, and then every colour in every shell is wrong.
        cmd.env("COLORTERM", "truecolor");
        #[cfg(unix)]
        cmd.env("TERM_PROGRAM", "ghostty");
        #[cfg(windows)]
        {
            cmd.env("TERM_PROGRAM", "keep");
            cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
        }
        cmd.env("KEEP_WORKSPACE", &self.name);

        // Whatever started this daemon does not get to introduce itself to
        // every shell it opens.
        //
        // A daemon outlives the shell that launched it — that is the whole
        // point of it — and it inherits that shell's environment whole,
        // session markers and credentials included. Anything downstream then
        // believes it is a child of a session that may have ended hours ago.
        for (key, _) in std::env::vars() {
            if key == "CLAUDECODE" || key.starts_with("CLAUDE_CODE_") {
                cmd.env_remove(&key);
            }
        }

        let tab = Arc::new(Tab::spawn(cmd, cols, rows)?);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        let mut guard = self.tabs.lock().map_err(|_| anyhow!("workspace poisoned"))?;
        if split_of != 0 && !guard.iter().any(|e| e.id == split_of) {
            return Err(anyhow!("no tab {split_of} to split"));
        }
        guard.push(Entry { id, tab: Arc::clone(&tab), split_of, split_dir });
        Ok((id, tab))
    }

    /// Take a tab and its whole pane tree out of this workspace.
    ///
    /// The entries leave exactly as they were — ids, layout, the running
    /// shells behind them — and it is the receiving workspace that renames
    /// them, because ids are per-workspace counters and only the receiver
    /// knows which numbers it has not used.
    pub fn take_tree(&self, root: u32) -> Result<Vec<(u32, Arc<Tab>, u32, u8)>> {
        let mut guard = self.tabs.lock().map_err(|_| anyhow!("workspace poisoned"))?;
        if !guard.iter().any(|e| e.id == root) {
            return Err(anyhow!("no tab {root}"));
        }
        // The subtree: the root, its panes, their panes. Ids are members once
        // found, so a chain of splits follows its parents out.
        let mut member = vec![root];
        loop {
            let more: Vec<u32> = guard
                .iter()
                .filter(|e| member.contains(&e.split_of) && !member.contains(&e.id))
                .map(|e| e.id)
                .collect();
            if more.is_empty() {
                break;
            }
            member.extend(more);
        }
        let mut taken = Vec::new();
        guard.retain_mut(|e| {
            if member.contains(&e.id) {
                taken.push((e.id, Arc::clone(&e.tab), e.split_of, e.split_dir));
                false
            } else {
                true
            }
        });
        Ok(taken)
    }

    /// Adopt a tree taken from another workspace. Every tab gets an id of this
    /// workspace's own, the pane links are rewritten to match, and the new
    /// root id comes back so the caller can say where the tab went.
    pub fn adopt_tree(&self, tree: Vec<(u32, Arc<Tab>, u32, u8)>) -> Result<u32> {
        let mut renamed: Vec<(u32, u32)> = Vec::new();
        for (old, _, _, _) in &tree {
            renamed.push((*old, self.next_id.fetch_add(1, Ordering::Relaxed)));
        }
        let rename = |old: u32| renamed.iter().find(|(o, _)| *o == old).map(|(_, n)| *n);
        let root = renamed.first().map(|(_, n)| *n).ok_or_else(|| anyhow!("empty tree"))?;
        let mut guard = self.tabs.lock().map_err(|_| anyhow!("workspace poisoned"))?;
        for (old, tab, split_of, split_dir) in tree {
            guard.push(Entry {
                id: rename(old).unwrap_or(root),
                tab,
                // The root's parent is outside the tree — it has none — and
                // stays 0. A pane's parent came along, renamed.
                split_of: rename(split_of).unwrap_or(0),
                split_dir,
            });
        }
        Ok(root)
    }

    pub fn tab(&self, id: u32) -> Option<Arc<Tab>> {
        let guard = self.tabs.lock().ok()?;
        guard.iter().find(|e| e.id == id).map(|e| Arc::clone(&e.tab))
    }

    /// The tab a client lands on when it names a workspace but not a tab.
    pub fn first_or_create(&self, cols: u16, rows: u16) -> Result<(u32, Arc<Tab>)> {
        {
            let guard = self.tabs.lock().map_err(|_| anyhow!("workspace poisoned"))?;
            if let Some(entry) = guard.iter().find(|e| !e.tab.is_finished()) {
                return Ok((entry.id, Arc::clone(&entry.tab)));
            }
        }
        self.new_tab(None, cols, rows, 0, 0)
    }

    pub fn tabs(&self) -> Vec<TabRef> {
        let guard = match self.tabs.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        let live: std::collections::HashSet<u32> = guard.iter().map(|e| e.id).collect();
        guard
            .iter()
            .map(|e| TabRef {
                id: e.id,
                // A pane whose base tab is gone is promoted to standalone
                // rather than reported dangling: the layout must always be
                // reconstructible from what this returns.
                split_of: if live.contains(&e.split_of) { e.split_of } else { 0 },
                split_dir: e.split_dir,
                tab: Arc::clone(&e.tab),
            })
            .collect()
    }

    /// Put panes somewhere else in the arrangement.
    ///
    /// All of them or none: dropping a pane to the left of another is two
    /// moves — the newcomer takes the other's place and the other becomes its
    /// pane — and a layout that took only the first of those is one nobody
    /// asked for. So the whole set is staged, checked, and only then kept.
    ///
    /// What is checked is that every pane named exists, and that no pane ends
    /// up its own ancestor. A loop is not a layout: reading it back would
    /// never terminate.
    pub fn rearrange(&self, moves: &[(u32, u32, u8)]) -> Result<()> {
        let mut guard = self.tabs.lock().map_err(|_| anyhow!("workspace poisoned"))?;

        // Stage on the bare shape, so nothing is touched until it all holds.
        let mut layout: Vec<(u32, u32, u8)> =
            guard.iter().map(|e| (e.id, e.split_of, e.split_dir)).collect();
        for &(tab, parent, dir) in moves {
            if tab == parent {
                return Err(anyhow!("a pane cannot be a pane of itself"));
            }
            if parent != 0 && !layout.iter().any(|e| e.0 == parent) {
                return Err(anyhow!("no tab {parent} to hang {tab} off"));
            }
            let at = layout
                .iter()
                .position(|e| e.0 == tab)
                .ok_or_else(|| anyhow!("no tab {tab} to move"))?;
            layout[at].1 = parent;
            layout[at].2 = dir;
        }

        let parent_of = |id: u32, layout: &[(u32, u32, u8)]| -> u32 {
            layout.iter().find(|e| e.0 == id).map(|e| e.1).unwrap_or(0)
        };
        for &(id, _, _) in &layout {
            let mut current = id;
            for _ in 0..=layout.len() {
                current = parent_of(current, &layout);
                if current == 0 {
                    break;
                }
                if current == id {
                    return Err(anyhow!("that would make a pane its own pane"));
                }
            }
        }

        for entry in guard.iter_mut() {
            if let Some(&(_, parent, dir)) = layout.iter().find(|e| e.0 == entry.id) {
                entry.split_of = parent;
                entry.split_dir = dir;
            }
        }

        // A client rebuilds the arrangement by replaying this list in order,
        // splitting each pane off one that is already there — so a pane must
        // never come before the one it hangs off. Creation order guaranteed
        // that for free; moving panes does not.
        let mut ordered: Vec<Entry> = Vec::with_capacity(guard.len());
        let mut placed: Vec<u32> = Vec::with_capacity(guard.len());
        let mut rest: Vec<Entry> = guard.drain(..).collect();
        while !rest.is_empty() {
            let next = rest
                .iter()
                .position(|e| e.split_of == 0 || placed.contains(&e.split_of));
            // Nothing placeable means a cycle got past the check above; keep
            // the tabs rather than drop them, and let the client promote what
            // it cannot place.
            let at = next.unwrap_or(0);
            let entry = rest.remove(at);
            placed.push(entry.id);
            ordered.push(entry);
        }
        *guard = ordered;
        Ok(())
    }

    pub fn close_tab(&self, id: u32) -> Result<()> {
        let tab = {
            let mut guard = self.tabs.lock().map_err(|_| anyhow!("workspace poisoned"))?;
            let pos = guard.iter().position(|e| e.id == id).ok_or_else(|| anyhow!("no tab {id}"))?;
            let closed = guard.remove(pos);
            rehome(&mut guard, &HashMap::from([(closed.id, (closed.split_of, closed.split_dir))]));
            closed.tab
        };
        tab.kill()
    }

    pub fn kill_all(&self) -> Result<()> {
        let tabs: Vec<Arc<Tab>> = {
            let mut guard = self.tabs.lock().map_err(|_| anyhow!("workspace poisoned"))?;
            guard.drain(..).map(|e| e.tab).collect()
        };
        for tab in tabs {
            tab.kill().ok();
        }
        Ok(())
    }

    /// Drop tabs whose child exited and that nobody is watching.
    ///
    /// A finished tab someone is still attached to stays: they may not have
    /// read the last of its output yet.
    pub fn reap(&self) {
        if let Ok(mut guard) = self.tabs.lock() {
            let departed: HashMap<u32, (u32, u8)> = guard
                .iter()
                .filter(|e| e.tab.is_finished() && e.tab.attached_clients() == 0)
                .map(|e| (e.id, (e.split_of, e.split_dir)))
                .collect();
            if departed.is_empty() {
                return;
            }
            guard.retain(|e| !departed.contains_key(&e.id));
            // The same inheritance as `close_tab`, resolved across the whole
            // group: panes whose own shell exited must not take the panes
            // below them out of the arrangement.
            rehome(&mut guard, &departed);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.lock().map(|g| g.is_empty()).unwrap_or(true)
    }
}
