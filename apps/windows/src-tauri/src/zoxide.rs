//! The folders the person goes to, from zoxide: the "abrir em" half of the
//! picker. zoxide keeps them by how often and how lately they were visited,
//! which is the honest source for somewhere to start something new — the one
//! the shell's `z` uses, not a list this app invents. Without zoxide the
//! answer is empty, and the page falls back to the folders its tabs are in.

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long zoxide may take. It reads one small file; a picker waiting longer
/// than this on folders is a picker that did not open.
const WITHIN: Duration = Duration::from_millis(1500);

/// The most folders the picker is given.
const LIMIT: usize = 200;

/// Where zoxide may be: on the `PATH`, then where the installers put it. A
/// window started from a launcher may not have the shell's `PATH` — the macOS
/// app, which is developed here, never does — so the usual places are tried by
/// hand.
fn candidates(var: &dyn Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    let name = format!("zoxide{}", std::env::consts::EXE_SUFFIX);
    let mut found = Vec::new();
    if let Some(path) = var("PATH") {
        found.extend(std::env::split_paths(&path).map(|dir| dir.join(&name)));
    }
    let under = |base: &str, rest: &[&str]| {
        var(base).map(|b| rest.iter().fold(PathBuf::from(b), |dir, part| dir.join(part)).join(&name))
    };
    // winget's links, scoop's shims, cargo's bin, Chocolatey's.
    found.extend(under("LOCALAPPDATA", &["Microsoft", "WinGet", "Links"]));
    found.extend(under("USERPROFILE", &["scoop", "shims"]));
    found.extend(under("USERPROFILE", &[".cargo", "bin"]));
    found.extend(under("ProgramData", &["chocolatey", "bin"]));
    // Homebrew's and the system's, for the Mac the app is developed on.
    for dir in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
        found.push(Path::new(dir).join(&name));
    }
    found.extend(under("HOME", &[".local", "bin"]));
    found
}

/// zoxide's lines as folders: trimmed, none blank, none twice, the first `limit`.
fn parse(output: &str, limit: usize) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && seen.insert(*line))
        .take(limit)
        .map(str::to_string)
        .collect()
}

/// `zoxide query --list`, stopped when it outlives `within`.
fn run(exe: &Path, within: Duration) -> Option<String> {
    let mut cmd = Command::new(exe);
    cmd.args(["query", "--list"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().ok()?;
    // Read while it runs, so a long list cannot fill the pipe and stall it.
    let mut out = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut data = Vec::new();
        let _ = out.read_to_end(&mut data);
        data
    });
    let until = Instant::now() + within;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    Some(String::from_utf8_lossy(&reader.join().ok()?).into_owned())
}

/// The folders zoxide knows, best first; none when it is not installed, does
/// not answer in time, or has not learned any yet.
pub fn directories() -> Vec<String> {
    let Some(exe) = candidates(&|name| std::env::var_os(name)).into_iter().find(|p| p.is_file()) else {
        return Vec::new();
    };
    run(&exe, WITHIN).map(|out| parse(&out, LIMIT)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lines_become_folders() {
        let clinica = "C:\\Users\\ana\\www\\clinica";
        let api = "C:\\Users\\ana\\www\\api";
        let out = format!("{clinica}\r\n\r\n  {api}  \n{clinica}\n/home/ana/x\n");
        assert_eq!(parse(&out, 10), [clinica, api, "/home/ana/x"]);
        assert_eq!(parse(&out, 2).len(), 2);
        assert!(parse("", 10).is_empty());
    }

    #[test]
    fn the_path_comes_before_the_usual_places() {
        let env = |name: &str| match name {
            "PATH" => Some(OsString::from(if cfg!(windows) { "C:\\bin;D:\\tools" } else { "/a:/b" })),
            "LOCALAPPDATA" => Some(OsString::from("C:\\Users\\ana\\AppData\\Local")),
            "USERPROFILE" => Some(OsString::from("C:\\Users\\ana")),
            _ => None,
        };
        let found = candidates(&env);
        let name = format!("zoxide{}", std::env::consts::EXE_SUFFIX);
        assert!(found[0].ends_with(&name));
        let local = Path::new("C:\\Users\\ana\\AppData\\Local");
        let links = local.join("Microsoft").join("WinGet").join("Links").join(&name);
        assert!(found.contains(&links));
        assert!(found.iter().position(|p| *p == links) > Some(1));
        // An environment with none of it leaves the system's own places only.
        assert_eq!(candidates(&|_| None).len(), 3);
    }

    #[cfg(unix)]
    fn script(name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("keep-zoxide-test-{}-{name}", std::process::id()));
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn it_asks_for_the_list_and_gives_up_on_a_hung_one() {
        let good = script("good", "[ \"$1 $2\" = \"query --list\" ] && printf '/a/b\\n/c/d\\n'");
        let listed = run(&good, Duration::from_secs(5)).map(|out| parse(&out, 10));
        assert_eq!(listed, Some(vec!["/a/b".to_string(), "/c/d".to_string()]));
        let slow = script("slow", "sleep 5");
        let started = Instant::now();
        assert_eq!(run(&slow, Duration::from_millis(150)), None);
        assert!(started.elapsed() < Duration::from_secs(3));
        let failing = script("failing", "exit 1");
        assert_eq!(run(&failing, Duration::from_secs(5)), None);
        for path in [good, slow, failing] {
            let _ = std::fs::remove_file(path);
        }
    }
}
