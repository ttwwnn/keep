//! Several windows over one daemon: the windows the app has, what each lists and
//! shows, what they share and what each keeps for itself — and the `state.json`
//! all of it is written to.
//!
//! Each window is a webview with a page of its own that knows nothing of the
//! others, as each window of the macOS app is one `MainWindowController` over
//! the one `Session`. What the pages must agree on lives here, once:
//!
//!  - the app's part of the state (the names given, the order of the tabs, the
//!    workspaces put away, the zoom and the look), which a page changes by
//!    sending the changes (`shared_apply`) and everybody hears, in the order
//!    they were made, the one that made them too;
//!  - each window's own part (its list of workspaces, what is folded, the tab in
//!    front), which only that window writes (`window_save`), and its frame,
//!    which is read from the window itself as it moves;
//!  - the list of the windows that are open, oldest first, each with what it
//!    says of itself — the workspaces it lists, the tab in front — for the ⧉ on
//!    a tab shown elsewhere and for deciding who takes a workspace nobody lists.
//!
//! Everything is read, changed and written under one lock, so that two windows
//! saving at once do not step on each other, and state.json keeps the rotating
//! copies and the recovery of `settings`.
//!
//! The page decides what the lists mean (src/windowModel.ts); this decides only
//! what must not be decided twice: which label a new window gets, the changes
//! that make a difference, and what a window that closes leaves behind.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Window, WindowEvent};

use crate::settings;

type Answer<T> = Result<T, String>;

/// Told to every window when the app's part of the state changes: `{seq, from, ops}`.
const EVENT_SHARED: &str = "keep://shared";
/// Told to every window when a window opens or closes or says something of itself: the list of windows.
const EVENT_WINDOWS: &str = "keep://windows";

const MAIN: &str = "main";

/// The fields of the state every window shares (src/windowState.ts: `SHARED_FIELDS`).
const SHARED: [&str; 11] = [
    "tabNames",
    "workspaceNames",
    "tabOrder",
    "lastTab",
    "hidden",
    "daemon",
    "zoom",
    "appearance",
    "font",
    "theme",
    "terminal",
];

/// The fields each window keeps for itself (`WINDOW_FIELDS`).
const OWN: [&str; 5] = ["order", "folded", "verticalTabs", "sidebar", "active"];

// ------------------------------------------------------------------ which window

/// The slot a label names: `main` is 0, `w1`, `w2`… the rest; none for any other window (the quick terminal's).
pub fn slot_of(label: &str) -> Option<u32> {
    if label == MAIN {
        return Some(0);
    }
    let digits = label.strip_prefix('w')?;
    if digits.is_empty() || digits.len() > 6 || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

pub fn is_app_window(label: &str) -> bool {
    slot_of(label).is_some()
}

pub fn label_of(slot: u32) -> String {
    if slot == 0 {
        MAIN.to_string()
    } else {
        format!("w{slot}")
    }
}

/// The lowest slot nobody is using, so that a closed window's furniture goes to the next.
pub fn free_label<'a>(taken: impl IntoIterator<Item = &'a str>) -> String {
    let slots: HashSet<u32> = taken.into_iter().filter_map(slot_of).collect();
    let mut slot = 0;
    while slots.contains(&slot) {
        slot += 1;
    }
    label_of(slot)
}

// ------------------------------------------------------------------ the state, as it is written

/// What state.json holds: the app's part, and a record per window.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Doc {
    pub shared: Map<String, Value>,
    pub windows: BTreeMap<String, Value>,
}

fn pick(from: &Map<String, Value>, keys: &[&str]) -> Map<String, Value> {
    keys.iter().filter_map(|k| from.get(*k).map(|v| ((*k).to_string(), v.clone()))).collect()
}

impl Doc {
    /// The state as it was read: the document of this version, or the one the app wrote while it had a window —
    /// all in one object, which becomes the app's part and the first window's.
    pub fn from_value(value: Value) -> Doc {
        let Value::Object(top) = value else { return Doc::default() };
        if top.get("version").and_then(Value::as_u64) == Some(2) {
            let shared = top.get("shared").and_then(Value::as_object).cloned().unwrap_or_default();
            let windows = top
                .get("windows")
                .and_then(Value::as_object)
                .map(|all| all.iter().filter(|(_, v)| v.is_object()).map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();
            return Doc { shared, windows };
        }
        let own = pick(&top, &OWN);
        let mut windows = BTreeMap::new();
        if !own.is_empty() {
            windows.insert(MAIN.to_string(), Value::Object(own));
        }
        Doc { shared: pick(&top, &SHARED), windows }
    }

    pub fn to_value(&self) -> Value {
        json!({ "version": 2, "shared": self.shared, "windows": self.windows })
    }
}

/// A record without the frame, which the page does not keep.
fn without_frame(record: &Value) -> Value {
    let mut copy = record.clone();
    if let Some(object) = copy.as_object_mut() {
        object.remove("frame");
    }
    copy
}

// ------------------------------------------------------------------ saying what changed

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Value,
    Map,
    Set,
}

fn kind_of(field: &str) -> Option<Kind> {
    match field {
        "tabNames" | "workspaceNames" | "tabOrder" | "lastTab" => Some(Kind::Map),
        "hidden" => Some(Kind::Set),
        "daemon" | "zoom" | "appearance" | "font" | "theme" | "terminal" => Some(Kind::Value),
        _ => None,
    }
}

/// Make one change to the app's part. The change as it made a difference, or none when it was already so.
pub fn apply_op(shared: &mut Map<String, Value>, op: &Value) -> Option<Value> {
    let field = op.get("field")?.as_str()?;
    let kind = kind_of(field)?;
    match (op.get("op")?.as_str()?, kind) {
        ("put", Kind::Value) => {
            let value = op.get("value")?;
            if shared.get(field) == Some(value) {
                return None;
            }
            shared.insert(field.to_string(), value.clone());
        }
        ("set", Kind::Map) => {
            let (key, value) = (op.get("key")?.as_str()?, op.get("value")?);
            let map = shared.entry(field.to_string()).or_insert_with(|| json!({}));
            let map = map.as_object_mut()?;
            if map.get(key) == Some(value) {
                return None;
            }
            map.insert(key.to_string(), value.clone());
        }
        ("del", Kind::Map) => {
            let key = op.get("key")?.as_str()?;
            shared.get_mut(field)?.as_object_mut()?.remove(key)?;
        }
        ("add", Kind::Set) => {
            let item = op.get("item")?.as_str()?;
            let set = shared.entry(field.to_string()).or_insert_with(|| json!([]));
            let set = set.as_array_mut()?;
            if set.iter().any(|v| v.as_str() == Some(item)) {
                return None;
            }
            set.push(json!(item));
        }
        ("remove", Kind::Set) => {
            let item = op.get("item")?.as_str()?;
            let set = shared.get_mut(field)?.as_array_mut()?;
            let before = set.len();
            set.retain(|v| v.as_str() != Some(item));
            if set.len() == before {
                return None;
            }
        }
        _ => return None,
    }
    Some(op.clone())
}

// ------------------------------------------------------------------ the windows that are open

/// The windows that list workspaces, oldest first, each with what it last said of itself; and which was in front last.
#[derive(Debug, Default)]
pub struct Registry {
    entries: Vec<(String, Value)>,
    /// Most recently in front first.
    focus: Vec<String>,
}

fn carries(placement: &Value) -> Vec<String> {
    placement
        .get("carries")
        .and_then(Value::as_array)
        .map(|names| names.iter().filter_map(|n| n.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

impl Registry {
    /// Know a window. False when it was known already.
    pub fn add(&mut self, label: &str, placement: Option<Value>) -> bool {
        if !is_app_window(label) || self.entries.iter().any(|(l, _)| l == label) {
            return false;
        }
        self.entries.push((label.to_string(), placement.unwrap_or_else(|| json!({ "carries": [], "showing": null }))));
        true
    }

    /// What a window says of itself. False when it is what it was, or the window is not known.
    pub fn report(&mut self, label: &str, placement: Value) -> bool {
        match self.entries.iter_mut().find(|(l, _)| l == label) {
            Some((_, was)) if *was != placement => {
                *was = placement;
                true
            }
            _ => false,
        }
    }

    pub fn remove(&mut self, label: &str) -> bool {
        self.focus.retain(|l| l != label);
        let before = self.entries.len();
        self.entries.retain(|(l, _)| l != label);
        self.entries.len() != before
    }

    pub fn labels(&self) -> Vec<&str> {
        self.entries.iter().map(|(l, _)| l.as_str()).collect()
    }

    /// The windows, as the pages read them.
    pub fn list(&self) -> Value {
        Value::Array(self.entries.iter().map(|(label, placement)| json!({ "label": label, "placement": placement })).collect())
    }

    pub fn raise(&mut self, label: &str) {
        self.focus.retain(|l| l != label);
        self.focus.insert(0, label.to_string());
    }

    /// The windows from the one most recently in front to the one longest behind.
    pub fn front_to_back(&self) -> Vec<String> {
        let mut order: Vec<String> = self.focus.iter().filter(|l| self.entries.iter().any(|(e, _)| e == *l)).cloned().collect();
        for (label, _) in &self.entries {
            if !order.contains(label) {
                order.push(label.clone());
            }
        }
        order
    }

    /// The workspaces only this window lists: what is put away when it closes while others stay open.
    pub fn carried_only_by(&self, label: &str) -> Vec<String> {
        let elsewhere: HashSet<String> =
            self.entries.iter().filter(|(l, _)| l != label).flat_map(|(_, placement)| carries(placement)).collect();
        self.entries
            .iter()
            .find(|(l, _)| l == label)
            .map(|(_, placement)| carries(placement).into_iter().filter(|name| !elsewhere.contains(name)).collect())
            .unwrap_or_default()
    }

    pub fn others(&self, label: &str) -> usize {
        self.entries.iter().filter(|(l, _)| l != label).count()
    }
}

// ------------------------------------------------------------------ where things are on the screen

/// A rectangle on the screen, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Frame {
    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && i64::from(x) < i64::from(self.x) + i64::from(self.width) && i64::from(y) < i64::from(self.y) + i64::from(self.height)
    }
}

/// The window under a point: of those that contain it, the one longest in front. `frames` are in front-to-back order.
pub fn topmost_at(frames: &[(String, Frame)], x: i32, y: i32, exclude: Option<&str>) -> Option<String> {
    frames.iter().find(|(label, frame)| Some(label.as_str()) != exclude && frame.contains(x, y)).map(|(label, _)| label.clone())
}

/// How much of the title row a monitor must show for a window put there to be reachable.
const REACH_WIDTH: i64 = 96;
const REACH_HEIGHT: i64 = 32;

/// Whether a saved frame is on a monitor that is here: "a restored frame is a request, not a promise" — a laptop
/// comes back without its monitor. The window's top edge must show enough of itself to be grabbed.
pub fn on_a_monitor(frame: &Frame, monitors: &[Frame]) -> bool {
    monitors.iter().any(|m| {
        let left = i64::from(frame.x.max(m.x));
        let right = (i64::from(frame.x) + i64::from(frame.width)).min(i64::from(m.x) + i64::from(m.width));
        let top = i64::from(frame.y.max(m.y));
        let bottom = (i64::from(frame.y) + REACH_HEIGHT).min(i64::from(m.y) + i64::from(m.height));
        right - left >= REACH_WIDTH && bottom - top >= REACH_HEIGHT.min(i64::from(m.height))
    })
}

// ------------------------------------------------------------------ the hub

#[derive(Default)]
struct Inner {
    loaded: bool,
    path: Option<PathBuf>,
    doc: Doc,
    /// How many changes to the app's part have been made: every window hears them in this order.
    seq: u64,
    registry: Registry,
}

/// What the app knows of its windows, managed by Tauri.
#[derive(Default)]
pub struct Hub {
    inner: Mutex<Inner>,
    /// The app is quitting: the windows closing on the way out put nothing away.
    exiting: AtomicBool,
    /// A write of the frames is on its way.
    flushing: AtomicBool,
}

fn state_path(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    Some(settings::state_file(&dir))
}

impl Hub {
    fn with<R>(&self, app: &AppHandle, f: impl FnOnce(&mut Inner) -> R) -> R {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if !inner.loaded {
            inner.loaded = true;
            inner.path = state_path(app);
            if let Some(value) = inner.path.as_deref().and_then(settings::load) {
                inner.doc = Doc::from_value(value);
            }
        }
        f(&mut inner)
    }

    /// The app is leaving: from here on a window that closes is the end of the app, not a window put away.
    pub fn quitting(&self) {
        self.exiting.store(true, Ordering::SeqCst);
    }
}

fn hub(app: &AppHandle) -> tauri::State<'_, Hub> {
    app.state::<Hub>()
}

fn persist(inner: &Inner) {
    let Some(path) = &inner.path else { return };
    if let Err(e) = settings::save(path, &inner.doc.to_value()) {
        settings::log(&format!("estado não gravado: {e:#}"));
    }
}

fn tell_windows(app: &AppHandle, list: Value) {
    let _ = app.emit(EVENT_WINDOWS, list);
}

/// The app starts: the first window is known before its page asks.
pub fn start(app: &AppHandle) {
    let known = hub(app).with(app, |inner| inner.registry.add(MAIN, None));
    if known {
        let list = hub(app).with(app, |inner| inner.registry.list());
        tell_windows(app, list);
    }
}

/// The app is leaving: what is on disk is written, and the windows closing from here on put nothing away.
pub fn leave(app: &AppHandle) {
    let hub = hub(app);
    hub.quitting();
    hub.with(app, |inner| persist(inner));
}

// ------------------------------------------------------------------ what a page asks

/// What a page needs before it draws: the app's part, its own, and which windows there are.
#[derive(Serialize)]
pub struct Boot {
    label: String,
    seq: u64,
    shared: Value,
    /// This window's own record; none for a window that has never been saved.
    window: Option<Value>,
    /// The first window's record, for a window with none to start from the furniture it has.
    inherit: Option<Value>,
    windows: Value,
    /// The windows of the last run that are not open, to be opened (asked by the first window only).
    saved: Vec<Value>,
}

#[tauri::command]
pub fn window_boot(app: AppHandle, window: Window) -> Boot {
    let label = window.label().to_string();
    let (boot, changed) = hub(&app).with(&app, |inner| {
        let changed = inner.registry.add(&label, None);
        let live: HashSet<String> = inner.registry.labels().into_iter().map(str::to_string).collect();
        let saved = if label == MAIN {
            inner
                .doc
                .windows
                .iter()
                .filter(|(l, _)| is_app_window(l) && *l != MAIN && !live.contains(*l))
                .map(|(l, record)| json!({ "label": l, "window": without_frame(record), "frame": record.get("frame") }))
                .collect()
        } else {
            Vec::new()
        };
        let boot = Boot {
            label: label.clone(),
            seq: inner.seq,
            shared: Value::Object(inner.doc.shared.clone()),
            window: inner.doc.windows.get(&label).filter(|_| is_app_window(&label)).map(without_frame),
            inherit: inner.doc.windows.get(MAIN).map(without_frame),
            windows: inner.registry.list(),
            saved,
        };
        (boot, changed)
    });
    if changed {
        tell_windows(&app, boot.windows.clone());
    }
    boot
}

/// Change the app's part of the state. Every window hears what made a difference, the sender too; the number
/// it is told under is the order the changes were made in.
#[tauri::command]
pub fn shared_apply(app: AppHandle, window: Window, ops: Vec<Value>) -> u64 {
    let (seq, done) = hub(&app).with(&app, |inner| {
        let done: Vec<Value> = ops.iter().filter_map(|op| apply_op(&mut inner.doc.shared, op)).collect();
        if !done.is_empty() {
            inner.seq += 1;
            persist(inner);
        }
        (inner.seq, done)
    });
    if !done.is_empty() {
        let _ = app.emit(EVENT_SHARED, json!({ "seq": seq, "from": window.label(), "ops": done }));
    }
    seq
}

/// The part of the state this window keeps for itself.
#[tauri::command]
pub fn window_save(app: AppHandle, window: Window, record: Value) {
    let label = window.label().to_string();
    if !is_app_window(&label) || !record.is_object() {
        return;
    }
    hub(&app).with(&app, |inner| {
        let mut record = without_frame(&record);
        if let Some(frame) = inner.doc.windows.get(&label).and_then(|r| r.get("frame")) {
            record["frame"] = frame.clone();
        }
        if inner.doc.windows.get(&label) != Some(&record) {
            inner.doc.windows.insert(label, record);
            persist(inner);
        }
    });
}

/// What this window says of itself: the workspaces it lists, the tab in front.
#[tauri::command]
pub fn windows_report(app: AppHandle, window: Window, placement: Value) {
    let label = window.label().to_string();
    let list = hub(&app).with(&app, |inner| inner.registry.report(&label, placement).then(|| inner.registry.list()));
    if let Some(list) = list {
        tell_windows(&app, list);
    }
}

#[tauri::command]
pub fn windows_list(app: AppHandle) -> Value {
    hub(&app).with(&app, |inner| inner.registry.list())
}

/// What a window to open is asked to be.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Open {
    /// The label to give it (a window of the last run); else the lowest free slot.
    label: Option<String>,
    frame: Option<Frame>,
    maximized: Option<bool>,
    /// What it will say of itself until it speaks.
    placement: Option<Value>,
    /// The part of the state it starts with (a tab torn out of a row opens a window listing its workspace, with
    /// the tab in front), when it is not the one it kept last time.
    record: Option<Value>,
}

#[tauri::command]
pub async fn window_open(app: AppHandle, request: Open) -> Answer<String> {
    let (label, list) = hub(&app).with(&app, |inner| {
        let taken = inner.registry.labels();
        let label = match request.label.as_deref() {
            Some(wanted) if is_app_window(wanted) && !taken.contains(&wanted) => wanted.to_string(),
            _ => free_label(taken),
        };
        inner.registry.add(&label, request.placement.clone());
        if let Some(record) = request.record.as_ref().filter(|r| r.is_object()) {
            let mut record = without_frame(record);
            if let Some(frame) = inner.doc.windows.get(&label).and_then(|r| r.get("frame")) {
                record["frame"] = frame.clone();
            }
            inner.doc.windows.insert(label.clone(), record);
            persist(inner);
        }
        (label, inner.registry.list())
    });
    match build(&app, &label, &request) {
        Ok(()) => {
            tell_windows(&app, list);
            Ok(label)
        }
        Err(e) => {
            let list = hub(&app).with(&app, |inner| {
                inner.registry.remove(&label);
                // What it was to start with is not kept for a window that never opened.
                if request.record.is_some() {
                    inner.doc.windows.remove(&label);
                    persist(inner);
                }
                inner.registry.list()
            });
            tell_windows(&app, list);
            Err(e)
        }
    }
}

/// The monitors, as frames.
fn monitors(app: &AppHandle) -> Vec<Frame> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| Frame { x: m.position().x, y: m.position().y, width: m.size().width, height: m.size().height })
        .collect()
}

/// Make the window: the first one's own configuration (frameless on Windows, the title bar of the macOS build),
/// the same page, at the size and place it is asked for when that place is on a monitor that is here.
fn build(app: &AppHandle, label: &str, request: &Open) -> Answer<()> {
    let mut config = app.config().app.windows.first().cloned().ok_or("sem janela na configuração")?;
    config.label = label.to_string();
    let window = tauri::WebviewWindowBuilder::from_config(app, &config)
        .map_err(|e| e.to_string())?
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
    if let Some(frame) = request.frame {
        let _ = window.set_size(PhysicalSize::new(frame.width, frame.height));
        if on_a_monitor(&frame, &monitors(app)) {
            let _ = window.set_position(PhysicalPosition::new(frame.x, frame.y));
        }
    }
    if request.maximized == Some(true) {
        let _ = window.maximize();
    }
    window.show().map_err(|e| e.to_string())?;
    let _ = window.set_focus();
    Ok(())
}

/// The window under a point of the screen, if any, front to back; not `exclude`.
#[tauri::command]
pub async fn window_under(app: AppHandle, x: i32, y: i32, exclude: Option<String>) -> Option<String> {
    let order = hub(&app).with(&app, |inner| inner.registry.front_to_back());
    let windows = app.webview_windows();
    let frames: Vec<(String, Frame)> = order
        .into_iter()
        .filter_map(|label| {
            let window = windows.get(&label)?;
            if window.is_minimized().unwrap_or(false) || !window.is_visible().unwrap_or(true) {
                return None;
            }
            let (at, size) = (window.outer_position().ok()?, window.outer_size().ok()?);
            Some((label, Frame { x: at.x, y: at.y, width: size.width, height: size.height }))
        })
        .collect();
    topmost_at(&frames, x, y, exclude.as_deref())
}

/// Bring a window forward.
#[tauri::command]
pub async fn window_show(app: AppHandle, label: String) {
    if let Some(window) = app.get_webview_window(&label) {
        bring_forward(&window);
    }
}

fn bring_forward(window: &tauri::WebviewWindow) {
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

/// A second launch: the window that was in front last comes forward.
pub fn show_front(app: &AppHandle) {
    let order = hub(app).with(app, |inner| inner.registry.front_to_back());
    for label in order {
        if let Some(window) = app.get_webview_window(&label) {
            bring_forward(&window);
            return;
        }
    }
    if let Some(window) = app.webview_windows().values().next() {
        bring_forward(window);
    }
}

// ------------------------------------------------------------------ what happens to a window

/// What the windows of the app do: one comes to the front, one moves, one is gone.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    let label = window.label();
    if !is_app_window(label) {
        return;
    }
    let app = window.app_handle();
    match event {
        WindowEvent::Focused(true) => hub(app).with(app, |inner| inner.registry.raise(label)),
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => note_frame(app, window),
        WindowEvent::Destroyed => closed(app, label),
        _ => {}
    }
}

/// Where a window is, kept for the next run. The first window's frame is the window-state plugin's.
fn note_frame(app: &AppHandle, window: &Window) {
    let label = window.label();
    if label == MAIN || window.is_minimized().unwrap_or(false) {
        return;
    }
    let (Ok(at), Ok(size), Ok(maximized)) = (window.outer_position(), window.inner_size(), window.is_maximized()) else { return };
    if size.width == 0 || size.height == 0 {
        return;
    }
    hub(app).with(app, |inner| {
        let Some(record) = inner.doc.windows.get_mut(label) else { return };
        let was = record.get("frame").cloned();
        // Maximized, the frame it had before is the one to come back to.
        let frame = if maximized {
            let mut kept = was.clone().filter(Value::is_object).unwrap_or_else(|| json!({}));
            kept["maximized"] = json!(true);
            kept
        } else {
            json!({ "x": at.x, "y": at.y, "width": size.width, "height": size.height, "maximized": false })
        };
        if was.as_ref() != Some(&frame) {
            record["frame"] = frame;
            schedule_flush(app);
        }
    });
}

/// Write the frames a moment from now, once for a burst of moves.
fn schedule_flush(app: &AppHandle) {
    let hub = hub(app);
    if hub.flushing.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(900));
        let hub = app.state::<Hub>();
        hub.flushing.store(false, Ordering::SeqCst);
        hub.with(&app, |inner| persist(inner));
    });
}

/// A window is gone. If others stay open, what only it listed is put away — running, a Go To away, "not pushed
/// into another window" — and its record goes with it. The last one going is the app quitting: it puts
/// nothing away and its record stays, to be opened again.
fn closed(app: &AppHandle, label: &str) {
    let hub = hub(app);
    let quitting = hub.exiting.load(Ordering::SeqCst);
    let (done, seq, list) = hub.with(app, |inner| {
        let leaving = !quitting && inner.registry.others(label) > 0;
        let mut done = Vec::new();
        if leaving {
            for name in inner.registry.carried_only_by(label) {
                let op = json!({ "op": "add", "field": "hidden", "item": name });
                done.extend(apply_op(&mut inner.doc.shared, &op));
            }
            inner.doc.windows.remove(label);
            if !done.is_empty() {
                inner.seq += 1;
            }
            persist(inner);
        }
        let known = inner.registry.remove(label);
        (done, inner.seq, known.then(|| inner.registry.list()))
    });
    if !done.is_empty() {
        let _ = app.emit(EVENT_SHARED, json!({ "seq": seq, "from": label, "ops": done }));
    }
    if let Some(list) = list {
        tell_windows(app, list);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(x: i32, y: i32, width: u32, height: u32) -> Frame {
        Frame { x, y, width, height }
    }

    #[test]
    fn labels_are_main_and_w_n() {
        assert_eq!(slot_of("main"), Some(0));
        assert_eq!(slot_of("w1"), Some(1));
        assert_eq!(slot_of("w42"), Some(42));
        for bad in ["quick", "w0", "w01", "w", "wx", "main2", "", "w1000000", "W1"] {
            assert_eq!(slot_of(bad), None, "{bad}");
        }
        assert_eq!(label_of(0), "main");
        assert_eq!(label_of(7), "w7");
        assert!(is_app_window("w3") && !is_app_window("quick"));
    }

    #[test]
    fn a_new_window_takes_the_lowest_free_slot() {
        assert_eq!(free_label(["main"]), "w1");
        assert_eq!(free_label(["main", "w1", "w2"]), "w3");
        assert_eq!(free_label(["main", "w2"]), "w1");
        assert_eq!(free_label(["w1"]), "main");
        assert_eq!(free_label(["main", "quick"]), "w1");
        assert_eq!(free_label([]), "main");
    }

    #[test]
    fn the_state_the_app_wrote_with_one_window_becomes_the_first_windows() {
        let old = json!({
            "version": 1, "order": ["a", "b"], "folded": ["a"], "tabNames": {"a\u{0}1": "um"}, "workspaceNames": {},
            "active": {"workspace": "a", "tab": 1}, "lastTab": {"a": 1}, "tabOrder": {"a": [2, 1]}, "hidden": ["b"],
            "verticalTabs": true, "zoom": 1.25, "appearance": "dark", "sidebar": {"visible": true, "width": 260},
            "font": {"family": "", "size": 0}, "theme": "Nord", "terminal": {"cursor": "bar", "blink": true, "scrollback": 10000},
            "daemon": "1-2"
        });
        let doc = Doc::from_value(old);
        assert_eq!(doc.shared.len(), SHARED.len());
        for key in SHARED {
            assert!(doc.shared.contains_key(key), "{key}");
        }
        assert_eq!(doc.shared["hidden"], json!(["b"]));
        assert_eq!(doc.shared["zoom"], json!(1.25));
        let main = doc.windows["main"].as_object().unwrap();
        assert_eq!(main.len(), OWN.len());
        assert_eq!(main["order"], json!(["a", "b"]));
        assert_eq!(main["verticalTabs"], json!(true));
        assert_eq!(main["sidebar"], json!({"visible": true, "width": 260}));
        // Nothing of the old state is lost, and nothing is in both parts.
        assert!(SHARED.iter().all(|k| !OWN.contains(k)));
        assert!(!doc.windows["main"].as_object().unwrap().contains_key("version"));
    }

    #[test]
    fn a_state_with_no_window_in_it_starts_no_record() {
        let doc = Doc::from_value(json!({"version": 1, "zoom": 2}));
        assert_eq!(doc.shared["zoom"], json!(2));
        assert!(doc.windows.is_empty());
        assert_eq!(Doc::from_value(json!([1, 2])), Doc::default());
        assert_eq!(Doc::from_value(json!("x")), Doc::default());
    }

    #[test]
    fn the_document_of_this_version_is_read_back_whole() {
        let doc = Doc {
            shared: json!({"zoom": 1.5, "hidden": ["x"]}).as_object().unwrap().clone(),
            windows: BTreeMap::from([
                ("main".to_string(), json!({"order": ["a"], "frame": {"x": 1}})),
                ("w1".to_string(), json!({"order": []})),
            ]),
        };
        let value = doc.to_value();
        assert_eq!(value["version"], json!(2));
        assert_eq!(Doc::from_value(value), doc);
        // A record that is not an object is not a window.
        let odd = Doc::from_value(json!({"version": 2, "shared": {"zoom": 1}, "windows": {"main": 3, "w1": {"order": []}}}));
        assert_eq!(odd.windows.keys().collect::<Vec<_>>(), ["w1"]);
    }

    #[test]
    fn a_record_is_given_to_the_page_without_its_frame() {
        let record = json!({"order": ["a"], "frame": {"x": 1, "y": 2, "width": 3, "height": 4, "maximized": false}});
        assert_eq!(without_frame(&record), json!({"order": ["a"]}));
        assert_eq!(without_frame(&json!(3)), json!(3));
    }

    fn shared() -> Map<String, Value> {
        json!({"tabNames": {"a\u{0}1": "um"}, "hidden": ["x"], "zoom": 1}).as_object().unwrap().clone()
    }

    #[test]
    fn changes_that_make_a_difference_are_made_and_told() {
        let mut s = shared();
        let ops = [
            json!({"op": "set", "field": "tabNames", "key": "a\u{0}2", "value": "dois"}),
            json!({"op": "del", "field": "tabNames", "key": "a\u{0}1"}),
            json!({"op": "add", "field": "hidden", "item": "y"}),
            json!({"op": "remove", "field": "hidden", "item": "x"}),
            json!({"op": "put", "field": "zoom", "value": 1.5}),
            json!({"op": "put", "field": "theme", "value": "Nord"}),
            json!({"op": "set", "field": "lastTab", "key": "a", "value": 3}),
        ];
        for op in &ops {
            assert_eq!(apply_op(&mut s, op).as_ref(), Some(op));
        }
        assert_eq!(s["tabNames"], json!({"a\u{0}2": "dois"}));
        assert_eq!(s["hidden"], json!(["y"]));
        assert_eq!(s["zoom"], json!(1.5));
        assert_eq!(s["theme"], json!("Nord"));
        assert_eq!(s["lastTab"], json!({"a": 3}));
    }

    #[test]
    fn changes_already_so_are_not_told() {
        let mut s = shared();
        let before = s.clone();
        for op in [
            json!({"op": "set", "field": "tabNames", "key": "a\u{0}1", "value": "um"}),
            json!({"op": "del", "field": "tabNames", "key": "nunca"}),
            json!({"op": "add", "field": "hidden", "item": "x"}),
            json!({"op": "remove", "field": "hidden", "item": "nunca"}),
            json!({"op": "put", "field": "zoom", "value": 1}),
            json!({"op": "del", "field": "lastTab", "key": "a"}),
        ] {
            assert_eq!(apply_op(&mut s, &op), None, "{op}");
        }
        assert_eq!(s, before);
    }

    #[test]
    fn changes_that_make_no_sense_are_left_out() {
        let mut s = shared();
        let before = s.clone();
        for op in [
            json!({"op": "put", "field": "order", "value": []}),
            json!({"op": "put", "field": "hidden", "value": []}),
            json!({"op": "set", "field": "zoom", "key": "k", "value": 1}),
            json!({"op": "add", "field": "tabNames", "item": "x"}),
            json!({"op": "set", "field": "tabNames", "key": 3, "value": "x"}),
            json!({"op": "add", "field": "hidden", "item": 3}),
            json!({"op": "boom", "field": "zoom"}),
            json!({"field": "zoom"}),
            json!("x"),
        ] {
            assert_eq!(apply_op(&mut s, &op), None, "{op}");
        }
        assert_eq!(s, before);
    }

    #[test]
    fn a_field_nobody_has_set_yet_is_made_by_the_first_change() {
        let mut s = Map::new();
        assert!(apply_op(&mut s, &json!({"op": "set", "field": "lastTab", "key": "a", "value": 1})).is_some());
        assert!(apply_op(&mut s, &json!({"op": "add", "field": "hidden", "item": "a"})).is_some());
        assert_eq!(Value::Object(s), json!({"lastTab": {"a": 1}, "hidden": ["a"]}));
        assert_eq!(apply_op(&mut Map::new(), &json!({"op": "del", "field": "tabNames", "key": "a"})), None);
    }

    fn placement(carries: &[&str], showing: Option<(&str, u32)>) -> Value {
        json!({ "carries": carries, "showing": showing.map(|(w, t)| json!({"workspace": w, "tab": t})) })
    }

    #[test]
    fn the_windows_are_listed_oldest_first_and_only_the_ones_that_list_workspaces() {
        let mut r = Registry::default();
        assert!(r.add("main", None));
        assert!(r.add("w1", Some(placement(&["a"], None))));
        assert!(!r.add("w1", None), "known already");
        assert!(!r.add("quick", None), "not a window of the app");
        assert_eq!(r.labels(), ["main", "w1"]);
        let list = r.list();
        assert_eq!(list[0]["label"], json!("main"));
        assert_eq!(list[0]["placement"], json!({"carries": [], "showing": null}));
        assert_eq!(list[1]["placement"]["carries"], json!(["a"]));
    }

    #[test]
    fn a_window_says_what_it_lists_and_a_repeat_is_no_news() {
        let mut r = Registry::default();
        r.add("main", None);
        assert!(r.report("main", placement(&["a", "b"], Some(("a", 1)))));
        assert!(!r.report("main", placement(&["a", "b"], Some(("a", 1)))));
        assert!(r.report("main", placement(&["a", "b"], Some(("a", 2)))));
        assert!(!r.report("w9", placement(&[], None)), "a window nobody knows");
    }

    #[test]
    fn what_only_a_closing_window_lists_is_what_is_put_away() {
        let mut r = Registry::default();
        r.add("main", Some(placement(&["a", "b"], None)));
        r.add("w1", Some(placement(&["b", "c", "d"], None)));
        r.add("w2", Some(placement(&["d"], None)));
        assert_eq!(r.carried_only_by("w1"), ["c"]);
        assert_eq!(r.carried_only_by("main"), ["a"]);
        assert_eq!(r.carried_only_by("w2"), Vec::<String>::new());
        assert_eq!(r.carried_only_by("nobody"), Vec::<String>::new());
        assert_eq!(r.others("w1"), 2);
        assert!(r.remove("w2"));
        assert_eq!(r.carried_only_by("w1"), ["c", "d"]);
        assert!(!r.remove("w2"));
    }

    #[test]
    fn the_window_in_front_is_the_one_that_was_in_front_last() {
        let mut r = Registry::default();
        for l in ["main", "w1", "w2"] {
            r.add(l, None);
        }
        assert_eq!(r.front_to_back(), ["main", "w1", "w2"]);
        r.raise("w2");
        r.raise("w1");
        assert_eq!(r.front_to_back(), ["w1", "w2", "main"]);
        r.remove("w1");
        assert_eq!(r.front_to_back(), ["w2", "main"]);
        r.raise("ghost");
        assert_eq!(r.front_to_back(), ["w2", "main"], "a window nobody knows is not in the list");
    }

    #[test]
    fn the_window_under_a_point_is_the_one_in_front_of_those_that_contain_it() {
        let windows = vec![
            ("w1".to_string(), frame(100, 100, 400, 300)),
            ("main".to_string(), frame(0, 0, 1000, 800)),
            ("w2".to_string(), frame(2000, 0, 500, 500)),
        ];
        assert_eq!(topmost_at(&windows, 200, 200, None).as_deref(), Some("w1"));
        assert_eq!(topmost_at(&windows, 700, 700, None).as_deref(), Some("main"));
        assert_eq!(topmost_at(&windows, 2100, 10, None).as_deref(), Some("w2"));
        assert_eq!(topmost_at(&windows, 1500, 10, None), None);
        // The window the tab came from does not count, and what is under it does.
        assert_eq!(topmost_at(&windows, 200, 200, Some("w1")).as_deref(), Some("main"));
        // The edges: left and top inside, right and bottom outside.
        assert_eq!(topmost_at(&windows, 100, 100, None).as_deref(), Some("w1"));
        assert_eq!(topmost_at(&windows, 500, 400, None).as_deref(), Some("main"));
        assert_eq!(topmost_at(&windows, -1, 0, None), None);
    }

    #[test]
    fn a_saved_frame_comes_back_only_where_a_monitor_is() {
        let monitors = [frame(0, 0, 1920, 1080), frame(1920, 0, 1280, 1024)];
        assert!(on_a_monitor(&frame(100, 100, 800, 600), &monitors));
        assert!(on_a_monitor(&frame(2000, 50, 1000, 700), &monitors), "on the second monitor");
        assert!(on_a_monitor(&frame(1800, 50, 800, 600), &monitors), "across the two");
        // The monitor it was on is gone.
        assert!(!on_a_monitor(&frame(3300, 100, 800, 600), &monitors));
        assert!(!on_a_monitor(&frame(-2500, 100, 800, 600), &monitors));
        // Its title row below the screen, or only a sliver of it showing at the side.
        assert!(!on_a_monitor(&frame(100, 1070, 800, 600), &monitors));
        assert!(!on_a_monitor(&frame(1900, 100, 800, 600), &[frame(0, 0, 1920, 1080)]));
        assert!(!on_a_monitor(&frame(-780, 100, 800, 600), &monitors));
        assert!(!on_a_monitor(&frame(100, 100, 800, 600), &[]));
    }
}
