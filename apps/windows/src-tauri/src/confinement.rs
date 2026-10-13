//! The folder each workspace's AI is held to.
//!
//! One file, `~/.config/keep/confinement.json`, as `{"workspace": "C:\\folder"}`:
//! the file and the format the macOS app writes (WorkspaceConfinement.swift)
//! and `keep ia confinar` reads (crates/keep-ia/src/confinar.rs). This only
//! keeps the choice. It is the shell side that holds the AI to it when it
//! starts, so the choice reaches the sessions started after it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

/// `KEEP_CONFINE_CONFIG` (tests), else `.config/keep/confinement.json` under
/// the home the core uses (`KEEP_IA_HOME`, `USERPROFILE`, `HOME`).
pub fn path() -> PathBuf {
    if let Some(named) = std::env::var_os("KEEP_CONFINE_CONFIG").filter(|v| !v.is_empty()) {
        return PathBuf::from(named);
    }
    let home = ["KEEP_IA_HOME", "USERPROFILE", "HOME"]
        .iter()
        .find_map(|var| std::env::var_os(var).filter(|v| !v.is_empty()))
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join(".config").join("keep").join("confinement.json")
}

/// Every workspace that has a folder. A file that is missing or is not an
/// object reads as none; a value that is not a text, or is empty, is dropped.
pub fn read(file: &Path) -> BTreeMap<String, String> {
    let Ok(text) = std::fs::read_to_string(file) else { return BTreeMap::new() };
    let Ok(Value::Object(entries)) = serde_json::from_str::<Value>(&text) else { return BTreeMap::new() };
    entries
        .into_iter()
        .filter_map(|(workspace, folder)| match folder {
            Value::String(f) if !f.is_empty() => Some((workspace, f)),
            _ => None,
        })
        .collect()
}

/// Choose a folder for a workspace, or none to take it back; the folders as
/// they stand after. Written whole and renamed into place, so a reader (the
/// core, as an AI starts) never sees half of it.
pub fn write(file: &Path, workspace: &str, folder: Option<&str>) -> Result<BTreeMap<String, String>> {
    let mut next = read(file);
    match folder.filter(|f| !f.is_empty()) {
        Some(f) => {
            next.insert(workspace.to_string(), f.to_string());
        }
        None => {
            next.remove(workspace);
        }
    }
    let dir = file.parent().filter(|d| !d.as_os_str().is_empty());
    if let Some(dir) = dir {
        std::fs::create_dir_all(dir).with_context(|| format!("criar {}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(&next)?;
    let aside = file.with_extension(format!("json.{}.tmp", std::process::id()));
    std::fs::write(&aside, text + "\n").with_context(|| format!("gravar {}", aside.display()))?;
    std::fs::rename(&aside, file).with_context(|| format!("trocar {}", file.display())).inspect_err(|_| {
        let _ = std::fs::remove_file(&aside);
    })?;
    Ok(next)
}

/// Every workspace's folder, by workspace name.
#[tauri::command]
pub fn confinement_all() -> BTreeMap<String, String> {
    read(&path())
}

/// Hold a workspace's AI to a folder, or let it go (`None`).
#[tauri::command]
pub fn confinement_set(workspace: String, folder: Option<String>) -> Result<BTreeMap<String, String>, String> {
    if workspace.trim().is_empty() {
        return Err("workspace sem nome".into());
    }
    if let Some(f) = folder.as_deref().filter(|f| !f.is_empty()) {
        if !Path::new(f).is_dir() {
            return Err(format!("a pasta {f} não existe"));
        }
    }
    write(&path(), &workspace, folder.as_deref()).map_err(|e| format!("{e:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("keep-app-confinement-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_missing_or_wrong_file_reads_as_none() {
        let dir = scratch("missing");
        assert!(read(&dir.join("confinement.json")).is_empty());
        std::fs::create_dir_all(&dir).unwrap();
        for text in ["", "not json", "[1,2]", "\"x\"", "null"] {
            std::fs::write(dir.join("c.json"), text).unwrap();
            assert!(read(&dir.join("c.json")).is_empty(), "{text:?}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_a_name_with_a_folder_counts() {
        let dir = scratch("kinds");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("c.json");
        std::fs::write(&file, r#"{"a": "C:\\x", "b": "", "c": 3, "d": null, "e": ["f"], "g": "/y"}"#).unwrap();
        let all = read(&file);
        assert_eq!(all.keys().collect::<Vec<_>>(), ["a", "g"]);
        assert_eq!(all["a"], "C:\\x");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_choice_is_written_whole_sorted_and_taken_back() {
        let dir = scratch("write");
        let file = dir.join("deep").join("keep").join("confinement.json");
        let first = write(&file, "zeta", Some("C:\\z")).unwrap();
        assert_eq!(first.len(), 1);
        let second = write(&file, "alfa", Some("C:\\a")).unwrap();
        assert_eq!(second.keys().collect::<Vec<_>>(), ["alfa", "zeta"]);
        // The format the core and the macOS app read: an object, keys in order.
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.find("alfa").unwrap() < text.find("zeta").unwrap());
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["zeta"], "C:\\z");
        // Another folder for the same name replaces it; none, or empty, lets it go.
        assert_eq!(write(&file, "alfa", Some("C:\\b")).unwrap()["alfa"], "C:\\b");
        assert_eq!(write(&file, "alfa", None).unwrap().keys().collect::<Vec<_>>(), ["zeta"]);
        assert!(write(&file, "zeta", Some("")).unwrap().is_empty());
        assert!(read(&file).is_empty());
        // Nothing of the write is left beside the file.
        assert_eq!(std::fs::read_dir(file.parent().unwrap()).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn what_the_core_wrote_is_kept_and_what_it_could_not_read_is_dropped() {
        let dir = scratch("kept");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("c.json");
        std::fs::write(&file, r#"{"presa": "C:\\p", "lixo": 1}"#).unwrap();
        let after = write(&file, "outra", Some("C:\\o")).unwrap();
        assert_eq!(after.keys().collect::<Vec<_>>(), ["outra", "presa"]);
        assert!(!std::fs::read_to_string(&file).unwrap().contains("lixo"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_folder_that_is_not_there_is_refused() {
        let missing = scratch("nowhere").join("nada");
        let answer = confinement_set("alfa".into(), Some(missing.to_string_lossy().into_owned()));
        assert!(answer.unwrap_err().contains("não existe"));
        assert!(confinement_set("  ".into(), None).is_err());
    }
}
