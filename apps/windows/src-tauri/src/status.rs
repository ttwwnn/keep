//! What the bar under a conversation reads besides the screen.
//!
//! - Claude Code calls a command of the user's (`tools/keep-statusline.ps1`)
//!   with a JSON that names the model and the effort; the command saves it as
//!   `<shell pid>.json`, the shell being the tab's (the daemon lists it per
//!   tab). The page asks for the files of the tabs it shows and reads the JSON.
//! - Codex keeps the models it offers in `models_cache.json`: their slugs,
//!   their names on screen and the levels each takes. The page reads it again
//!   only when it changed (it is some hundreds of kilobytes).
//!
//! This side only finds and returns the text, the same way the macOS app does.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// A status file is a few hundred bytes; anything past this is not one.
const MAX_BYTES: u64 = 256 * 1024;

/// Codex's cache is some hundreds of kilobytes; past this it is not read.
const MAX_CACHE_BYTES: u64 = 16 * 1024 * 1024;

/// Where the files are: `KEEP_STATUS_DIR`, else `.local/state/keep/claude`
/// under the user's home (`%USERPROFILE%` on Windows), as the script writes.
pub fn directory(var: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let set = |name: &str| var(name).filter(|v| !v.trim().is_empty());
    if let Some(dir) = set("KEEP_STATUS_DIR") {
        return Some(PathBuf::from(dir));
    }
    let home = set("USERPROFILE").or_else(|| set("HOME"))?;
    Some(Path::new(&home).join(".local").join("state").join("keep").join("claude"))
}

/// The text of each shell's file that exists and can be read, by shell pid.
pub fn read(dir: &Path, shells: &[u32]) -> HashMap<u32, String> {
    let mut found = HashMap::new();
    for &pid in shells {
        if pid == 0 || found.contains_key(&pid) {
            continue;
        }
        let Ok(file) = std::fs::File::open(dir.join(format!("{pid}.json"))) else { continue };
        let mut text = String::new();
        if file.take(MAX_BYTES).read_to_string(&mut text).is_ok() && !text.trim().is_empty() {
            found.insert(pid, text);
        }
    }
    found
}

/// Codex's model cache: `models_cache.json` in `CODEX_HOME`, else in
/// `.codex` under the user's home, where Codex itself keeps it.
pub fn codex_cache(var: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let set = |name: &str| var(name).filter(|v| !v.trim().is_empty());
    let home = match set("CODEX_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(&set("USERPROFILE").or_else(|| set("HOME"))?).join(".codex"),
    };
    Some(home.join("models_cache.json"))
}

/// What tells one version of a file from the next: its size and when it was
/// last written. `None` when there is no file.
pub fn stamp(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let written = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(format!("{}-{}", meta.len(), written.as_nanos()))
}

/// A file as it stands now, if it is not the version the page has
/// (`known`): its stamp, and its text when that changed and it can be read.
pub fn if_changed(path: &Path, known: Option<&str>) -> (Option<String>, Option<String>) {
    let now = stamp(path);
    if now.is_none() || now.as_deref() == known {
        return (now, None);
    }
    let mut text = String::new();
    let read = std::fs::File::open(path)
        .and_then(|f| f.take(MAX_CACHE_BYTES).read_to_string(&mut text))
        .is_ok();
    (now, read.then_some(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string())
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("keep-status-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_variable_wins_then_the_windows_home_then_home() {
        let all = env(&[("KEEP_STATUS_DIR", "/tmp/x"), ("USERPROFILE", "C:/u"), ("HOME", "/h")]);
        assert_eq!(directory(all), Some(PathBuf::from("/tmp/x")));
        let windows = directory(env(&[("USERPROFILE", "C:/u"), ("HOME", "/h")])).unwrap();
        assert_eq!(windows, Path::new("C:/u").join(".local/state/keep/claude"));
        let unix = directory(env(&[("HOME", "/h")])).unwrap();
        assert_eq!(unix, Path::new("/h").join(".local/state/keep/claude"));
        assert_eq!(directory(env(&[])), None);
        assert_eq!(directory(env(&[("KEEP_STATUS_DIR", "  "), ("HOME", "/h")])), Some(unix));
    }

    #[test]
    fn reads_the_files_that_are_there() {
        let dir = scratch("reads");
        std::fs::write(dir.join("12.json"), r#"{"model":{"id":"a"}}"#).unwrap();
        std::fs::write(dir.join("13.json"), "   \n").unwrap();
        let got = read(&dir, &[12, 13, 14, 0, 12]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[&12], r#"{"model":{"id":"a"}}"#);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_directory_is_no_files() {
        assert!(read(Path::new("/nonexistent/keep/status"), &[1, 2]).is_empty());
    }

    #[test]
    fn codex_keeps_its_cache_in_its_home() {
        let set = directory_of(codex_cache(env(&[("CODEX_HOME", "/x"), ("USERPROFILE", "C:/u"), ("HOME", "/h")])));
        assert_eq!(set, PathBuf::from("/x"));
        let windows = codex_cache(env(&[("USERPROFILE", "C:/u"), ("HOME", "/h")])).unwrap();
        assert_eq!(windows, Path::new("C:/u").join(".codex").join("models_cache.json"));
        let unix = codex_cache(env(&[("HOME", "/h"), ("CODEX_HOME", "")])).unwrap();
        assert_eq!(unix, Path::new("/h").join(".codex").join("models_cache.json"));
        assert_eq!(codex_cache(env(&[])), None);
    }

    fn directory_of(path: Option<PathBuf>) -> PathBuf {
        let path = path.unwrap();
        assert_eq!(path.file_name().unwrap(), "models_cache.json");
        path.parent().unwrap().to_path_buf()
    }

    #[test]
    fn the_cache_is_read_again_only_when_it_changed() {
        let dir = scratch("cache");
        let path = dir.join("models_cache.json");
        assert_eq!(if_changed(&path, None), (None, None));
        std::fs::write(&path, r#"{"models":[]}"#).unwrap();
        let (first, text) = if_changed(&path, None);
        assert!(first.is_some());
        assert_eq!(text.as_deref(), Some(r#"{"models":[]}"#));
        // The same version: only its stamp.
        assert_eq!(if_changed(&path, first.as_deref()), (first.clone(), None));
        // Another one: its text again.
        std::fs::write(&path, r#"{"models":[{"slug":"a"}]}"#).unwrap();
        let (second, text) = if_changed(&path, first.as_deref());
        assert_ne!(second, first);
        assert_eq!(text.as_deref(), Some(r#"{"models":[{"slug":"a"}]}"#));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_too_big_to_be_one_is_cut_not_trusted() {
        let dir = scratch("big");
        std::fs::write(dir.join("5.json"), "x".repeat(MAX_BYTES as usize + 10)).unwrap();
        assert_eq!(read(&dir, &[5])[&5].len(), MAX_BYTES as usize);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
