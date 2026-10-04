//! What the app remembers between runs, and the shell new tabs start with.
//!
//! The daemon owns the facts — which workspaces and tabs exist, what runs in
//! them. The app keeps only what is the app's: the order of the sidebar, the
//! names given to tabs, which groups are folded, the zoom, and a record of the
//! workspaces as they were, so they can be opened again after the machine
//! restarts and the daemon starts empty.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

/// The file the interface's state lives in. `KEEP_STATE_DIR` moves it, for
/// tests that must not touch the real one.
pub fn state_file(config_dir: &Path) -> PathBuf {
    match std::env::var("KEEP_STATE_DIR") {
        Ok(dir) => PathBuf::from(dir).join("state.json"),
        Err(_) => config_dir.join("state.json"),
    }
}

pub fn load(path: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Written whole to a temporary file and moved into place, so a crash in the
/// middle leaves the last good copy rather than half of a new one.
pub fn save(path: &Path, value: &serde_json::Value) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("criar {}", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Where the daemon looks for the shell chosen here: `%APPDATA%\Keep\shell.txt`.
/// Read by keepd for every new tab, so a choice applies without restarting
/// the daemon that holds the tabs already open.
pub fn shell_file() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("KEEP_STATE_DIR") {
        return Some(PathBuf::from(dir).join("shell.txt"));
    }
    let appdata = std::env::var("APPDATA").ok()?;
    Some(Path::new(&appdata).join("Keep").join("shell.txt"))
}

pub fn chosen_shell() -> String {
    shell_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

pub fn choose_shell(command: &str) -> Result<()> {
    let path = shell_file().context("sem pasta de configuração")?;
    if command.trim().is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, format!("{}\n", command.trim()))?;
    Ok(())
}

/// A shell this machine has, offered in the settings.
#[derive(Serialize, Clone, Debug)]
pub struct Shell {
    pub id: String,
    pub label: String,
    pub command: String,
}

/// The shells installed here, best first. The first one is what a new tab
/// gets when nothing was chosen, which is the daemon's own rule.
pub fn shells() -> Vec<Shell> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        let on_path = |exe: &str| -> Option<PathBuf> {
            let path = std::env::var_os("PATH")?;
            std::env::split_paths(&path).map(|d| d.join(exe)).find(|p| p.is_file())
        };
        let program_files = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into());
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let quote = |p: &Path| format!("\"{}\"", p.display());

        let pwsh = on_path("pwsh.exe").or_else(|| {
            let p = Path::new(&program_files).join(r"PowerShell\7\pwsh.exe");
            p.is_file().then_some(p)
        });
        if let Some(p) = pwsh {
            out.push(Shell { id: "pwsh".into(), label: "PowerShell 7".into(), command: format!("{} -NoLogo", quote(&p)) });
        }
        let powershell = Path::new(&root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
        if powershell.is_file() {
            out.push(Shell {
                id: "powershell".into(),
                label: "Windows PowerShell".into(),
                command: format!("{} -NoLogo", quote(&powershell)),
            });
        }
        let cmd = Path::new(&root).join(r"System32\cmd.exe");
        if cmd.is_file() {
            out.push(Shell { id: "cmd".into(), label: "Prompt de Comando".into(), command: quote(&cmd) });
        }
        let git_bash = Path::new(&program_files).join(r"Git\bin\bash.exe");
        if git_bash.is_file() {
            out.push(Shell {
                id: "git-bash".into(),
                label: "Git Bash".into(),
                command: format!("{} --login -i", quote(&git_bash)),
            });
        }
        let wsl = Path::new(&root).join(r"System32\wsl.exe");
        if wsl.is_file() {
            out.push(Shell { id: "wsl".into(), label: "WSL".into(), command: quote(&wsl) });
        }
    }
    #[cfg(not(windows))]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        out.push(Shell { id: "login".into(), label: shell.clone(), command: shell });
    }
    out
}

/// The Windows build, which the page's terminal uses to match how ConPTY
/// reflows lines. Zero elsewhere.
pub fn windows_build() -> u32 {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;
        #[link(name = "ntdll")]
        unsafe extern "system" {
            fn RtlGetVersion(info: *mut OSVERSIONINFOW) -> i32;
        }
        let mut info: OSVERSIONINFOW = unsafe { std::mem::zeroed() };
        info.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;
        if unsafe { RtlGetVersion(&mut info) } == 0 {
            return info.dwBuildNumber;
        }
        0
    }
    #[cfg(not(windows))]
    0
}
