//! keep — persistent terminal sessions.

mod attach;
mod picker;

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use keep_proto::{ClientMsg, ServerMsg, SessionInfo, TAB_ANY};

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
            if let Some((name, tab)) = embedded_target() {
                return enter(&socket, &name, tab);
            }

            let sessions = list(&socket)?;
            match picker::pick(&sessions)? {
                picker::Choice::Session(name) | picker::Choice::New(name) => {
                    enter(&socket, &name, TAB_ANY)
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
            let name = args.get(1).context("usage: keep new <session>")?;
            ensure_daemon(&socket)?;
            let mut sock = UnixStream::connect(&socket)?;
            ClientMsg::NewTab { session: name.clone(), cwd: None, cols: 80, rows: 24 }
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
            let mut sock = UnixStream::connect(&socket)?;
            ClientMsg::Kill { session: name.clone() }.write(&mut sock)?;
            match ServerMsg::read(&mut sock)? {
                Some(ServerMsg::Ok) => {
                    println!("killed {name}");
                    Ok(())
                }
                Some(ServerMsg::Error(e)) => anyhow::bail!(e),
                _ => anyhow::bail!("unexpected reply"),
            }
        }
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
            enter(&socket, name, tab)
        }
    }
}

fn enter(socket: &Path, name: &str, tab: u32) -> Result<()> {
    match attach::attach(socket, name, tab)? {
        attach::Outcome::Detached => {
            println!("detached from {name}");
        }
        attach::Outcome::SessionEnded => {
            println!("session {name} ended");
        }
    }
    Ok(())
}

fn list(socket: &Path) -> Result<Vec<SessionInfo>> {
    let mut sock = UnixStream::connect(socket).context("connect to daemon")?;
    ClientMsg::List.write(&mut sock)?;
    match ServerMsg::read(&mut sock)? {
        Some(ServerMsg::Sessions(list)) => Ok(list),
        Some(ServerMsg::Error(e)) => anyhow::bail!(e),
        _ => anyhow::bail!("unexpected reply from daemon"),
    }
}

fn print_list(sessions: &[SessionInfo]) {
    if sessions.is_empty() {
        println!("no sessions");
        return;
    }
    for s in sessions {
        let live = s.tabs.iter().filter(|t| !t.finished).count();
        let state = if s.clients() > 0 { "attached" } else { "idle" };
        println!("{:<24} {:>2} tab(s)  {}", s.name, live, state);
        for t in &s.tabs {
            let mark = if t.finished { "exited" } else if t.clients > 0 { "attached" } else { "idle" };
            let label = if t.title.is_empty() {
                format!("tab {}", t.id)
            } else {
                format!("tab {} · {}", t.id, t.title)
            };
            println!("  {:<30} {:>4}x{:<4} {}", label, t.cols, t.rows, mark);
        }
    }
}

/// Start the daemon if it is not already listening.
///
/// The daemon is deliberately not a child of this process: it must outlive
/// every client, including the one that happened to start it.
fn ensure_daemon(socket: &Path) -> Result<()> {
    if UnixStream::connect(socket).is_ok() {
        return Ok(());
    }

    let exe = daemon_binary()?;
    std::process::Command::new(&exe)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("start daemon at {}", exe.display()))?;

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if UnixStream::connect(socket).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    anyhow::bail!("daemon did not start listening on {}", socket.display())
}

/// The name of the file an embedder drops in the working directory.
const TARGET_FILE: &str = ".keep-attach";

/// Read the session and tab an embedder wants, if it left them for us.
///
/// The macOS app cannot pass arguments: libghostty takes its spawn command
/// once per application, and the per-surface `command`, `env_vars` and
/// `initial_input` fields are all ignored by the runtime we link against.
/// `working_directory` is the one per-surface field that survives, so the app
/// gives each surface a private directory holding this file.
///
/// The file is consumed on read: a session that later runs `cd` into the same
/// directory must not be treated as a fresh attach request.
fn embedded_target() -> Option<(String, u32)> {
    embedded_target_in(&std::env::current_dir().ok()?)
}

fn embedded_target_in(dir: &Path) -> Option<(String, u32)> {
    let path = dir.join(TARGET_FILE);
    let body = std::fs::read_to_string(&path).ok()?;
    std::fs::remove_file(&path).ok();

    let mut lines = body.lines();
    let name = lines.next()?.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let tab = lines.next().and_then(|v| v.trim().parse().ok()).unwrap_or(TAB_ANY);
    Some((name, tab))
}

/// Look for `keepd` next to this binary first, so a build tree and an
/// installed copy never mix versions.
fn daemon_binary() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("KEEPD_BIN") {
        return Ok(p.into());
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("keepd");
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
keep — persistent terminal sessions

USAGE
  keep                 pick a session (or type a name to start one)
  keep <name>          attach to <name>, creating it if needed
  keep <name> --tab N  attach to a specific tab
  keep new <name>      open a tab in <name>, without attaching
  keep ls              list sessions and their tabs
  keep kill <name>     end a session

IN A SESSION
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
        assert_eq!(embedded_target_in(&dir), Some(("orion".into(), 7)));
    }

    #[test]
    fn a_missing_tab_means_any_tab() {
        let dir = scratch("notab");
        std::fs::write(dir.join(TARGET_FILE), "orion\n").unwrap();
        assert_eq!(embedded_target_in(&dir), Some(("orion".into(), TAB_ANY)));
    }

    /// Reading consumes the file. Calling twice must not resurrect the target,
    /// and must not panic: an earlier version read it once to test and once to
    /// use, so the second read found nothing.
    #[test]
    fn the_target_is_consumed_by_reading_it() {
        let dir = scratch("once");
        std::fs::write(dir.join(TARGET_FILE), "orion\n1\n").unwrap();

        assert!(embedded_target_in(&dir).is_some(), "first read should find it");
        assert_eq!(embedded_target_in(&dir), None, "second read should find nothing");
        assert!(!dir.join(TARGET_FILE).exists(), "file should be gone");
    }

    #[test]
    fn no_file_is_not_an_error() {
        let dir = scratch("empty");
        std::fs::remove_file(dir.join(TARGET_FILE)).ok();
        assert_eq!(embedded_target_in(&dir), None);
    }
}
