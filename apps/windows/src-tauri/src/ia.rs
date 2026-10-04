//! The AI layer the footer, the tabs' menus and the close question use —
//! accounts and their usage, each tab's AI, the worktrees a closed tab leaves
//! behind — asked of `keep.exe`, the core's command line that goes in the
//! installer (`keep ia …`, `keep worktrees …`; docs/ia.md). Every question is
//! answered with one JSON object.
//!
//! One place runs it: off the window's thread, with no console window of its
//! own, told which daemon the app uses, and stopped when it outlives its time
//! — an answer held up by a hung helper would be a button that does nothing.
//! The page gets the answer as it came, with `"_saida"` added: the exit code,
//! which tells "refused" (1) from "asked wrong" (2), and 2 is what a core
//! that predates a command answers.
//!
//! The move to the Recycle Bin lives here too, with the steps around it
//! (`trash`): it must finish even when the window closes halfway, so it does
//! not depend on the page, and the app does not quit while it runs.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The core's command line: `KEEP_IA_BIN` when set — the only candidate
/// then, so a test that points it at nothing gets no AI layer rather than
/// the installed one — else the copy installed with the app, else one beside
/// the app's executable.
pub fn cli(resources: Option<&Path>) -> Option<PathBuf> {
    if let Some(named) = std::env::var_os("KEEP_IA_BIN") {
        let path = PathBuf::from(named);
        return path.is_file().then_some(path);
    }
    let name = format!("keep{}", std::env::consts::EXE_SUFFIX);
    let mut candidates = Vec::new();
    if let Some(dir) = resources {
        candidates.push(dir.join("bin").join(&name));
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        candidates.push(dir.join("bin").join(&name));
        candidates.push(dir.join(&name));
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// What the page needs to know before it shows any of this.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    /// The core is here to be asked: without it there is no footer, no menu
    /// and no worktree in the close question.
    pub available: bool,
    /// A test pointed the core at a made-up home: nothing is kept between
    /// runs, as the macOS app keeps nothing under `KEEP_AI_USAGE_HOME`.
    pub test_home: bool,
    /// The home the logins are read from, for writing paths with "~".
    pub home: String,
}

pub fn info(resources: Option<&Path>) -> Info {
    let test = std::env::var("KEEP_IA_HOME").ok().filter(|h| !h.is_empty());
    Info { available: cli(resources).is_some(), test_home: test.is_some(), home: home().to_string_lossy().into_owned() }
}

fn home() -> PathBuf {
    for var in ["KEEP_IA_HOME", "USERPROFILE", "HOME"] {
        if let Some(value) = std::env::var_os(var).filter(|v| !v.is_empty()) {
            return PathBuf::from(value);
        }
    }
    PathBuf::new()
}

/// How long each question may take: the contract's deadlines (docs/ia.md),
/// which are the macOS app's. `None` for a question the page may not ask —
/// the interactive login runs in a tab, never from here, and the two steps of
/// a move to the Recycle Bin only inside `trash`.
pub fn deadline(args: &[String]) -> Option<Duration> {
    let word = |i: usize| args.get(i).map(String::as_str).unwrap_or("");
    let seconds = match (word(0), word(1)) {
        ("ia", "contas") => 30.0,
        ("ia", "uso") => 30.0,
        ("ia", "ordem") if word(2) == "mover" => 10.0,
        ("ia", "ordem") => 5.0,
        ("ia", "abas") => 5.0,
        ("ia", "trocar") => 40.0,
        ("ia", "entrar") => 20.0,
        ("ia", "sincronizar") => 60.0,
        ("worktrees", "listar") => {
            let asked = args
                .iter()
                .position(|a| a == "--prazo")
                .and_then(|i| args.get(i + 1))
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(3.0);
            asked.clamp(0.5, 30.0) + 1.5
        }
        ("worktrees", "indexar") => 120.0,
        _ => return None,
    };
    Some(Duration::from_secs_f64(seconds))
}

/// Ask the core, as the page asked it.
pub fn ask(resources: Option<&Path>, daemon: &Path, args: &[String]) -> Result<Value, String> {
    let within = deadline(args).ok_or_else(|| format!("pedido que o app não faz: {}", args.join(" ")))?;
    let mut answer = run(resources, daemon, args, within)?;
    // A worktree's birth is in nanoseconds since 1970, past what a
    // JavaScript number holds exactly: it travels as text, and comes back to
    // `trash` as the same text.
    if args.first().map(String::as_str) == Some("worktrees") {
        if let Some(items) = answer.get_mut("lixeira").and_then(Value::as_array_mut) {
            for item in items {
                if let Some(born) = item.get("nasceu_ns").filter(|v| v.is_number()).map(|v| v.to_string()) {
                    item["nasceu_ns"] = Value::String(born);
                }
            }
        }
    }
    Ok(answer)
}

/// Run the core with `args` for at most `within`.
fn run(resources: Option<&Path>, daemon: &Path, args: &[String], within: Duration) -> Result<Value, String> {
    let exe = cli(resources).ok_or_else(|| "o keep.exe não está junto do app".to_string())?;
    let mut cmd = Command::new(&exe);
    cmd.args(args).env("KEEP_SOCKET", daemon).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| format!("o keep não abriu: {e}"))?;
    // Read while it runs, so a long answer cannot fill the pipe and stall it.
    let mut out = child.stdout.take().ok_or_else(|| "o keep não abriu a saída".to_string())?;
    let reader = std::thread::spawn(move || {
        let mut data = Vec::new();
        let _ = out.read_to_end(&mut data);
        data
    });
    let until = Instant::now() + within;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("o keep passou de {} s", within.as_secs_f64().round()));
            }
            Err(e) => return Err(format!("o keep não respondeu: {e}")),
        }
    };
    let data = reader.join().unwrap_or_default();
    parse(&data, status.code().unwrap_or(-1))
}

/// The object the core printed, with its exit code in `"_saida"`.
pub fn parse(data: &[u8], code: i32) -> Result<Value, String> {
    let text = String::from_utf8_lossy(data);
    let whole = serde_json::from_str::<Value>(text.trim()).ok().filter(Value::is_object);
    let answer = whole.or_else(|| {
        text.lines()
            .rev()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .find_map(|l| serde_json::from_str::<Value>(l).ok().filter(Value::is_object))
    });
    let Some(mut answer) = answer else {
        return Err(if code == 2 {
            "o keep não entendeu o pedido".to_string()
        } else {
            format!("o keep respondeu algo que não dá para ler (saiu com {code})")
        });
    };
    if answer.get("versao").and_then(Value::as_i64) != Some(1) {
        return Err(format!("o keep respondeu noutra versão do contrato (saiu com {code})"));
    }
    if let Some(object) = answer.as_object_mut() {
        object.insert("_saida".into(), Value::from(code));
    }
    Ok(answer)
}

// ------------------------------------------------------------------ logins

/// The files the logins and the order live in, as a string that changes when
/// any of them does: a glance every two seconds that reads nothing over the
/// network, so that a login just made shows at once (the macOS footer's
/// `AIAccounts.signature`).
pub fn signature() -> String {
    let home = home();
    let claude = home.join(".claude");
    let accounts = claude.join("contas");
    let mut files = vec![
        claude.join(".credentials.json"),
        accounts.join(".ordem"),
        accounts.join(".ativa"),
        home.join(".codex").join("auth.json"),
    ];
    let children = |dir: &Path| -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|entries| entries.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        found.sort();
        found
    };
    for path in children(&accounts) {
        if path.extension().is_some_and(|e| e == "json") {
            files.push(path);
        }
    }
    for dir in children(&accounts.join("fixas")) {
        for name in ["keep.json", "conta.json", ".credentials.json"] {
            files.push(dir.join(name));
        }
    }
    for dir in children(&home.join(".codex-contas")) {
        files.push(dir.join("auth.json"));
    }
    let mut out = String::new();
    for file in files {
        if let Ok(meta) = std::fs::metadata(&file) {
            let modified = meta
                .modified()
                .ok()
                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_millis());
            out.push_str(&format!("{}|{}|{};", file.display(), meta.len(), modified));
        }
    }
    out
}

// ------------------------------------------------------------------ the Recycle Bin

/// One folder the question showed, to go to the Recycle Bin.
#[derive(Deserialize)]
pub struct Going {
    pub caminho: String,
    /// The birth the listing showed, as text (see `ask`): another worktree
    /// made at the same path while the question was up has another, and the
    /// yes was not for it.
    #[serde(default)]
    pub nasceu_ns: Option<String>,
}

static TRASHING: AtomicUsize = AtomicUsize::new(0);

/// Moves still under way: the app waits for them before it quits.
pub fn trashing() -> usize {
    TRASHING.load(Ordering::SeqCst)
}

struct Busy;

impl Busy {
    fn start() -> Busy {
        TRASHING.fetch_add(1, Ordering::SeqCst);
        Busy
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        TRASHING.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Send the listed folders to the Recycle Bin once the tabs' processes are
/// gone; what did not go, one line each, empty when everything did.
///
/// Only what the question showed: consent was given for that list. Each
/// folder is prepared by the core first — its HEAD, and any change not yet
/// committed, saved in `refs/keep-lixeira` so that no commit depends on the
/// folder surviving the Bin — then moved here, then its registration in the
/// repository dropped by the core, so its branch is free again. A folder
/// something still runs in is tried again for a minute, the time a test
/// runner the conversation left behind takes to notice; then it stays, and
/// says so. The macOS app's `Worktrees.trash`, step for step.
pub fn trash(resources: Option<&Path>, daemon: &Path, items: Vec<Going>, pids: Vec<u32>) -> Vec<String> {
    let _busy = Busy::start();
    if items.is_empty() {
        return Vec::new();
    }
    let home = home();
    // The tabs' own processes gone, or nothing moves: a conversation still
    // running is still using its worktrees, whatever the close reported.
    let running = wait_for_exit(&pids, Duration::from_secs(20));
    if !running.is_empty() {
        let pids = running.iter().map(u32::to_string).collect::<Vec<_>>().join(", ");
        return items
            .iter()
            .map(|i| format!("{}: a conversa da aba ainda está rodando (pid {pids}); nada foi movido", tilde(&i.caminho, &home)))
            .collect();
    }
    let mut problems = Vec::new();
    for item in items {
        let place = tilde(&item.caminho, &home);
        let mut reason = "sem resposta do keep".to_string();
        let give_up = Instant::now() + Duration::from_secs(60);
        let ready = loop {
            let mut args = vec!["worktrees".to_string(), "preparar".into(), item.caminho.clone()];
            if let Some(born) = item.nasceu_ns.as_ref().filter(|b| !b.is_empty()) {
                args.extend(["--nasceu".to_string(), born.clone()]);
            }
            match run(resources, daemon, &args, Duration::from_secs(30)) {
                Err(why) => {
                    reason = why;
                    break false;
                }
                Ok(answer) if answer.get("ok").and_then(Value::as_bool) == Some(true) => break true,
                Ok(answer) => {
                    reason = text(&answer, "motivo").unwrap_or_else(|| "recusada pelo keep".into());
                    // Only something still running is worth waiting out.
                    let busy = answer.get("processos").and_then(Value::as_array).is_some_and(|p| !p.is_empty());
                    if busy && Instant::now() < give_up {
                        std::thread::sleep(Duration::from_secs(3));
                        continue;
                    }
                    break false;
                }
            }
        };
        if !ready {
            problems.push(format!("{place}: {reason}"));
            continue;
        }
        let destination = match to_recycle_bin(Path::new(&item.caminho)) {
            Ok(destination) => destination,
            Err(why) => {
                problems.push(format!("{place}: {why}"));
                continue;
            }
        };
        let args = vec![
            "worktrees".to_string(),
            "concluir".into(),
            item.caminho.clone(),
            destination.to_string_lossy().into_owned(),
        ];
        match run(resources, daemon, &args, Duration::from_secs(30)) {
            Err(why) => problems.push(format!("{place} foi para a Lixeira, mas o git ainda a registra: {why}")),
            Ok(answer) if answer.get("ok").and_then(Value::as_bool) != Some(true) => {
                let why = text(&answer, "motivo").unwrap_or_else(|| "recusado".into());
                problems.push(format!("{place} foi para a Lixeira, mas o git ainda a registra: {why}"));
            }
            Ok(_) => {}
        }
    }
    problems
}

fn text(answer: &Value, key: &str) -> Option<String> {
    answer.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

/// A path under the home, written from "~".
pub fn tilde(path: &str, home: &Path) -> String {
    let home = home.to_string_lossy();
    let home = home.trim_end_matches(['\\', '/']);
    if home.is_empty() {
        return path.to_string();
    }
    // Windows paths are compared as Windows compares them: either slash, any case.
    let same = |a: &str, b: &str| a.replace('/', "\\").to_lowercase() == b.replace('/', "\\").to_lowercase();
    if same(path, home) {
        return "~".into();
    }
    match (path.get(..home.len()), path.get(home.len()..)) {
        (Some(head), Some(rest)) if same(head, home) && rest.starts_with(['\\', '/']) => format!("~{rest}"),
        _ => path.to_string(),
    }
}

/// Until every one of these processes is gone, or the time is up; the ones
/// still running then.
pub fn wait_for_exit(pids: &[u32], upto: Duration) -> Vec<u32> {
    let until = Instant::now() + upto;
    let mut running = Vec::new();
    for &pid in pids {
        if pid == 0 {
            continue;
        }
        if !exits_by(pid, until) {
            running.push(pid);
        }
    }
    running
}

#[cfg(windows)]
fn exits_by(pid: u32, until: Instant) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};
    // SAFETY: a handle opened here, waited on and closed here.
    unsafe {
        let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if process.is_null() {
            // Gone already: there is no process of that number to open.
            return true;
        }
        let left = until.saturating_duration_since(Instant::now()).as_millis().min(u128::from(u32::MAX - 1)) as u32;
        let waited = WaitForSingleObject(process, left);
        CloseHandle(process);
        waited == WAIT_OBJECT_0
    }
}

#[cfg(not(windows))]
fn exits_by(pid: u32, until: Instant) -> bool {
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    loop {
        // SAFETY: signal 0 asks whether the process exists and sends nothing.
        if unsafe { kill(pid as i32, 0) } != 0 {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Move a folder to the Recycle Bin, as Explorer's Delete does, and say where
/// it went there. A folder too big for the Bin is not deleted for good
/// behind the person's back: Windows asks first (`FOF_WANTNUKEWARNING`), and
/// a no is a folder that stays.
#[cfg(windows)]
pub fn to_recycle_bin(path: &Path) -> Result<PathBuf, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
    use windows_sys::Win32::UI::Shell::{
        FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FOF_WANTNUKEWARNING,
        SHFILEOPSTRUCTW, SHFileOperationW,
    };
    let full = std::path::absolute(path).map_err(|e| format!("caminho ilegível: {e}"))?;
    if !full.is_dir() {
        return Err("a pasta não existe mais".into());
    }
    // The name the Bin will record is the long one: a path given in 8.3
    // (`C:\Users\RUNNER~1\…`, which TEMP often is) is looked for by both.
    let long = std::fs::canonicalize(&full).ok().map(|p| {
        let text = p.to_string_lossy().into_owned();
        PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text))
    });
    // A list of paths, each ended by a NUL, the list by another.
    let mut from: Vec<u16> = full.as_os_str().encode_wide().collect();
    from.extend([0, 0]);
    let started = std::time::SystemTime::now();
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: from.as_ptr(),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI | FOF_WANTNUKEWARNING) as u16,
        ..Default::default()
    };
    // SAFETY: the shell's own operation over a list that outlives the call;
    // COM is entered and left on this thread around it.
    let (code, aborted) = unsafe {
        let entered = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        let code = SHFileOperationW(&mut op);
        if entered >= 0 {
            CoUninitialize();
        }
        (code, op.fAnyOperationsAborted != 0)
    };
    if code != 0 {
        return Err(format!("o Windows não moveu a pasta (código {code:#x})"));
    }
    if aborted || full.exists() {
        return Err("a pasta continua no lugar".into());
    }
    let names: Vec<&Path> = std::iter::once(full.as_path()).chain(long.as_deref()).collect();
    Ok(find_in_recycle_bin(&names, started).unwrap_or_else(|| recycle_bin_of(&full)))
}

#[cfg(not(windows))]
pub fn to_recycle_bin(_path: &Path) -> Result<PathBuf, String> {
    Err("a Lixeira do Windows só existe no Windows".into())
}

/// The Recycle Bin of the drive a path is on.
#[cfg_attr(not(windows), allow(dead_code))]
fn recycle_bin_of(path: &Path) -> PathBuf {
    let root: PathBuf = path.components().take_while(|c| !matches!(c, std::path::Component::Normal(_))).collect();
    root.join("$Recycle.Bin")
}

/// Where in the Recycle Bin a folder just sent there is: the `$R…` item whose
/// `$I…` twin records its original path, the newest of them. The Bin keeps
/// one folder per user (their SID) on each drive; only one's own can be read.
#[cfg_attr(not(windows), allow(dead_code))]
fn find_in_recycle_bin(names: &[&Path], since: std::time::SystemTime) -> Option<PathBuf> {
    let fold = |p: &str| p.replace('/', "\\").trim_end_matches('\\').to_lowercase();
    let wanted: Vec<String> = names.iter().map(|p| fold(&p.to_string_lossy())).collect();
    let earliest = since.checked_sub(Duration::from_secs(5)).unwrap_or(since);
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for user in std::fs::read_dir(recycle_bin_of(names.first()?)).ok()?.flatten() {
        let Ok(entries) = std::fs::read_dir(user.path()) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(rest) = name.strip_prefix("$I") else { continue };
            let Some(modified) = entry.metadata().ok().and_then(|m| m.modified().ok()) else { continue };
            if modified < earliest {
                continue;
            }
            let Ok(data) = std::fs::read(entry.path()) else { continue };
            let Some(recorded) = recorded_path(&data) else { continue };
            if !wanted.contains(&fold(&recorded)) {
                continue;
            }
            let item = entry.path().with_file_name(format!("$R{rest}"));
            if item.exists() && best.as_ref().is_none_or(|(t, _)| modified > *t) {
                best = Some((modified, item));
            }
        }
    }
    best.map(|(_, item)| item)
}

/// The original path a `$I…` file records: version 2 (Windows 10 on) has its
/// length before it, version 1 a fixed 260 characters.
pub fn recorded_path(data: &[u8]) -> Option<String> {
    if data.len() < 28 {
        return None;
    }
    let version = i64::from_le_bytes(data[0..8].try_into().ok()?);
    let (start, chars) = match version {
        2 => (28, u32::from_le_bytes(data[24..28].try_into().ok()?) as usize),
        1 => (24, 260),
        _ => return None,
    };
    let units: Vec<u16> = data
        .get(start..)?
        .chunks_exact(2)
        .take(chars)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    (!units.is_empty()).then(|| String::from_utf16_lossy(&units))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_carries_its_exit_code() {
        let answer = parse(b"{\"versao\":1,\"ok\":false,\"motivo\":\"ocupada\"}\n", 1).unwrap();
        assert_eq!(answer["_saida"], 1);
        assert_eq!(answer["motivo"], "ocupada");
        // Pretty-printed, or after a line of something else.
        assert!(parse(b"aviso\n{\"versao\":1,\"ok\":true}\n", 0).is_ok());
        assert!(parse(b"{\n  \"versao\": 1,\n  \"ok\": true\n}\n", 0).is_ok());
        assert_eq!(parse(b"uso: keep ...", 2).unwrap_err(), "o keep não entendeu o pedido");
        assert!(parse(b"{\"versao\":2}", 0).is_err());
    }

    #[test]
    fn only_the_questions_the_page_asks_have_a_deadline() {
        let args = |s: &str| s.split(' ').map(String::from).collect::<Vec<_>>();
        assert_eq!(deadline(&args("ia trocar --ws=a --aba=1 --para=gpt:x --json")), Some(Duration::from_secs(40)));
        assert_eq!(deadline(&args("ia ordem mover gpt:x cima --json")), Some(Duration::from_secs(10)));
        assert_eq!(deadline(&args("worktrees listar --prazo 3.0 a:1")), Some(Duration::from_secs_f64(4.5)));
        assert_eq!(deadline(&args("ia login claude")), None);
        assert_eq!(deadline(&args("worktrees preparar C:\\x")), None);
    }

    #[test]
    fn the_recycle_bin_record_says_where_a_folder_came_from() {
        let path = "C:\\Users\\ana\\repo-wt";
        let mut v2 = Vec::new();
        v2.extend(2i64.to_le_bytes());
        v2.extend(4096i64.to_le_bytes());
        v2.extend(0u64.to_le_bytes());
        let wide: Vec<u16> = path.encode_utf16().chain([0]).collect();
        v2.extend((wide.len() as u32).to_le_bytes());
        for u in &wide {
            v2.extend(u.to_le_bytes());
        }
        assert_eq!(recorded_path(&v2).as_deref(), Some(path));
        let mut v1 = Vec::new();
        v1.extend(1i64.to_le_bytes());
        v1.extend(4096i64.to_le_bytes());
        v1.extend(0u64.to_le_bytes());
        let mut fixed = wide.clone();
        fixed.resize(260, 0);
        for u in &fixed {
            v1.extend(u.to_le_bytes());
        }
        assert_eq!(recorded_path(&v1).as_deref(), Some(path));
        assert_eq!(recorded_path(b"short"), None);
    }

    #[test]
    fn a_path_under_home_is_written_from_the_tilde() {
        let home = Path::new("C:\\Users\\Ana");
        assert_eq!(tilde("C:\\Users\\ana\\projetos\\wt-x", home), "~\\projetos\\wt-x");
        assert_eq!(tilde("C:\\Users\\Ana", home), "~");
        assert_eq!(tilde("D:\\outro", home), "D:\\outro");
        assert_eq!(tilde("C:\\Users\\Anabela\\x", home), "C:\\Users\\Anabela\\x");
    }
}
