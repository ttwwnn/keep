//! Keep for Windows: the window over the daemon.
//!
//! The daemon (`keepd`) owns every shell; this app shows them. Each command
//! below is one question to the daemon or one attached tab, and the page —
//! sidebar, tab strip, terminals — is in `../src`.

mod claude_settings;
mod daemon;
mod ia;
mod memory;
mod sessions;
mod settings;
mod status;
mod zoxide;

use std::path::PathBuf;
use std::sync::Arc;

use keep_proto::PaneMove;
use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, State};

use sessions::Sessions;

type Answer<T> = Result<T, String>;

fn text(e: anyhow::Error) -> String {
    format!("{e:#}")
}

fn resources(app: &AppHandle) -> Option<PathBuf> {
    app.path().resource_dir().ok()
}

/// What the page needs to know before it draws anything.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Startup {
    platform: &'static str,
    windows_build: u32,
    home: String,
    address: String,
    version: &'static str,
    /// Set by tests: the page runs its scripted check and reports here.
    e2e_report: Option<String>,
    /// Set by tests: the scripted check covers the AI layer too, against a
    /// stand-in core ("falso") or the real one ("real").
    e2e_ia: Option<String>,
}

#[tauri::command]
fn startup() -> Startup {
    Startup {
        platform: std::env::consts::OS,
        windows_build: settings::windows_build(),
        home: std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_default(),
        address: daemon::address().display().to_string(),
        version: env!("CARGO_PKG_VERSION"),
        e2e_report: std::env::var("KEEP_E2E_REPORT").ok(),
        e2e_ia: std::env::var("KEEP_E2E_IA").ok().filter(|v| !v.is_empty()),
    }
}

/// Make sure the daemon runs. Started here when it does not — the first
/// launch after a reboot — and from then on it belongs to nobody.
#[tauri::command]
async fn ensure_daemon(app: AppHandle) -> Answer<()> {
    let dir = resources(&app);
    tauri::async_runtime::spawn_blocking(move || daemon::ensure(dir.as_deref()))
        .await
        .map_err(|e| e.to_string())?
        .map_err(text)
}

/// Which daemon is answering; see `daemon::identity`.
#[tauri::command]
async fn daemon_identity() -> Answer<Option<String>> {
    tauri::async_runtime::spawn_blocking(daemon::identity).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn list() -> Answer<Vec<daemon::Workspace>> {
    tauri::async_runtime::spawn_blocking(daemon::list).await.map_err(|e| e.to_string())?.map_err(text)
}

#[tauri::command]
async fn new_tab(
    workspace: String,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    split_of: Option<u32>,
    split_dir: Option<u8>,
) -> Answer<u32> {
    tauri::async_runtime::spawn_blocking(move || {
        daemon::new_tab(&workspace, cwd, cols, rows, split_of.unwrap_or(0), split_dir.unwrap_or(0))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(text)
}

#[tauri::command]
async fn close_tab(workspace: String, tab: u32) -> Answer<()> {
    tauri::async_runtime::spawn_blocking(move || daemon::close_tab(&workspace, tab))
        .await
        .map_err(|e| e.to_string())?
        .map_err(text)
}

#[tauri::command]
async fn kill_workspace(workspace: String) -> Answer<()> {
    tauri::async_runtime::spawn_blocking(move || daemon::kill(&workspace))
        .await
        .map_err(|e| e.to_string())?
        .map_err(text)
}

#[tauri::command]
async fn move_tab(workspace: String, tab: u32, to: String) -> Answer<u32> {
    tauri::async_runtime::spawn_blocking(move || daemon::move_tab(&workspace, tab, &to))
        .await
        .map_err(|e| e.to_string())?
        .map_err(text)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Move {
    tab: u32,
    split_of: u32,
    split_dir: u8,
}

#[tauri::command]
async fn rearrange(workspace: String, moves: Vec<Move>) -> Answer<()> {
    let moves: Vec<PaneMove> =
        moves.into_iter().map(|m| PaneMove { tab: m.tab, split_of: m.split_of, split_dir: m.split_dir }).collect();
    tauri::async_runtime::spawn_blocking(move || daemon::rearrange(&workspace, moves))
        .await
        .map_err(|e| e.to_string())?
        .map_err(text)
}

#[tauri::command]
async fn preview(workspace: String, tab: u32) -> Answer<String> {
    tauri::async_runtime::spawn_blocking(move || daemon::preview(&workspace, tab))
        .await
        .map_err(|e| e.to_string())?
        .map_err(text)
}

/// A tab's screen with its colours (the sequences the daemon repaints with),
/// and the grid it is held on.
#[tauri::command]
async fn preview_vt(workspace: String, tab: u32) -> Answer<daemon::Screen> {
    tauri::async_runtime::spawn_blocking(move || daemon::preview_vt(&workspace, tab))
        .await
        .map_err(|e| e.to_string())?
        .map_err(text)
}

/// The folders the person goes to, from zoxide; none where it is not installed.
#[tauri::command]
async fn directories() -> Answer<Vec<String>> {
    tauri::async_runtime::spawn_blocking(zoxide::directories).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn search(query: String, limit: u32, workspace: Option<String>, tab: Option<u32>) -> Answer<Vec<daemon::Hit>> {
    tauri::async_runtime::spawn_blocking(move || {
        daemon::search(&query, limit, workspace.as_deref().unwrap_or(""), tab.unwrap_or(0))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(text)
}

/// Attach to a tab. What it writes arrives on `channel` (see `sessions::kind`);
/// the returned handle is for `input`, `resize` and `detach`.
#[tauri::command]
fn attach(
    sessions: State<'_, Arc<Sessions>>,
    workspace: String,
    tab: u32,
    cols: u16,
    rows: u16,
    channel: Channel<InvokeResponseBody>,
) -> Answer<u64> {
    sessions.inner().attach(workspace, tab, cols, rows, channel).map_err(text)
}

#[tauri::command]
fn input(sessions: State<'_, Arc<Sessions>>, handle: u64, data: String) -> Answer<()> {
    sessions.input(handle, data.as_bytes()).map_err(text)
}

/// Input that is bytes rather than text: mouse reports in the X10 encoding.
#[tauri::command]
fn input_bytes(sessions: State<'_, Arc<Sessions>>, handle: u64, data: Vec<u8>) -> Answer<()> {
    sessions.input(handle, &data).map_err(text)
}

#[tauri::command]
fn resize(sessions: State<'_, Arc<Sessions>>, handle: u64, cols: u16, rows: u16) -> Answer<()> {
    sessions.resize(handle, cols, rows).map_err(text)
}

#[tauri::command]
fn detach(sessions: State<'_, Arc<Sessions>>, handle: u64) {
    sessions.detach(handle);
}

fn state_path(app: &AppHandle) -> Answer<PathBuf> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(settings::state_file(&dir))
}

#[tauri::command]
fn load_state(app: AppHandle) -> Answer<Option<serde_json::Value>> {
    Ok(settings::load(&state_path(&app)?))
}

#[tauri::command]
fn save_state(app: AppHandle, state: serde_json::Value) -> Answer<()> {
    settings::save(&state_path(&app)?, &state).map_err(text)
}

#[derive(Serialize)]
struct ShellChoice {
    chosen: String,
    available: Vec<settings::Shell>,
}

#[tauri::command]
fn shells() -> ShellChoice {
    ShellChoice { chosen: settings::chosen_shell(), available: settings::shells() }
}

#[tauri::command]
fn choose_shell(command: String) -> Answer<()> {
    settings::choose_shell(&command).map_err(text)
}

/// Whether the core is here to ask about AI accounts and worktrees.
/// The computer's memory in use; none where it cannot be read.
#[tauri::command]
fn memory_now() -> Option<memory::Memory> {
    memory::now()
}

#[tauri::command]
fn memory_monitor() {
    memory::open_monitor();
}

#[tauri::command]
fn ia_info(app: AppHandle) -> ia::Info {
    ia::info(resources(&app).as_deref())
}

/// One question to the core (`keep ia …`, `keep worktrees …`), answered with
/// what it printed; see `ia`.
#[tauri::command]
async fn ia(app: AppHandle, args: Vec<String>) -> Answer<serde_json::Value> {
    let dir = resources(&app);
    tauri::async_runtime::spawn_blocking(move || ia::ask(dir.as_deref(), &daemon::address(), &args))
        .await
        .map_err(|e| e.to_string())?
}

/// The login files as they stand, to notice a new one without the network.
#[tauri::command]
async fn ia_signature() -> Answer<String> {
    tauri::async_runtime::spawn_blocking(ia::signature).await.map_err(|e| e.to_string())
}

/// Send the worktrees a closed tab left to the Recycle Bin; what did not go.
#[tauri::command]
async fn ia_trash(app: AppHandle, items: Vec<ia::Going>, pids: Vec<u32>) -> Answer<Vec<String>> {
    let dir = resources(&app);
    tauri::async_runtime::spawn_blocking(move || ia::trash(dir.as_deref(), &daemon::address(), items, pids))
        .await
        .map_err(|e| e.to_string())
}

/// The worktrees of tabs that closed without the question, to the Recycle
/// Bin by the core's say; what did not go.
#[tauri::command]
async fn ia_sweep(app: AppHandle) -> Answer<Vec<String>> {
    let dir = resources(&app);
    tauri::async_runtime::spawn_blocking(move || ia::sweep(dir.as_deref(), &daemon::address()))
        .await
        .map_err(|e| e.to_string())
}

/// What Claude Code's status line saved for each of these shells (the model
/// and the effort it runs with): the JSON text, by shell pid; see `status`.
#[tauri::command]
async fn claude_statuses(shells: Vec<u32>) -> Answer<std::collections::HashMap<u32, String>> {
    tauri::async_runtime::spawn_blocking(move || {
        status::directory(|name| std::env::var(name).ok())
            .map(|dir| status::read(&dir, &shells))
            .unwrap_or_default()
    })
    .await
    .map_err(|e| e.to_string())
}

/// Claude Code's settings file as it stands, and the status line script this
/// app carries: what the page needs to set the status line up and to put
/// back a default `/model <id>` saved; see `claude_settings`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeSettings {
    path: String,
    /// The file's text; `None` when there is no file.
    text: Option<String>,
    /// Whether Claude Code's folder is there: where it is not, Claude Code
    /// has not run here, and no file is made for it.
    folder: bool,
    /// The `keep-statusline.ps1` installed with the app, when it is there.
    script: Option<String>,
}

#[tauri::command]
async fn claude_settings(app: AppHandle) -> Answer<ClaudeSettings> {
    let resources = resources(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let dir = claude_settings::directory(|name| std::env::var(name).ok())
            .ok_or_else(|| "sem pasta do usuário".to_string())?;
        let path = dir.join("settings.json");
        let text = claude_settings::read(&path).map_err(crate::text)?;
        let script = resources
            .map(|r| r.join("keep-statusline.ps1"))
            .filter(|p| p.is_file())
            .map(|p| claude_settings::plain(&p));
        Ok(ClaudeSettings { path: claude_settings::plain(&path), text, folder: dir.is_dir(), script })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Write Claude Code's settings file, if it still holds what the page read
/// (`expected`; `None` for no file). False when it changed meanwhile.
#[tauri::command]
async fn write_claude_settings(expected: Option<String>, text: String) -> Answer<bool> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = claude_settings::directory(|name| std::env::var(name).ok())
            .ok_or_else(|| "sem pasta do usuário".to_string())?;
        claude_settings::replace(&dir.join("settings.json"), expected.as_deref(), &text).map_err(crate::text)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Codex's model cache, when it is not the version the page has.
#[derive(Serialize)]
struct CodexModels {
    /// Which version is there; `None` when there is no cache.
    stamp: Option<String>,
    /// Its text, when the version changed.
    text: Option<String>,
}

#[tauri::command]
async fn codex_models(known: Option<String>) -> Answer<CodexModels> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(path) = status::codex_cache(|name| std::env::var(name).ok()) else {
            return CodexModels { stamp: None, text: None };
        };
        let (stamp, text) = status::if_changed(&path, known.as_deref());
        CodexModels { stamp, text }
    })
    .await
    .map_err(|e| e.to_string())
}

/// A line for the app's log (`settings::log_file`): what the page did that
/// nobody sees, such as a status line left alone or a picker backed out of.
#[tauri::command]
async fn app_log(line: String) {
    let _ = tauri::async_runtime::spawn_blocking(move || settings::log(&line)).await;
}

/// For the scripted check: whether a path is still there.
#[tauri::command]
fn path_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}

/// For the scripted check: have the test photograph the screen now, as
/// `<name>.png` beside the report, and wait until it has.
#[tauri::command]
async fn e2e_shot(name: String) -> Answer<()> {
    let report = std::env::var("KEEP_E2E_REPORT").map_err(|_| "sem KEEP_E2E_REPORT".to_string())?;
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(format!("nome de tela inválido: {name}"));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let dir = std::path::Path::new(&report).parent().map(|d| d.join("telas")).unwrap_or_default();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let picture = dir.join(format!("{name}.png"));
        let _ = std::fs::remove_file(&picture);
        std::fs::write(dir.join(format!("{name}.pedido")), b"").map_err(|e| e.to_string())?;
        let until = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while std::time::Instant::now() < until {
            if picture.is_file() {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        Err(format!("a tela {name} não foi fotografada em 15 s"))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// For the scripted check: what the stand-in core answers about the tabs.
#[tauri::command]
fn e2e_fake(state: serde_json::Value) -> Answer<()> {
    let path = std::env::var("KEEP_E2E_FAKE_STATE").map_err(|_| "sem KEEP_E2E_FAKE_STATE".to_string())?;
    let mut current: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let (Some(into), Some(from)) = (current.as_object_mut(), state.as_object()) {
        for (k, v) in from {
            into.insert(k.clone(), v.clone());
        }
    }
    std::fs::write(&path, serde_json::to_vec_pretty(&current).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Where a scripted run writes what it found, for the test that started it.
#[tauri::command]
fn e2e_report(report: String) -> Answer<()> {
    let path = std::env::var("KEEP_E2E_REPORT").map_err(|_| "sem KEEP_E2E_REPORT".to_string())?;
    std::fs::write(path, report).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let sessions = Arc::new(Sessions::default());
    let on_exit = Arc::clone(&sessions);
    tauri::Builder::default()
        // A second launch brings the window that is already open forward,
        // rather than a second viewer of the same daemon.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED,
                )
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(sessions)
        .invoke_handler(tauri::generate_handler![
            startup,
            ensure_daemon,
            daemon_identity,
            list,
            new_tab,
            close_tab,
            kill_workspace,
            move_tab,
            rearrange,
            preview,
            preview_vt,
            directories,
            search,
            attach,
            input,
            input_bytes,
            resize,
            detach,
            load_state,
            save_state,
            shells,
            choose_shell,
            ia_info,
            memory_now,
            memory_monitor,
            ia,
            ia_signature,
            ia_trash,
            ia_sweep,
            claude_statuses,
            claude_settings,
            write_claude_settings,
            codex_models,
            app_log,
            path_exists,
            e2e_shot,
            e2e_fake,
            e2e_report,
        ])
        .build(tauri::generate_context!())
        .expect("erro ao iniciar o Keep")
        .run(move |app, event| match event {
            // Worktrees on their way to the Recycle Bin: the window may go,
            // the app waits for them. A folder half dealt with and nobody
            // told is worse than a process that lingers a few seconds.
            tauri::RunEvent::ExitRequested { api, .. } if ia::trashing() > 0 => {
                api.prevent_exit();
                let app = app.clone();
                std::thread::spawn(move || {
                    let until = std::time::Instant::now() + std::time::Duration::from_secs(180);
                    while ia::trashing() > 0 && std::time::Instant::now() < until {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    }
                    app.exit(0);
                });
            }
            // Leaving lets go of every tab and nothing more: the shells are
            // the daemon's, and they keep running.
            tauri::RunEvent::Exit => on_exit.detach_all(),
            _ => {}
        });
}
