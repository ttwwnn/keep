//! `keep ia confinar` the way a tab runs it: inside a terminal of its own.
//!
//! The terminal is a PTY of the test's own (ConPTY, the one a tab uses), and
//! what the confined program draws is read back through libghostty-vt, the
//! same grid the daemon keeps. This proves what no run without a terminal can:
//! the restricted program gets the tab's console, writes to it, reads what is
//! typed into it, and still cannot touch what is outside its folder.

#![cfg(windows)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use keep_vt::Terminal;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

fn keep_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_keep"))
}

/// A folder under the profile (where the restriction is meant to hold),
/// removed when the test ends, whatever way it ends.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn json_string(path: &Path) -> String {
    format!("\"{}\"", path.display().to_string().replace('\\', "\\\\").replace('"', "\\\""))
}

#[test]
fn the_confined_program_gets_the_tabs_console() {
    let profile = PathBuf::from(std::env::var("USERPROFILE").expect("USERPROFILE"));
    let scratch = Scratch(profile.join(format!(".keep-confine-pty-{}", std::process::id())));
    let allowed = scratch.0.join("allowed");
    let outside = scratch.0.join("outside");
    std::fs::create_dir_all(&allowed).expect("allowed folder");
    std::fs::create_dir_all(&outside).expect("outside folder");
    let secret = outside.join("secret.txt");
    std::fs::write(&secret, "outside").expect("secret");
    let written = outside.join("written.txt");

    // The program: says it is there, tries what it must not be able to do,
    // reads a line typed into the tab, and answers with the code 7.
    let script = allowed.join("talk.cmd");
    std::fs::write(
        &script,
        format!(
            "@echo off\r\nsetlocal enabledelayedexpansion\r\n\
             echo mark-from-the-tab\r\n\
             type \"{secret}\" >nul 2>&1 && echo READ-OUTSIDE || echo DENIED-READ\r\n\
             echo x> \"{written}\"\r\n\
             if exist \"{written}\" (echo WROTE-OUTSIDE) else (echo DENIED-WRITE)\r\n\
             set /p typed=\r\n\
             echo typed-!typed!\r\n\
             exit /b 7\r\n",
            secret = secret.display(),
            written = written.display(),
        ),
    )
    .expect("script");

    // The workspace's folder, the way the app records it.
    let config = scratch.0.join("confinement.json");
    std::fs::write(&config, format!("{{\"jail\":{}}}", json_string(&allowed))).expect("config");

    let pty = native_pty_system()
        .openpty(PtySize { rows: 30, cols: 100, pixel_width: 0, pixel_height: 0 })
        .expect("openpty");
    let mut cmd = CommandBuilder::new(keep_bin());
    cmd.args(["ia", "confinar", "--"]);
    cmd.arg(script.display().to_string());
    cmd.env("KEEP_CONFINE_CONFIG", config.display().to_string());
    cmd.env("KEEP_WORKSPACE", "jail");
    cmd.cwd(&allowed);
    let mut child = pty.slave.spawn_command(cmd).expect("spawn keep");
    drop(pty.slave);

    let mut writer = pty.master.take_writer().expect("writer");
    let mut reader = pty.master.try_clone_reader().expect("reader");
    let grid = Arc::new(Mutex::new(Terminal::new(100, 30).expect("grid")));
    // ConPTY asks where the cursor is before it shows anything: the grid
    // answers the way a terminal would, and the answers are handed back below.
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
    let screen = || grid.lock().ok().and_then(|g| g.text().ok()).unwrap_or_default();
    let answer = |writer: &mut Box<dyn Write + Send>| {
        let pending = std::mem::take(&mut *replies.lock().unwrap());
        if !pending.is_empty() {
            writer.write_all(&pending).unwrap();
            writer.flush().unwrap();
        }
    };

    let wait_for = |writer: &mut Box<dyn Write + Send>, what: &str, seconds: u64| -> bool {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        while Instant::now() < deadline {
            answer(writer);
            if screen().contains(what) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    };

    assert!(
        wait_for(&mut writer, "mark-from-the-tab", 60),
        "the confined program never wrote to the tab: {:?}",
        screen()
    );
    assert!(wait_for(&mut writer, "DENIED-READ", 20), "it read what is outside its folder: {:?}", screen());
    assert!(wait_for(&mut writer, "DENIED-WRITE", 20), "it wrote outside its folder: {:?}", screen());
    assert!(!screen().contains("READ-OUTSIDE") && !screen().contains("WROTE-OUTSIDE"), "{:?}", screen());
    assert!(!written.exists(), "the file outside the folder was created");

    // What is typed into the tab reaches the program.
    writer.write_all(b"abc\r").unwrap();
    writer.flush().unwrap();
    assert!(
        wait_for(&mut writer, "typed-abc", 20),
        "what was typed into the tab never reached the confined program: {:?}",
        screen()
    );

    let deadline = Instant::now() + Duration::from_secs(20);
    let mut code = None;
    while Instant::now() < deadline {
        answer(&mut writer);
        if let Ok(Some(status)) = child.try_wait() {
            code = Some(status.exit_code());
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(code, Some(7), "keep did not hand back the program's exit code: {:?}", screen());
}
