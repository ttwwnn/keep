//! keep — persistent terminal workspaces.

mod attach;
#[cfg(windows)]
mod console;
mod picker;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use keep_proto::net::Stream;
use keep_proto::{ClientMsg, ServerMsg, TAB_ANY, WorkspaceInfo};

fn main() {
    if let Err(e) = run() {
        eprintln!("keep: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let socket = keep_proto::socket_path();

    match args.first().map(String::as_str) {
        None => {
            ensure_daemon(&socket)?;

            // An embedder (the macOS app) launches this with no arguments and
            // leaves the target in the working directory. Read it exactly
            // once: the read consumes the file.
            if let Some(target) = embedded_target() {
                return enter(&socket, &target.workspace, target.tab, target.watching);
            }

            let workspaces = list(&socket)?;
            match picker::pick(&workspaces)? {
                picker::Choice::Workspace(name) | picker::Choice::New(name) => {
                    enter(&socket, &name, TAB_ANY, None)
                }
                picker::Choice::Cancelled => Ok(()),
            }
        }
        Some("ls" | "list") => {
            ensure_daemon(&socket)?;
            print_list(&list(&socket)?);
            Ok(())
        }
        Some("new") => {
            let name = args.get(1).context("usage: keep new <workspace>")?;
            ensure_daemon(&socket)?;
            let mut sock = Stream::connect(&socket)?;
            ClientMsg::NewTab {
                workspace: name.clone(),
                // Where it is run from. `keep <name>` has always opened a
                // workspace in the directory you were standing in; `keep new`
                // put its tab in the home directory instead, which is not a
                // second way of saying the same thing — it is the one thing
                // nobody wants after typing the command inside a project.
                cwd: std::env::current_dir().ok().map(|p| p.display().to_string()),
                cols: 80,
                rows: 24,
                split_of: TAB_ANY,
                split_dir: keep_proto::SPLIT_NONE,
            }
            .write(&mut sock)?;
            match ServerMsg::read(&mut sock)? {
                Some(ServerMsg::TabCreated { tab }) => {
                    println!("{name}: opened tab {tab}");
                    Ok(())
                }
                Some(ServerMsg::Error(e)) => anyhow::bail!(e),
                _ => anyhow::bail!("unexpected reply"),
            }
        }
        Some("kill") => {
            let name = args.get(1).context("usage: keep kill <name>")?;
            ensure_daemon(&socket)?;
            let mut sock = Stream::connect(&socket)?;
            ClientMsg::Kill { workspace: name.clone() }.write(&mut sock)?;
            match ServerMsg::read(&mut sock)? {
                Some(ServerMsg::Ok) => {
                    println!("killed {name}");
                    Ok(())
                }
                Some(ServerMsg::Error(e)) => anyhow::bail!(e),
                _ => anyhow::bail!("unexpected reply"),
            }
        }
        // The AI accounts, usage and per-tab AI, and the worktrees a tab's
        // conversation left: see docs/ia.md. Answered in JSON, for the apps.
        Some("ia") => std::process::exit(keep_ia::cli::ia(&args[1..])),
        Some("worktrees") => std::process::exit(keep_ia::cli::worktrees(&args[1..])),
        Some("-h" | "--help" | "help") => {
            print_help();
            Ok(())
        }
        Some("--version" | "-V") => {
            println!("keep {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some(name) => {
            ensure_daemon(&socket)?;
            // `keep <session> --tab N` targets one tab; without it the daemon
            // picks the session's first live tab.
            let tab = match args.iter().position(|a| a == "--tab") {
                Some(i) => args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(TAB_ANY),
                None => TAB_ANY,
            };
            enter(&socket, name, tab, None)
        }
    }
}

fn enter(
    socket: &Path,
    name: &str,
    tab: u32,
    watching: Option<std::path::PathBuf>,
) -> Result<()> {
    match attach::attach(socket, name, tab, watching)? {
        attach::Outcome::Detached => {
            println!("detached from {name}");
        }
        attach::Outcome::Ended => {
            println!("{name} ended");
        }
    }
    Ok(())
}

/// The workspace list, asking for the fuller answer first.
///
/// A daemon that predates `cwd` and `last_active` refuses the tag and closes,
/// which arrives here as a read error rather than as a reply — so the retry
/// needs a fresh connection, not another write on this one.
fn list(socket: &Path) -> Result<Vec<WorkspaceInfo>> {
    if let Ok(list) = ask(socket, ClientMsg::List2) {
        return Ok(list);
    }
    ask(socket, ClientMsg::List)
}

fn ask(socket: &Path, question: ClientMsg) -> Result<Vec<WorkspaceInfo>> {
    let mut sock = Stream::connect(socket).context("connect to daemon")?;
    question.write(&mut sock)?;
    match ServerMsg::read(&mut sock)? {
        Some(ServerMsg::Workspaces(list) | ServerMsg::Workspaces2(list)) => Ok(list),
        Some(ServerMsg::Error(e)) => anyhow::bail!(e),
        _ => anyhow::bail!("unexpected reply from daemon"),
    }
}

fn print_list(workspaces: &[WorkspaceInfo]) {
    if workspaces.is_empty() {
        println!("no workspaces");
        return;
    }
    for s in workspaces {
        let live = s.tabs.iter().filter(|t| !t.finished).count();
        let state = if s.busy() {
            "running"
        } else if s.clients() > 0 {
            "attached"
        } else {
            "idle"
        };
        println!("{:<24} {:>2} tab(s)  {}", s.name, live, state);
        for t in &s.tabs {
            // How many are watching, once that can be more than one: two
            // windows on one tab is a thing you do on purpose, and a thing
            // you can forget you did.
            let mark = if t.finished {
                "exited".to_string()
            } else if t.busy {
                "running".to_string()
            } else if t.clients > 1 {
                format!("attached \u{d7}{}", t.clients)
            } else if t.clients == 1 {
                "attached".to_string()
            } else {
                "idle".to_string()
            };
            let mut label = if t.title.is_empty() {
                format!("tab {}", t.id)
            } else {
                format!("tab {} · {}", t.id, t.title)
            };
            if t.split_of != TAB_ANY {
                label = format!("{label} (pane of {})", t.split_of);
            }
            println!("  {:<30} {:>4}x{:<4} {}", label, t.cols, t.rows, mark);
        }
    }
}

/// Start the daemon if it is not already listening.
///
/// The daemon is deliberately not a child of this process: it must outlive
/// every client, including the one that happened to start it.
fn ensure_daemon(socket: &Path) -> Result<()> {
    if Stream::connect(socket).is_ok() {
        return Ok(());
    }

    let exe = daemon_binary()?;
    spawn_daemon(&exe).with_context(|| format!("start daemon at {}", exe.display()))?;

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if Stream::connect(socket).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    anyhow::bail!("daemon did not start listening on {}", socket.display())
}

fn daemon_command(exe: &Path) -> std::process::Command {
    let mut cmd = std::process::Command::new(exe);
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    cmd
}

#[cfg(unix)]
fn spawn_daemon(exe: &Path) -> std::io::Result<()> {
    daemon_command(exe).spawn().map(|_| ())
}

/// On Windows a child is tied to its parent's console and, often, to a job
/// that ends with whatever started it — a terminal closing its window, an
/// editor quitting. The daemon is detached from both, so it outlives this
/// client the way it does on unix.
#[cfg(windows)]
fn spawn_daemon(exe: &Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    match daemon_command(exe).creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB).spawn() {
        Ok(_) => Ok(()),
        // A job that does not allow breaking away refuses the flag; inside
        // one the daemon cannot outlive it anyway, so start it there.
        Err(_) => daemon_command(exe).creation_flags(flags).spawn().map(|_| ()),
    }
}

/// The name of the file an embedder drops in the working directory.
const TARGET_FILE: &str = ".keep-attach";

/// Read the workspace and tab an embedder wants, if it left them for us.
///
/// The macOS app cannot pass arguments: libghostty takes its spawn command
/// once per application, and the per-surface `command`, `env_vars` and
/// `initial_input` fields are all ignored by the runtime we link against.
/// `working_directory` is the one per-surface field that survives, so the app
/// gives each surface a private directory holding this file.
///
/// The file is consumed on read: a session that later runs `cd` into the same
/// directory must not be treated as a fresh attach request.
///
/// A third line, if present, names a file the embedder keeps updated with
/// whether its surface is on screen. That one is *not* consumed — it is read
/// for as long as this client runs.
struct Target {
    workspace: String,
    tab: u32,
    watching: Option<std::path::PathBuf>,
}

fn embedded_target() -> Option<Target> {
    embedded_target_in(&std::env::current_dir().ok()?)
}

fn embedded_target_in(dir: &Path) -> Option<Target> {
    let path = dir.join(TARGET_FILE);
    let body = std::fs::read_to_string(&path).ok()?;
    std::fs::remove_file(&path).ok();

    let mut lines = body.lines();
    let workspace = lines.next()?.trim().to_string();
    if workspace.is_empty() {
        return None;
    }
    let tab = lines.next().and_then(|v| v.trim().parse().ok()).unwrap_or(TAB_ANY);
    let watching = lines
        .next()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from);
    Some(Target { workspace, tab, watching })
}

/// Look for `keepd` next to this binary first, so a build tree and an
/// installed copy never mix versions.
fn daemon_binary() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("KEEPD_BIN") {
        return Ok(p.into());
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join(format!("keepd{}", std::env::consts::EXE_SUFFIX));
            if sibling.is_file() {
                return Ok(sibling);
            }
        }
    }
    Ok(PathBuf::from("keepd"))
}

fn print_help() {
    let mut out = std::io::stdout();
    let _ = write!(
        out,
        "\
keep — persistent terminal workspaces

USAGE
  keep                 pick a workspace (or type a name to start one)
  keep <name>          attach to <name>, creating it if needed
  keep <name> --tab N  attach to a specific tab
  keep new <name>      open a tab in <name>, without attaching
  keep ls              list workspaces and their tabs
  keep kill <name>     end a workspace

IN A TAB
  ctrl-\\               detach, leaving everything running
"
    );
}

#[cfg(test)]
mod embedded {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("keep-target-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reads_session_and_tab() {
        let dir = scratch("pair");
        std::fs::write(dir.join(TARGET_FILE), "orion\n7\n").unwrap();
        let target = embedded_target_in(&dir).expect("target");
        assert_eq!((target.workspace.as_str(), target.tab), ("orion", 7));
        assert!(target.watching.is_none(), "no third line means nobody is watching");
    }

    #[test]
    fn a_missing_tab_means_any_tab() {
        let dir = scratch("notab");
        std::fs::write(dir.join(TARGET_FILE), "orion\n").unwrap();
        let target = embedded_target_in(&dir).expect("target");
        assert_eq!((target.workspace.as_str(), target.tab), ("orion", TAB_ANY));
    }

    /// A third line names a file that says whether the surface is on screen.
    /// It is a path, not a state: the state changes while this client runs.
    #[test]
    fn a_third_line_names_the_file_that_says_if_we_are_seen() {
        let dir = scratch("watch");
        let flag = dir.join("showing");
        std::fs::write(dir.join(TARGET_FILE), format!("orion\n7\n{}\n", flag.display()))
            .unwrap();
        let target = embedded_target_in(&dir).expect("target");
        assert_eq!(target.watching.as_deref(), Some(flag.as_path()));
    }

    /// Reading consumes the file. Calling twice must not resurrect the target,
    /// and must not panic: an earlier version read it once to test and once to
    /// use, so the second read found nothing.
    #[test]
    fn the_target_is_consumed_by_reading_it() {
        let dir = scratch("once");
        std::fs::write(dir.join(TARGET_FILE), "orion\n1\n").unwrap();

        assert!(embedded_target_in(&dir).is_some(), "first read should find it");
        assert!(embedded_target_in(&dir).is_none(), "second read should find nothing");
        assert!(!dir.join(TARGET_FILE).exists(), "file should be gone");
    }

    #[test]
    fn no_file_is_not_an_error() {
        let dir = scratch("empty");
        std::fs::remove_file(dir.join(TARGET_FILE)).ok();
        assert!(embedded_target_in(&dir).is_none());
    }
}
