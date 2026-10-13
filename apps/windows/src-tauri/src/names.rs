//! The names given to tabs, where the core can see them.
//!
//! The window keeps the names in its own state (`state.json`), and the core —
//! `keep restauro`, which writes down every tab for the next restart, and
//! `keep nova --nome`, which names the tab it opens — keeps them in the
//! macOS app's file: `names.json` in the Keep's folder (`settings::keep_dir`),
//! `{"daemonStart": <seconds>, "tabs": {"<workspace>␟<tab>": "<name>"},
//! "workspaces": {…}}`, true for the one daemon that started at that moment.
//! This is that file's two ends in the window: what the page writes and
//! reads; what it makes of the names is `../src/names.ts`.
//!
//! The core takes `.state.lock` of the folder while it changes the file, and
//! so does the page's writing, which is a whole file moved into place.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub fn file() -> Option<PathBuf> {
    crate::settings::keep_dir().map(|dir| dir.join("names.json"))
}

/// The file as it stands if it is not the version `known`: its stamp (`None`
/// when there is no file) and, when that changed, its text.
pub fn read(path: &Path, known: Option<&str>) -> (Option<String>, Option<String>) {
    crate::status::if_changed(path, known)
}

/// `text` as the file, whole or not at all, under the core's lock; the stamp of
/// what was written.
pub fn write(path: &Path, text: &str) -> Result<Option<String>> {
    let dir = path.parent().context("o names.json não tem pasta")?;
    std::fs::create_dir_all(dir).with_context(|| format!("criar {}", dir.display()))?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(".state.lock"))
        .context("abrir .state.lock")?;
    // Held until `lock` is dropped; the core holds it while it changes the file.
    lock.lock().context("esperar .state.lock")?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).with_context(|| format!("gravar {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("gravar {}", path.display()))?;
    Ok(crate::status::stamp(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("keep-names-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn written_whole_and_read_back_only_when_it_changed() {
        let dir = scratch("roundtrip");
        let path = dir.join("names.json");
        assert_eq!(read(&path, None), (None, None), "no file yet");
        let first = write(&path, r#"{"daemonStart":1.5,"tabs":{"a\u001f1":"Um"},"workspaces":{}}"#).unwrap();
        assert!(first.is_some());
        // The version the page has: only its stamp.
        assert_eq!(read(&path, first.as_deref()), (first.clone(), None));
        // Another one: its text.
        let second = write(&path, r#"{"daemonStart":1.5,"tabs":{},"workspaces":{}}"#).unwrap();
        assert_ne!(first, second);
        let (stamp, text) = read(&path, first.as_deref());
        assert_eq!(stamp, second);
        assert_eq!(text.as_deref(), Some(r#"{"daemonStart":1.5,"tabs":{},"workspaces":{}}"#));
        // The lock file stays beside it, the temporary one does not.
        let mut names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, [".state.lock", "names.json"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
