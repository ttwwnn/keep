//! Claude Code's settings file, for the bar: read for the page, and written
//! back only as the page asks — the status line it sets up on opening, the
//! default a `/model <id>` saved put back (see `../src/claudeSettings.ts`) —
//! and only over the text the page read: a file Claude Code wrote in between
//! is left as Claude Code left it.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// A settings file is a few kilobytes; anything past this is not one.
const MAX_BYTES: u64 = 4 * 1024 * 1024;

/// Claude Code's own folder: `CLAUDE_CONFIG_DIR`, else `.claude` in the
/// user's home (`%USERPROFILE%` on Windows), as Claude Code itself reads it.
pub fn directory(var: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let set = |name: &str| var(name).filter(|v| !v.trim().is_empty());
    if let Some(dir) = set("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    let home = set("USERPROFILE").or_else(|| set("HOME"))?;
    Some(Path::new(&home).join(".claude"))
}

/// The file's text; `None` when there is no file.
pub fn read(path: &Path) -> Result<Option<String>> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("ler {}", path.display())),
    };
    let mut text = String::new();
    file.take(MAX_BYTES + 1).read_to_string(&mut text).with_context(|| format!("ler {}", path.display()))?;
    if text.len() as u64 > MAX_BYTES {
        bail!("{} é grande demais para ser um settings.json", path.display());
    }
    Ok(Some(text))
}

/// Write `text` over the file if it still holds `expected` (`None`: there
/// was no file). False, and nothing written, when it changed meanwhile. The
/// text goes to a file beside it first and is moved over it, so Claude Code
/// never reads half of it; a link is written through, not replaced.
pub fn replace(path: &Path, expected: Option<&str>, text: &str) -> Result<bool> {
    if read(path)?.as_deref() != expected {
        return Ok(false);
    }
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let dir = target.parent().context("o settings.json não tem pasta")?;
    std::fs::create_dir_all(dir).with_context(|| format!("criar {}", dir.display()))?;
    let aside = dir.join(format!(".settings.json.keep-{}", std::process::id()));
    std::fs::write(&aside, text).with_context(|| format!("gravar {}", aside.display()))?;
    if let Err(e) = std::fs::rename(&aside, &target) {
        let _ = std::fs::remove_file(&aside);
        return Err(e).with_context(|| format!("gravar {}", target.display()));
    }
    Ok(true)
}

/// A path the way a person writes it: without the `\\?\` that a Windows API
/// may put in front, which PowerShell and Git Bash do not take.
pub fn plain(path: &Path) -> String {
    let text = path.display().to_string();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    text.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string())
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("keep-claude-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn claude_codes_variable_then_the_windows_home_then_home() {
        let all = env(&[("CLAUDE_CONFIG_DIR", "/c"), ("USERPROFILE", "C:/u"), ("HOME", "/h")]);
        assert_eq!(directory(all), Some(PathBuf::from("/c")));
        assert_eq!(directory(env(&[("USERPROFILE", "C:/u"), ("HOME", "/h")])), Some(Path::new("C:/u").join(".claude")));
        assert_eq!(directory(env(&[("HOME", "/h")])), Some(Path::new("/h").join(".claude")));
        assert_eq!(directory(env(&[("CLAUDE_CONFIG_DIR", " "), ("HOME", "/h")])), Some(Path::new("/h").join(".claude")));
        assert_eq!(directory(env(&[])), None);
    }

    #[test]
    fn no_file_is_none_and_a_file_is_its_text() {
        let dir = scratch("read");
        let path = dir.join("settings.json");
        assert_eq!(read(&path).unwrap(), None);
        std::fs::write(&path, "{\"a\": 1}\n").unwrap();
        assert_eq!(read(&path).unwrap().as_deref(), Some("{\"a\": 1}\n"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn written_only_over_what_was_read() {
        let dir = scratch("replace");
        let path = dir.join("settings.json");
        // No file, none expected: made.
        assert!(replace(&path, None, "{\"x\": 1}\n").unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"x\": 1}\n");
        // The text read: written over.
        assert!(replace(&path, Some("{\"x\": 1}\n"), "{\"x\": 2}\n").unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"x\": 2}\n");
        // Changed meanwhile: left as it is.
        assert!(!replace(&path, Some("{\"x\": 1}\n"), "{\"x\": 3}\n").unwrap());
        assert!(!replace(&path, None, "{\"x\": 3}\n").unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"x\": 2}\n");
        // Nothing left beside it.
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names.len(), 1, "{names:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_folder_that_is_not_there_yet_is_made() {
        let dir = scratch("folder");
        let path = dir.join("nova").join("settings.json");
        assert!(replace(&path, None, "{}\n").unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_written_through() {
        let dir = scratch("link");
        let real = dir.join("real.json");
        let link = dir.join("settings.json");
        std::fs::write(&real, "{}\n").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(replace(&link, Some("{}\n"), "{\"y\": 1}\n").unwrap());
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "{\"y\": 1}\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_verbatim_prefix_goes() {
        assert_eq!(plain(Path::new(r"\\?\C:\Users\a\Keep\x.ps1")), r"C:\Users\a\Keep\x.ps1");
        assert_eq!(plain(Path::new(r"\\?\UNC\srv\share\x.ps1")), r"\\srv\share\x.ps1");
        assert_eq!(plain(Path::new(r"C:\Users\a\x.ps1")), r"C:\Users\a\x.ps1");
    }
}
