//! What the app remembers between runs, and the shell new tabs start with.
//!
//! The daemon owns the facts — which workspaces and tabs exist, what runs in
//! them. The app keeps only what is the app's: the order of the sidebar, the
//! names given to tabs, which groups are folded, the zoom, and a record of the
//! workspaces as they were, so they can be opened again after the machine
//! restarts and the daemon starts empty.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

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

/// The file is read and written by one thing at a time: a recovery that is
/// putting a copy back must not meet a save that is replacing it.
static STATE_LOCK: Mutex<()> = Mutex::new(());

/// What the last reading of the state found wrong, for the page to say once.
static WARNING: Mutex<Option<String>> = Mutex::new(None);

/// The state could not be read when the app opened (as opposed to being read
/// and found bad): nothing is saved over it this time.
static UNREAD: AtomicBool = AtomicBool::new(false);

/// How many older copies of the state are kept beside it (`state.json.bak1`,
/// the newest, to `.bak5`), and how far apart in time they are taken: the
/// copies are what the state was a while ago, not what it was a moment ago,
/// so that a bad write that goes unnoticed for a few minutes is still behind
/// them. The macOS app keeps a copy of every state it replaces
/// (`KeepStateFile.archive`); here they rotate.
const COPIES: usize = 5;
const COPY_EVERY: Duration = Duration::from_secs(10 * 60);

fn copy_path(path: &Path, n: usize) -> PathBuf {
    path.with_extension(format!("json.bak{n}"))
}

/// Where a state that could not be read is kept, rather than written over.
fn unreadable_path(path: &Path) -> PathBuf {
    path.with_extension("json.ilegivel")
}

/// A state file: a JSON object, with or without a byte order mark.
fn parse(bytes: &[u8]) -> Option<serde_json::Value> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    serde_json::from_slice::<serde_json::Value>(bytes).ok().filter(serde_json::Value::is_object)
}

fn warn(message: String) {
    log(&format!("estado: {message}"));
    if let Ok(mut said) = WARNING.lock() {
        *said = Some(message);
    }
}

/// What reading the state found wrong, once: the page tells the person.
pub fn take_warning() -> Option<String> {
    WARNING.lock().ok().and_then(|mut said| said.take())
}

/// "há 12 min", "há 3 h", "há 2 dias": how long ago a copy was taken.
pub fn age_text(age: Duration) -> String {
    let minutes = age.as_secs() / 60;
    match minutes {
        0 => "agora há pouco".into(),
        1..=59 => format!("há {minutes} min"),
        60..=1439 => format!("há {} h", minutes / 60),
        _ => format!("há {} dias", minutes / 1440),
    }
}

/// The state as it was left, or `None` for a first run. A file that cannot be
/// read is kept aside (`state.json.ilegivel`) and the newest copy that can is
/// put in its place; with none, the app starts from its defaults and says so.
/// Either way the person is told (`take_warning`), where before an unreadable
/// state quietly became the defaults — and the next save made that final.
pub fn load(path: &Path) -> Option<serde_json::Value> {
    let (state, said) = read_state(path, SystemTime::now());
    if let Some(message) = said {
        warn(message);
    }
    state
}

fn read_state(path: &Path, now: SystemTime) -> (Option<serde_json::Value>, Option<String>) {
    let _only = STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (None, None),
        Err(e) => {
            // Not that it is bad: it could not be read just now (a program
            // holding it, say). Saving the defaults over it would make that
            // final, so nothing is saved this time.
            UNREAD.store(true, Ordering::SeqCst);
            let said = format!("O Keep não conseguiu ler o arquivo de estado ({e}); as mudanças desta vez não serão guardadas.");
            return (None, Some(said));
        }
    };
    match parse(&bytes) {
        Some(value) => (Some(value), None),
        None => {
            let (value, said) = recover(path, &bytes, now);
            (value, Some(said))
        }
    }
}

/// The newest copy that reads, put back as the state, and what to tell the
/// person; the unreadable file is kept.
fn recover(path: &Path, bad: &[u8], now: SystemTime) -> (Option<serde_json::Value>, String) {
    let aside = unreadable_path(path);
    let kept = std::fs::write(&aside, bad).is_ok();
    let shown = if kept { aside.display().to_string() } else { path.display().to_string() };
    for n in 1..=COPIES {
        let copy = copy_path(path, n);
        let Ok(bytes) = std::fs::read(&copy) else { continue };
        let Some(value) = parse(&bytes) else { continue };
        if write_atomic(path, &bytes).is_err() {
            break;
        }
        let age = std::fs::metadata(&copy)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .map(age_text)
            .unwrap_or_else(|| "de antes".into());
        let said = format!(
            "O arquivo de estado do Keep estava ilegível. Voltei à cópia de {age}; o arquivo com problema ficou em {shown}."
        );
        return (Some(value), said);
    }
    let said = format!(
        "O arquivo de estado do Keep estava ilegível e não havia cópia boa: o Keep começou do zero. O arquivo com problema ficou em {shown}."
    );
    (None, said)
}

/// Written whole to a temporary file, on the disk before it replaces the
/// state, and moved into place, so a crash in the middle leaves the last good
/// copy rather than half of a new one. The older copies rotate first.
pub fn save(path: &Path, value: &serde_json::Value) -> Result<()> {
    save_at(path, value, SystemTime::now())
}

fn save_at(path: &Path, value: &serde_json::Value, now: SystemTime) -> Result<()> {
    let _only = STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if UNREAD.load(Ordering::SeqCst) {
        anyhow::bail!("o estado não pôde ser lido ao abrir: nada é gravado por cima dele");
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("criar {}", dir.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(value)?;
    match std::fs::read(path) {
        Ok(old) if old == bytes => return Ok(()),
        Ok(old) if parse(&old).is_some() => rotate(path, &old, now).context("guardar a cópia do estado")?,
        // Not a state: kept as it is before the new one goes over it.
        Ok(old) => std::fs::write(unreadable_path(path), old).context("guardar o estado ilegível")?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| format!("ler {} antes de gravar", path.display())),
    }
    write_atomic(path, &bytes)
}

/// `old`, the state about to be replaced, becomes the newest copy — unless the
/// newest is recent already, or says the same.
fn rotate(path: &Path, old: &[u8], now: SystemTime) -> Result<()> {
    let newest = copy_path(path, 1);
    if let Ok(copy) = std::fs::metadata(&newest) {
        let taken = copy.modified().ok().and_then(|t| now.duration_since(t).ok());
        if taken.is_some_and(|age| age < COPY_EVERY) || std::fs::read(&newest).is_ok_and(|b| b == old) {
            return Ok(());
        }
    }
    for n in (1..COPIES).rev() {
        let from = copy_path(path, n);
        if from.exists() {
            std::fs::rename(&from, copy_path(path, n + 1))?;
        }
    }
    write_file(&newest, old)
}

/// A file on the disk, not only in the system's cache.
fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(path).with_context(|| format!("criar {}", path.display()))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    write_file(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        // The rename itself on the disk too (Windows keeps its own journal).
        let _ = std::fs::File::open(dir).and_then(|d| d.sync_all());
    }
    Ok(())
}

/// The app's log: `app.log` in `KEEP_STATE_DIR` (a test's), else in
/// `%LOCALAPPDATA%\Keep` beside the daemon's, else in `~/.local/state/keep`.
pub fn log_file() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("KEEP_STATE_DIR") {
        return Some(PathBuf::from(dir).join("app.log"));
    }
    if let Ok(dir) = std::env::var("LOCALAPPDATA") {
        return Some(Path::new(&dir).join("Keep").join("app.log"));
    }
    let home = std::env::var("HOME").ok()?;
    Some(Path::new(&home).join(".local").join("state").join("keep").join("app.log"))
}

/// Past this the log starts again, the previous one kept as `app.log.1`.
const LOG_BYTES: u64 = 512 * 1024;

/// One line at the end of the app's log; nothing when it cannot be written.
pub fn log(line: &str) {
    let Some(path) = log_file() else { return };
    append(&path, line, LOG_BYTES);
}

fn append(path: &Path, line: &str, limit: u64) {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(path).map(|m| m.len() > limit).unwrap_or(false) {
        let _ = std::fs::rename(path, path.with_extension("log.1"));
    }
    let one = line.replace(['\r', '\n'], " ");
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{one}");
    }
}

/// The folder the app, the daemon and the core share for what is the Keep's
/// own: `KEEP_STATE_DIR` (a test's), else `%APPDATA%\Keep`. The shell chosen
/// here, the pictures of the tabs and the names given to them are in it.
pub fn keep_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("KEEP_STATE_DIR") {
        return Some(PathBuf::from(dir));
    }
    let appdata = std::env::var("APPDATA").ok()?;
    Some(Path::new(&appdata).join("Keep"))
}

/// Where the daemon looks for the shell chosen here: `%APPDATA%\Keep\shell.txt`.
/// Read by keepd for every new tab, so a choice applies without restarting
/// the daemon that holds the tabs already open.
pub fn shell_file() -> Option<PathBuf> {
    keep_dir().map(|dir| dir.join("shell.txt"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("keep-state-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn at(minutes: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000 + minutes * 60)
    }

    fn set_mtime(path: &Path, when: SystemTime) {
        std::fs::File::options().write(true).open(path).unwrap().set_modified(when).unwrap();
    }

    #[test]
    fn a_save_leaves_the_state_whole_and_the_copies_rotate_only_when_old_enough() {
        let dir = scratch("rotate");
        let path = dir.join("state.json");
        save_at(&path, &json!({"zoom": 1}), at(0)).unwrap();
        assert!(!copy_path(&path, 1).exists(), "nothing to copy on the first save");
        // A change a moment later: the first copy is made of what was there.
        save_at(&path, &json!({"zoom": 2}), at(1)).unwrap();
        assert_eq!(parse(&std::fs::read(copy_path(&path, 1)).unwrap()).unwrap(), json!({"zoom": 1}));
        set_mtime(&copy_path(&path, 1), at(1));
        // Many changes within the interval: the copy is not touched again.
        for z in 3..9 {
            save_at(&path, &json!({"zoom": z}), at(2 + z)).unwrap();
        }
        assert_eq!(parse(&std::fs::read(copy_path(&path, 1)).unwrap()).unwrap(), json!({"zoom": 1}));
        assert!(!copy_path(&path, 2).exists());
        // Past the interval, what was there goes to the newest copy and the others step back.
        save_at(&path, &json!({"zoom": 20}), at(30)).unwrap();
        assert_eq!(parse(&std::fs::read(copy_path(&path, 1)).unwrap()).unwrap(), json!({"zoom": 8}));
        assert_eq!(parse(&std::fs::read(copy_path(&path, 2)).unwrap()).unwrap(), json!({"zoom": 1}));
        assert_eq!(parse(&std::fs::read(&path).unwrap()).unwrap(), json!({"zoom": 20}));
        // The same state again is no change, and no copy.
        let before = std::fs::read(copy_path(&path, 1)).unwrap();
        save_at(&path, &json!({"zoom": 20}), at(60)).unwrap();
        assert_eq!(std::fs::read(copy_path(&path, 1)).unwrap(), before);
        assert!(!copy_path(&path, 3).exists());
        // Never more than COPIES, and no temporary file left behind.
        for step in 0..12 {
            set_mtime(&copy_path(&path, 1), at(100 + step * 40));
            save_at(&path, &json!({"zoom": 100 + step}), at(100 + step * 40 + 20)).unwrap();
        }
        assert!(copy_path(&path, COPIES).exists() && !copy_path(&path, COPIES + 1).exists());
        assert!(!path.with_extension("json.tmp").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unreadable_state_is_kept_and_the_newest_good_copy_comes_back_with_a_warning() {
        let dir = scratch("recover");
        let path = dir.join("state.json");
        // A good state, then a better one: the copy has the first.
        save_at(&path, &json!({"order": ["a"]}), at(0)).unwrap();
        save_at(&path, &json!({"order": ["b"]}), at(20)).unwrap();
        set_mtime(&copy_path(&path, 1), at(20));
        // The state is damaged (a write that stopped halfway, a sync conflict).
        std::fs::write(&path, b"{\"order\": [\"b\"").unwrap();
        let (got, said) = read_state(&path, at(35));
        assert_eq!(got, Some(json!({"order": ["a"]})));
        assert_eq!(parse(&std::fs::read(&path).unwrap()).unwrap(), json!({"order": ["a"]}), "put back as the state");
        assert_eq!(std::fs::read(unreadable_path(&path)).unwrap(), b"{\"order\": [\"b\"", "the bad one is kept");
        let said = said.expect("the person is told");
        assert!(said.contains("ilegível") && said.contains("cópia de há 15 min") && said.contains("state.json.ilegivel"), "{said}");
        // Read again, it is the good one now: nothing more to say.
        assert_eq!(read_state(&path, at(36)), (Some(json!({"order": ["a"]})), None));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn with_no_good_copy_it_starts_from_the_defaults_and_says_so() {
        let dir = scratch("lost");
        let path = dir.join("state.json");
        std::fs::write(&path, b"nao e json").unwrap();
        std::fs::write(copy_path(&path, 1), b"[1, 2]").unwrap();
        let (got, said) = read_state(&path, at(0));
        assert_eq!(got, None);
        let said = said.expect("the person is told");
        assert!(said.contains("começou do zero") && said.contains("state.json.ilegivel"), "{said}");
        assert_eq!(std::fs::read(unreadable_path(&path)).unwrap(), b"nao e json");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_first_run_and_a_good_state_say_nothing() {
        let dir = scratch("quiet");
        let path = dir.join("state.json");
        assert_eq!(read_state(&path, at(0)), (None, None));
        save_at(&path, &json!({"zoom": 1.25}), at(0)).unwrap();
        assert_eq!(read_state(&path, at(1)), (Some(json!({"zoom": 1.25})), None));
        // A byte order mark some tool put in front is no damage.
        std::fs::write(&path, b"\xEF\xBB\xBF{\"zoom\": 2}").unwrap();
        assert_eq!(read_state(&path, at(2)), (Some(json!({"zoom": 2})), None));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_save_never_buries_a_state_it_cannot_read() {
        let dir = scratch("buried");
        let path = dir.join("state.json");
        std::fs::write(&path, b"{ nao fechou").unwrap();
        save_at(&path, &json!({"zoom": 1}), at(0)).unwrap();
        assert_eq!(std::fs::read(unreadable_path(&path)).unwrap(), b"{ nao fechou");
        assert_eq!(parse(&std::fs::read(&path).unwrap()).unwrap(), json!({"zoom": 1}));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn how_old_a_copy_is_said_in_words() {
        let s = Duration::from_secs;
        assert_eq!(age_text(s(20)), "agora há pouco");
        assert_eq!(age_text(s(60)), "há 1 min");
        assert_eq!(age_text(s(59 * 60)), "há 59 min");
        assert_eq!(age_text(s(3 * 3600 + 5)), "há 3 h");
        assert_eq!(age_text(s(2 * 86_400)), "há 2 dias");
    }

    #[test]
    fn the_log_takes_one_line_at_a_time_and_starts_again_when_long() {
        let dir = std::env::temp_dir().join(format!("keep-app-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("app.log");
        append(&path, "um", 64);
        append(&path, "dois\ncom quebra", 64);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "um\ndois com quebra\n");
        append(&path, &"x".repeat(80), 64);
        append(&path, "depois", 64);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "depois\n");
        assert!(std::fs::read_to_string(path.with_extension("log.1")).unwrap().ends_with(&format!("{}\n", "x".repeat(80))));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
