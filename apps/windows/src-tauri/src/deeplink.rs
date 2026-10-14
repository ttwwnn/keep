//! The `claude-cli://` links, when the person has asked for them to open here (Settings, "Links do Claude Code"; the
//! registration itself is the core's, `keep links instalar`, and is undone by `keep links remover`).
//!
//! Windows runs the registered command, `"Keep.exe" --open-uri "<link>"`, for a link a browser opened. The link comes
//! from outside, so this takes nothing from it: it only hands the text to the core (`keep links abrir`), as one
//! argument with no shell around it, and the core checks it and opens the tab (crates/keep-restauro/src/deeplink.rs).
//! The app that receives the launch is either a fresh one (the link started it: `setup` reads its own arguments) or
//! the one already open (the single-instance plugin hands it the second launch's arguments).

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

/// What Windows puts before the link in the registered command.
const FLAG: &str = "--open-uri";

/// A link is a few hundred bytes; Windows hands over at most 32767 characters of command line.
const MOST_BYTES: usize = 32 * 1024;

/// How long the core keeps trying while the app is still starting its daemon.
const WAIT_SECONDS: u64 = 90;

/// The link in a launch's arguments (the program's own first): exactly `--open-uri <link>`, as the registered
/// command writes it. Anything more means the link broke out of its quotes, and none of it is taken.
pub fn link_in(args: &[String]) -> Option<String> {
    let [_, flag, link] = args else { return None };
    let scheme_ok = link.get(..11).is_some_and(|s| s.eq_ignore_ascii_case("claude-cli:"));
    (flag == FLAG && scheme_ok && link.len() <= MOST_BYTES).then(|| link.clone())
}

/// Open the link in a tab, off the window's thread; say so in a message when the core could not.
pub fn open(app: &AppHandle, resources: Option<PathBuf>, link: String) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(why) = ask_core(resources.as_deref(), &link) {
            crate::settings::log(&format!("link claude-cli://: {why}"));
            app.dialog().message(format!("Não consegui abrir o link numa aba do Keep.\n\n{why}")).title("Keep").kind(MessageDialogKind::Error).show(|_| {});
        }
    });
}

/// `keep links abrir`, waiting for the app's own daemon to come up. The text of the core's complaint when it refuses.
fn ask_core(resources: Option<&std::path::Path>, link: &str) -> Result<(), String> {
    let exe = crate::ia::cli(resources).ok_or_else(|| "o keep.exe não está junto do app".to_string())?;
    let mut cmd = Command::new(exe);
    cmd.args(["links", "abrir", "--esperar", &WAIT_SECONDS.to_string(), link])
        .env("KEEP_SOCKET", crate::daemon::address())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    // The same folders every question to the core is given (ia.rs): the names of the tabs it names are the app's.
    if let Some(dir) = crate::settings::keep_dir() {
        if std::env::var_os("KEEP_STATE_DIR").is_none() {
            cmd.env("KEEP_STATE_DIR", &dir);
        }
        if std::env::var_os("KEEP_RESTAURO_ESTADO").is_none() {
            cmd.env("KEEP_RESTAURO_ESTADO", dir.join("restauro"));
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| format!("o keep não abriu: {e}"))?;
    let mut err = child.stderr.take().ok_or_else(|| "o keep não abriu a saída".to_string())?;
    let reader = std::thread::spawn(move || {
        let mut said = String::new();
        let _ = err.by_ref().take(4096).read_to_string(&mut said);
        said
    });
    // The core waits as long as it was told and a little more, then the app stops waiting for it.
    let until = std::time::Instant::now() + Duration::from_secs(WAIT_SECONDS + 30);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() < until => std::thread::sleep(Duration::from_millis(100)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("o keep passou do tempo".into());
            }
            Err(e) => return Err(format!("o keep não respondeu: {e}")),
        }
    };
    let said = reader.join().unwrap_or_default();
    if status.success() {
        return Ok(());
    }
    Err(said.lines().rev().map(str::trim).find(|l| !l.is_empty()).map(str::to_string).unwrap_or_else(|| format!("o keep saiu com {}", status.code().unwrap_or(-1))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_link_is_taken_only_from_the_launch_the_registry_writes() {
        let link = "claude-cli://open?q=oi";
        assert_eq!(link_in(&args(&["Keep.exe", "--open-uri", link])), Some(link.to_string()));
        assert_eq!(link_in(&args(&["Keep.exe", "--open-uri", "CLAUDE-CLI://open"])), Some("CLAUDE-CLI://open".to_string()));
    }

    #[test]
    fn anything_else_is_not_a_link() {
        let link = "claude-cli://open?q=oi";
        for launch in [
            args(&["Keep.exe"]),
            args(&["Keep.exe", "--open-uri"]),
            args(&["Keep.exe", link]),
            args(&["Keep.exe", "--open-uri", link, "--extra"]),
            args(&["Keep.exe", "--open-uri", link, "--open-uri", link]),
            args(&["Keep.exe", "--other", link]),
            args(&["Keep.exe", "--open-uri", "https://example.com/claude-cli:"]),
            args(&["Keep.exe", "--open-uri", "claude-cli"]),
            args(&["Keep.exe", "--open-uri", ""]),
            args(&["Keep.exe", "--open-uri", "ação"]),
            args(&["--open-uri", link]),
        ] {
            assert_eq!(link_in(&launch), None, "{launch:?}");
        }
        let huge = format!("claude-cli://open?q={}", "a".repeat(MOST_BYTES));
        assert_eq!(link_in(&args(&["Keep.exe", "--open-uri", &huge])), None);
    }
}
