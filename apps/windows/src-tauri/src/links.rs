//! What a terminal's text links to: which of the paths it prints are there,
//! and opening a path or an address.
//!
//! The page spells each path every way it may have been meant (`src/paths.ts`)
//! and asks here which spelling exists. It opens only what it found, and
//! only the kinds of address a person means to open by clicking.

use std::sync::mpsc;
use std::time::Duration;

use serde::Serialize;

/// One group's first spelling that is there.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    /// Which spelling of the group it is.
    pub index: usize,
    pub path: String,
    /// A folder; a file otherwise.
    pub dir: bool,
}

/// What the file system is given for a whole question. A share that does not
/// answer must not hold the window: its words just are not links.
const ASK_FOR: Duration = Duration::from_millis(2000);

/// The names Windows keeps for devices, with any extension: `NUL.txt` is the null device too.
fn is_device_name(path: &str) -> bool {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let stem = name.split('.').next().unwrap_or(name).trim_end().to_ascii_uppercase();
    let numbered = |prefix: &str| {
        stem.strip_prefix(prefix).is_some_and(|n| matches!(n.as_bytes(), [b'1'..=b'9']))
    };
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") || numbered("COM") || numbered("LPT")
}

/// `Some(true)` for a folder, `Some(false)` for a file, `None` for neither.
fn kind(path: &str) -> Option<bool> {
    if path.is_empty() || (cfg!(windows) && is_device_name(path)) {
        return None;
    }
    let meta = std::fs::metadata(path).ok()?;
    if meta.is_dir() {
        Some(true)
    } else if meta.is_file() {
        Some(false)
    } else {
        None
    }
}

fn first(group: &[String]) -> Option<Hit> {
    group
        .iter()
        .enumerate()
        .find_map(|(index, path)| kind(path).map(|dir| Hit { index, path: path.clone(), dir }))
}

/// For each group of spellings of one path, the first that exists.
pub fn find(groups: Vec<Vec<String>>) -> Vec<Option<Hit>> {
    let count = groups.len();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(groups.iter().map(|group| first(group)).collect::<Vec<_>>());
    });
    receiver.recv_timeout(ASK_FOR).unwrap_or_else(|_| vec![None; count])
}

/// A test run is not to open anything on the machine it runs on: with
/// `KEEP_LINK_LOG` set, what would have been opened is appended to that file
/// instead (the macOS app has the same hook). True when it was.
fn recorded(line: &str) -> bool {
    use std::io::Write;
    let Some(file) = std::env::var_os("KEEP_LINK_LOG").filter(|f| !f.is_empty()) else { return false };
    if let Ok(mut log) = std::fs::OpenOptions::new().create(true).append(true).open(file) {
        let _ = writeln!(log, "{line}");
    }
    true
}

/// Open a file with the program the system has for it, a folder in the file manager.
pub fn open(path: &str) -> Result<(), String> {
    if recorded(path) {
        return Ok(());
    }
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| e.to_string())
}

/// Show a file in the file manager, selected in its folder.
pub fn reveal(path: &str) -> Result<(), String> {
    if recorded(&format!("reveal {path}")) {
        return Ok(());
    }
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
}

/// What a web address may begin with to be opened from the terminal's text:
/// pages, mail and FTP. The system would hand any other scheme to whatever
/// program registered it, and text a program printed is not to start programs.
fn is_openable(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    ["http://", "https://", "mailto:", "ftp://"].iter().any(|scheme| lower.starts_with(scheme))
}

/// Open an address in the browser, or the mail program.
pub fn open_url(url: &str) -> Result<(), String> {
    if !is_openable(url) {
        return Err(format!("endereço que o Keep não abre: {url}"));
    }
    if recorded(url) {
        return Ok(());
    }
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices_are_not_files() {
        for name in ["NUL", "nul.txt", "C:\\x\\con", "C:\\x\\Aux.log", "COM1", "lpt9.dat", "prn "] {
            assert!(is_device_name(name), "{name}");
        }
        for name in ["COM0", "COM10", "console", "nullable.txt", "C:\\x\\comm", "a.nul", "LPT"] {
            assert!(!is_device_name(name), "{name}");
        }
    }

    #[test]
    fn only_pages_mail_and_ftp_are_opened() {
        for url in ["https://example.com/a", "HTTP://x.org", "mailto:a@b.c", "ftp://files.x.org/a"] {
            assert!(is_openable(url), "{url}");
        }
        for url in ["file:///C:/x.exe", "ms-msdt:/id", "javascript:alert(1)", "ssh://host", "tel:+5511999990000", "C:\\x.exe", ""] {
            assert!(!is_openable(url), "{url}");
        }
    }

    #[test]
    fn the_first_spelling_that_is_there_wins() {
        let dir = std::env::temp_dir().join(format!("keep-paths-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub dir")).unwrap();
        std::fs::write(dir.join("a.txt"), b"").unwrap();
        let at = |name: &str| dir.join(name).to_string_lossy().into_owned();

        let hits = find(vec![
            vec![at("missing.txt"), at("a.txt"), at("sub dir")],
            vec![at("sub dir")],
            vec![at("nope"), at("also-nope")],
            vec![],
            vec![String::new()],
        ]);
        assert_eq!(hits[0], Some(Hit { index: 1, path: at("a.txt"), dir: false }));
        assert_eq!(hits[1], Some(Hit { index: 0, path: at("sub dir"), dir: true }));
        assert_eq!(hits[2], None);
        assert_eq!(hits[3], None);
        assert_eq!(hits[4], None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
