//! The terminal client, run the way a person runs it: inside a terminal.
//!
//! The terminal is a PTY of the test's own — ConPTY on Windows — and what the
//! client draws into it is read back through libghostty-vt, the same grid the
//! daemon keeps. So this goes through everything a person's keystroke does:
//! the client's raw mode (a console's VT mode on Windows), the socket or pipe,
//! the daemon, the tab's shell, and back.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use keep_vt::Terminal;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

fn keep_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_keep"))
}

/// The daemon built beside the client.
fn keepd_bin() -> PathBuf {
    keep_bin().with_file_name(format!("keepd{}", std::env::consts::EXE_SUFFIX))
}

/// An address of the test's own: nothing a person has open is touched.
fn address() -> String {
    #[cfg(unix)]
    return format!("/tmp/keep-cli-test-{}.sock", std::process::id());
    #[cfg(windows)]
    return format!(r"\\.\pipe\keep-cli-test-{}", std::process::id());
}

fn with_test_env(cmd: &mut CommandBuilder, socket: &str) {
    cmd.env("KEEP_SOCKET", socket);
    cmd.env("KEEPD_BIN", keepd_bin().display().to_string());
    #[cfg(unix)]
    cmd.env("SHELL", "/bin/sh");
    #[cfg(windows)]
    cmd.env("KEEP_SHELL", "powershell.exe -NoLogo -NoProfile");
}

/// The command that proves the shell ran, and what it prints.
fn marker() -> (&'static [u8], &'static str) {
    #[cfg(unix)]
    return (b"echo cli-$((6*7))\r", "cli-42");
    #[cfg(windows)]
    return (b"Write-Output ('cli-' + (6*7))\r", "cli-42");
}

fn shows_prompt(screen: &str) -> bool {
    #[cfg(unix)]
    return screen.contains('$') || screen.contains('#');
    #[cfg(windows)]
    return screen.contains("PS ") && screen.contains('>');
}

struct Screen {
    grid: Arc<Mutex<Terminal>>,
}

impl Screen {
    fn text(&self) -> String {
        self.grid.lock().ok().and_then(|g| g.text().ok()).unwrap_or_default()
    }

    fn wait_for(&self, timeout: Duration, cond: impl Fn(&str) -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if cond(&self.text()) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }
}

#[test]
fn the_client_attaches_runs_a_command_and_detaches() {
    let socket = address();

    // The daemon is started here rather than by the client, so the test owns
    // it and ends it.
    let mut daemon = std::process::Command::new(keepd_bin())
        .env("KEEP_SOCKET", &socket)
        .env(if cfg!(windows) { "KEEP_SHELL" } else { "SHELL" }, if cfg!(windows) {
            "powershell.exe -NoLogo -NoProfile"
        } else {
            "/bin/sh"
        })
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("start keepd");

    let pty = native_pty_system()
        .openpty(PtySize { rows: 30, cols: 100, pixel_width: 0, pixel_height: 0 })
        .expect("openpty");
    let mut cmd = CommandBuilder::new(keep_bin());
    cmd.arg("cli-test");
    with_test_env(&mut cmd, &socket);
    let mut client = pty.slave.spawn_command(cmd).expect("spawn keep");
    drop(pty.slave);

    let mut writer = pty.master.take_writer().expect("writer");
    let mut reader = pty.master.try_clone_reader().expect("reader");
    let grid = Arc::new(Mutex::new(Terminal::new(100, 30).expect("grid")));
    // The grid answers what the client's terminal is asked — ConPTY asks
    // where the cursor is before it shows anything — the way a terminal would.
    grid.lock().unwrap().answer_queries().expect("answers");
    let replies: Arc<Mutex<Vec<u8>>> = Arc::default();
    {
        let grid = Arc::clone(&grid);
        let replies = Arc::clone(&replies);
        std::thread::spawn(move || {
            let mut buf = [0u8; 16 * 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let mut g = grid.lock().unwrap();
                        g.write(&buf[..n]);
                        replies.lock().unwrap().extend(g.take_replies());
                    }
                }
            }
        });
    }
    let screen = Screen { grid };
    let send = |writer: &mut Box<dyn Write + Send>, bytes: &[u8]| {
        writer.write_all(bytes).unwrap();
        writer.flush().unwrap();
    };
    // Hand the answers back as they are made.
    let answer = |writer: &mut Box<dyn Write + Send>| {
        let pending = std::mem::take(&mut *replies.lock().unwrap());
        if !pending.is_empty() {
            send(writer, &pending);
        }
    };

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut prompt = false;
    while Instant::now() < deadline {
        answer(&mut writer);
        if shows_prompt(&screen.text()) {
            prompt = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(prompt, "the attached shell never showed a prompt: {:?}", screen.text());

    let (command, expected) = marker();
    send(&mut writer, command);
    let ran = {
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut seen = false;
        while Instant::now() < deadline {
            answer(&mut writer);
            if screen.text().contains(expected) {
                seen = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        seen
    };
    assert!(ran, "the command never ran through the client: {:?}", screen.text());

    // Ctrl-\ detaches; the client says so and leaves.
    send(&mut writer, b"\x1c");
    assert!(
        screen.wait_for(Duration::from_secs(10), |s| s.contains("detached from cli-test")),
        "the client did not detach: {:?}",
        screen.text()
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut exited = false;
    while Instant::now() < deadline {
        if let Ok(Some(_)) = client.try_wait() {
            exited = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(exited, "the client is still running after detaching");

    // The workspace is still there: detaching left it running.
    let listed = std::process::Command::new(keep_bin())
        .arg("ls")
        .env("KEEP_SOCKET", &socket)
        .env("KEEPD_BIN", keepd_bin())
        .output()
        .expect("keep ls");
    let listing = String::from_utf8_lossy(&listed.stdout);
    assert!(listing.contains("cli-test"), "the workspace did not outlive the client: {listing}");

    daemon.kill().ok();
    daemon.wait().ok();
    #[cfg(unix)]
    std::fs::remove_file(&socket).ok();
}
