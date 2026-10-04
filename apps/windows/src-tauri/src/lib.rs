//! Keep for Windows: the window over the daemon.
//!
//! The daemon (`keepd`) owns every shell; this app shows them. Each command
//! below is one question to the daemon or one attached tab, and the page —
//! sidebar, tab strip, terminals — is in `../src`.

mod daemon;
mod ia;
mod sessions;
mod settings;

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
            ia,
            ia_signature,
            ia_trash,
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
