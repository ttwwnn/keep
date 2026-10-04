//! Keep for Windows: the window over the daemon.
//!
//! The daemon (`keepd`) owns every shell; this app shows them. Each command
//! below is one question to the daemon or one attached tab, and the page —
//! sidebar, tab strip, terminals — is in `../src`.

mod daemon;
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
            e2e_report,
        ])
        .build(tauri::generate_context!())
        .expect("erro ao iniciar o Keep")
        .run(move |_app, event| {
            // Leaving lets go of every tab and nothing more: the shells are
            // the daemon's, and they keep running.
            if let tauri::RunEvent::Exit = event {
                on_exit.detach_all();
            }
        });
}
