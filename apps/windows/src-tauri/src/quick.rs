//! The drop-down terminal: Ctrl+` from anywhere, and a terminal slides in over
//! whatever you were doing; the same key again — or a click elsewhere — and it
//! leaves (QuickTerminal.swift in the macOS app).
//!
//! The window is the ephemeral half. The shell behind it is a tab of the
//! daemon's workspace `quick`, held like every other tab, so what runs in it
//! outlives the window and the app: sliding the panel away only stops looking
//! at it. The panel is a second window of this app showing the same page in
//! its quick mode (`?quick=1`, ../src/quick/boot.tsx), made the first time it
//! is asked for and reused after.
//!
//! What is here: the shortcut (kept in `quick.json`, handed to the system,
//! changeable), the window (borderless, on top, off the taskbar, the screen's
//! width and 42% of its height, hung from the top of the screen the pointer is
//! on) and how it comes and goes (a slide in, a slide out, and away on losing
//! the focus). The numbers are QuickTerminal.swift's; the page only draws the
//! terminal.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tauri::window::Color;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, RunEvent, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, Window, WindowEvent,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// The window's label, and the name the capability (capabilities/quick.json) gives its permissions to.
pub const LABEL: &str = "quick";

/// What the Mac registers: ⌃`.
pub const DEFAULT_SHORTCUT: &str = "Control+Backquote";

/// Offered in its place when the system will not give the shortcut, in this order. Ctrl and Shift
/// come before Ctrl and Alt, which a layout with AltGr (ABNT2 has it) types its own characters with.
const ALTERNATIVES: [&str; 4] = [
    "Control+Shift+Backquote",
    "Control+Alt+Backquote",
    "Control+Shift+KeyK",
    "Control+Alt+Space",
];

/// How tall the panel is, as a share of the screen's work area: `visibleFrame.height * 0.42`.
const HEIGHT_SHARE: f64 = 0.42;
/// The slide in and the slide out, as the Mac times them.
const SLIDE_IN: Duration = Duration::from_millis(180);
const SLIDE_OUT: Duration = Duration::from_millis(150);
/// The time between two steps of a slide. The event loop sets the real pace; this keeps the thread from spinning.
const STEP: Duration = Duration::from_millis(8);
/// The longest the first show waits for the page to have drawn, so that the panel does not come in blank.
const PAGE_WAIT: Duration = Duration::from_secs(3);
/// The window's colour before the page has painted: Keep's dark ground, as the main window's.
const GROUND: Color = Color(0x28, 0x2c, 0x34, 0xff);

// ------------------------------------------------------------------ where the panel goes

/// A rectangle of the desktop, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= f64::from(self.x)
            && x < f64::from(self.x) + f64::from(self.width)
            && y >= f64::from(self.y)
            && y < f64::from(self.y) + f64::from(self.height)
    }

    /// The square of how far a point is from the rectangle: nothing inside it.
    fn distance2(&self, x: f64, y: f64) -> f64 {
        let gap = |low: f64, size: f64, at: f64| (low - at).max(at - (low + size)).max(0.0);
        let dx = gap(f64::from(self.x), f64::from(self.width), x);
        let dy = gap(f64::from(self.y), f64::from(self.height), y);
        dx * dx + dy * dy
    }
}

/// A monitor: all of it, and the part of it the taskbar and the like leave (the Mac's `frame` and `visibleFrame`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Screen {
    pub frame: Rect,
    pub work: Rect,
}

/// The screen the pointer is on: the panel is summoned by a keystroke, and the keystroke's context is wherever the
/// person is working. A pointer on none (the gap a staggered layout leaves) goes to the nearest.
pub fn screen_at(screens: &[Screen], x: f64, y: f64) -> Option<Screen> {
    screens.iter().copied().find(|s| s.frame.contains(x, y)).or_else(|| {
        screens
            .iter()
            .copied()
            .min_by(|a, b| a.frame.distance2(x, y).total_cmp(&b.frame.distance2(x, y)))
    })
}

/// Where the panel rests on a screen: the screen's whole width, `HEIGHT_SHARE` of the work area's height (rounded,
/// as the Mac's), hung from the top of the work area.
pub fn resting(screen: &Screen) -> Rect {
    Rect {
        x: screen.frame.x,
        y: screen.work.y,
        width: screen.frame.width,
        height: (f64::from(screen.work.height) * HEIGHT_SHARE).round() as u32,
    }
}

/// The top edge of the panel when it stands one height above its resting place: where it slides in from.
pub fn above(rest: Rect) -> i32 {
    rest.y - i32::try_from(rest.height).unwrap_or(i32::MAX)
}

/// A cubic Bézier timing curve, as Core Animation's named ones are.
#[derive(Clone, Copy, Debug)]
pub struct Curve {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

impl Curve {
    /// `CAMediaTimingFunction.easeOut`: the slide in.
    pub const EASE_OUT: Curve = Curve {
        x1: 0.0,
        y1: 0.0,
        x2: 0.58,
        y2: 1.0,
    };
    /// `CAMediaTimingFunction.easeIn`: the slide out.
    pub const EASE_IN: Curve = Curve {
        x1: 0.42,
        y1: 0.0,
        x2: 1.0,
        y2: 1.0,
    };

    fn bezier(a: f64, b: f64, s: f64) -> f64 {
        let u = 1.0 - s;
        3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
    }

    /// How far along the motion is, from 0 to 1, `t` of the way through the time.
    pub fn at(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        // x grows with the parameter, so the parameter for a given x is found by halving.
        let (mut low, mut high) = (0.0, 1.0);
        for _ in 0..40 {
            let mid = (low + high) / 2.0;
            if Self::bezier(self.x1, self.x2, mid) < t {
                low = mid;
            } else {
                high = mid;
            }
        }
        Self::bezier(self.y1, self.y2, (low + high) / 2.0)
    }
}

/// `progress` of the way from `from` to `to`.
fn lerp(from: i32, to: i32, progress: f64) -> i32 {
    from + (f64::from(to - from) * progress).round() as i32
}

// ------------------------------------------------------------------ the shortcut

/// A shortcut as the page writes it and `Shortcut` reads it — `Control+Alt+Backquote`: the modifiers in a
/// fixed order, then the key by its `KeyboardEvent.code`. One spelling, so that what the page keeps compares equal.
pub fn canonical(shortcut: &Shortcut) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (modifier, name) in [
        (Modifiers::CONTROL, "Control"),
        (Modifiers::ALT, "Alt"),
        (Modifiers::SHIFT, "Shift"),
        (Modifiers::SUPER, "Super"),
    ] {
        if shortcut.mods.contains(modifier) {
            parts.push(name.to_string());
        }
    }
    parts.push(shortcut.key.to_string());
    parts.join("+")
}

const FUNCTION_KEYS: [Code; 24] = [
    Code::F1,
    Code::F2,
    Code::F3,
    Code::F4,
    Code::F5,
    Code::F6,
    Code::F7,
    Code::F8,
    Code::F9,
    Code::F10,
    Code::F11,
    Code::F12,
    Code::F13,
    Code::F14,
    Code::F15,
    Code::F16,
    Code::F17,
    Code::F18,
    Code::F19,
    Code::F20,
    Code::F21,
    Code::F22,
    Code::F23,
    Code::F24,
];

/// A shortcut read from text, if it is one a person can safely have take a key from every program: some Ctrl, Alt
/// or Win in it (Shift alone would take the capital letters), or a function key.
pub fn parse(text: &str) -> Result<Shortcut, String> {
    let shortcut = Shortcut::from_str(text.trim()).map_err(|_| format!("não entendi o atalho “{}”", text.trim()))?;
    if shortcut.key == Code::Escape {
        return Err("a tecla Esc não serve de atalho".into());
    }
    let held = shortcut
        .mods
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER);
    if !held && !FUNCTION_KEYS.contains(&shortcut.key) {
        return Err("o atalho precisa de Ctrl, Alt ou Win (ou ser uma tecla de função)".into());
    }
    Ok(shortcut)
}

// ------------------------------------------------------------------ what is kept

/// `quick.json`: what the person chose, and only that, so that a default that changes later reaches
/// whoever never chose.
#[derive(Default, Serialize, Deserialize)]
struct Config {
    #[serde(default)]
    shortcut: Option<String>,
}

/// Beside the window's state (`KEEP_STATE_DIR` moves both, for a test).
fn config_file(app: &AppHandle) -> Option<PathBuf> {
    match std::env::var_os("KEEP_STATE_DIR") {
        Some(dir) => Some(PathBuf::from(dir).join("quick.json")),
        None => app.path().app_config_dir().ok().map(|dir| dir.join("quick.json")),
    }
}

/// A file that is not there, or not ours, reads as nothing chosen.
fn read_config(path: &Path) -> Config {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Whole or not at all: written beside and moved into place.
fn write_config(path: &Path, config: &Config) -> Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).with_context(|| format!("criar {}", dir.display()))?;
    }
    let aside = path.with_extension("json.tmp");
    std::fs::write(&aside, serde_json::to_string_pretty(config)? + "\n")
        .with_context(|| format!("gravar {}", aside.display()))?;
    std::fs::rename(&aside, path)
        .with_context(|| format!("trocar {}", path.display()))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&aside);
        })
}

// ------------------------------------------------------------------ the state

/// The shortcut: what is asked for, and whether the system gave it.
#[derive(Default)]
struct Setting {
    wanted: String,
    registered: Option<Shortcut>,
    problem: Option<String>,
    suggestion: Option<String>,
}

#[derive(Default)]
pub struct Quick {
    /// One decision at a time about coming or going: the shortcut, the palette and a click away may meet.
    /// Not held while a slide runs, so that a new decision does not wait for the old motion.
    turn: Mutex<()>,
    /// Whether the panel is meant to be in view. The window may still be sliding out.
    visible: AtomicBool,
    /// Which slide is the current one: a newer slide makes the older stop where it is.
    slide: AtomicU64,
    /// The panel has had the focus since it came in, so that losing it is a click away and not the noise of arriving.
    focused: AtomicBool,
    /// The page has drawn (`quick_ready`).
    page: Mutex<bool>,
    page_ready: Condvar,
    /// One choice of shortcut at a time. Taking one waits on the event loop, so what the loop itself may
    /// need (`setting`) is never held meanwhile.
    choosing: Mutex<()>,
    setting: Mutex<Setting>,
}

impl Quick {
    fn wait_for_page(&self) {
        let deadline = Instant::now() + PAGE_WAIT;
        let Ok(mut ready) = self.page.lock() else { return };
        while !*ready {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return;
            }
            match self.page_ready.wait_timeout(ready, left) {
                Ok((guard, _)) => ready = guard,
                Err(_) => return,
            }
        }
    }
}

/// Where the window stands, for the scripted check and the settings.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub frame: Rect,
    pub always_on_top: bool,
    pub focused: bool,
    pub shown: bool,
}

/// What the page is told about the quick window.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// The shortcut asked for: the default until a person chooses.
    pub shortcut: String,
    /// Whether the system gave it.
    pub registered: bool,
    /// If not, what the system said.
    pub problem: Option<String>,
    /// A shortcut that is free now, to offer in its place.
    pub suggestion: Option<String>,
    /// Whether the panel is meant to be in view.
    pub visible: bool,
    /// The window as it stands; none until it is first asked for.
    pub window: Option<Placement>,
}

pub fn status(app: &AppHandle) -> Status {
    let quick = app.state::<Quick>();
    let (shortcut, registered, problem, suggestion) = match quick.setting.lock() {
        Ok(s) => (
            s.wanted.clone(),
            s.registered.is_some(),
            s.problem.clone(),
            s.suggestion.clone(),
        ),
        Err(_) => (DEFAULT_SHORTCUT.to_string(), false, None, None),
    };
    let window = app.get_webview_window(LABEL).and_then(|w| {
        let position = w.outer_position().ok()?;
        let size = w.outer_size().ok()?;
        Some(Placement {
            frame: Rect {
                x: position.x,
                y: position.y,
                width: size.width,
                height: size.height,
            },
            always_on_top: w.is_always_on_top().unwrap_or(false),
            focused: w.is_focused().unwrap_or(false),
            shown: w.is_visible().unwrap_or(false),
        })
    });
    Status {
        shortcut,
        registered,
        problem,
        suggestion,
        visible: quick.visible.load(Ordering::SeqCst),
        window,
    }
}

// ------------------------------------------------------------------ registering

/// Hand the system a shortcut; what it does is `toggle`. Run on a thread of its own: the system calls the handler
/// where the shortcut is heard, and the window is made and shown elsewhere than that.
fn register(app: &AppHandle, shortcut: Shortcut) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            let app = app.clone();
            std::thread::spawn(move || {
                if let Err(error) = toggle(&app) {
                    crate::settings::log(&format!("janela rápida: {error}"));
                }
            });
        })
        .map_err(|e| e.to_string())
}

/// What a person is told when the system refuses a shortcut. The system's own words go to the log.
const REFUSED: &str = "O sistema não liberou esse atalho; em geral outro programa já o usa.";

/// A shortcut free right now, from the list: tried by taking it and giving it back.
fn free_alternative(app: &AppHandle, not: &str) -> Option<String> {
    ALTERNATIVES.iter().filter(|text| **text != not).find_map(|text| {
        let shortcut = parse(text).ok()?;
        let global = app.global_shortcut();
        if global.on_shortcut(shortcut, |_, _, _| {}).is_err() {
            return None;
        }
        let _ = global.unregister(shortcut);
        Some((*text).to_string())
    })
}

/// Ask the system for the shortcut at the start and say how it went. A shortcut it refuses leaves the app as
/// it was, with the reason kept for the page to say.
fn apply(app: &AppHandle, text: &str) {
    let quick = app.state::<Quick>();
    let Ok(_choosing) = quick.choosing.lock() else { return };
    let shortcut = match parse(text) {
        Ok(shortcut) => shortcut,
        Err(why) => {
            crate::settings::log(&format!(
                "janela rápida: atalho {text:?} deixado de lado ({why}); fica o padrão"
            ));
            let Ok(default) = parse(DEFAULT_SHORTCUT) else { return };
            default
        }
    };
    let wanted = canonical(&shortcut);
    let outcome = register(app, shortcut);
    let suggestion = if outcome.is_err() {
        free_alternative(app, &wanted)
    } else {
        None
    };
    let Ok(mut setting) = quick.setting.lock() else { return };
    setting.wanted = wanted;
    match outcome {
        Ok(()) => {
            setting.registered = Some(shortcut);
            setting.problem = None;
            setting.suggestion = None;
            crate::settings::log(&format!("janela rápida: atalho {} registrado", setting.wanted));
        }
        Err(why) => {
            crate::settings::log(&format!(
                "janela rápida: atalho {} não registrado: {why}",
                setting.wanted
            ));
            setting.registered = None;
            setting.problem = Some(REFUSED.into());
            setting.suggestion = suggestion;
        }
    }
}

/// The start: the shortcut the person chose, else the default.
pub fn setup(app: &AppHandle) {
    let chosen = config_file(app).map(|path| read_config(&path)).and_then(|c| c.shortcut);
    apply(app, chosen.as_deref().unwrap_or(DEFAULT_SHORTCUT));
}

/// Change the shortcut. The new one is taken before the old one is let go, so that one the system refuses leaves
/// things as they were.
pub fn set_shortcut(app: &AppHandle, text: &str) -> Result<Status, String> {
    let shortcut = parse(text)?;
    let quick = app.state::<Quick>();
    {
        let _choosing = quick
            .choosing
            .lock()
            .map_err(|_| "o atalho está sendo trocado".to_string())?;
        let held = quick
            .setting
            .lock()
            .map_err(|_| "o atalho está sendo trocado".to_string())?
            .registered;
        if held != Some(shortcut) {
            register(app, shortcut).map_err(|why| {
                crate::settings::log(&format!(
                    "janela rápida: atalho {} recusado: {why}",
                    canonical(&shortcut)
                ));
                REFUSED.to_string()
            })?;
            if let Some(old) = held {
                let _ = app.global_shortcut().unregister(old);
            }
        }
        let mut setting = quick
            .setting
            .lock()
            .map_err(|_| "o atalho está sendo trocado".to_string())?;
        setting.wanted = canonical(&shortcut);
        setting.registered = Some(shortcut);
        setting.problem = None;
        setting.suggestion = None;
        crate::settings::log(&format!("janela rápida: atalho {} registrado", setting.wanted));
    }
    if let Some(path) = config_file(app) {
        let config = Config {
            shortcut: Some(canonical(&shortcut)),
        };
        write_config(&path, &config).map_err(|e| format!("{e:#}"))?;
    }
    Ok(status(app))
}

// ------------------------------------------------------------------ the window

/// The panel, made if it is not there yet: borderless, on top, off the taskbar, hidden until it is shown.
/// Made from a thread that is not the event loop's, which is where a window is safe to make on Windows.
fn window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(LABEL) {
        return Ok(window);
    }
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?quick=1".into()))
        .title("Keep — janela rápida")
        .decorations(false)
        .resizable(false)
        .minimizable(false)
        .maximizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        // The system's shadow is the panel's edge, as on the Mac.
        .shadow(true)
        // On every desktop it is summoned from (the Mac's `canJoinAllSpaces`) where the system has such a thing.
        .visible_on_all_workspaces(true)
        .background_color(GROUND)
        .visible(false)
        .focused(false)
        .build()
        .map_err(|e| format!("não deu para criar a janela rápida: {e}"))
}

/// The screens of the computer, the primary one first.
fn screens(app: &AppHandle) -> Vec<Screen> {
    let rect = |position: &PhysicalPosition<i32>, size: &PhysicalSize<u32>| Rect {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };
    let mut list: Vec<Screen> = app
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| Screen {
            frame: rect(m.position(), m.size()),
            work: rect(&m.work_area().position, &m.work_area().size),
        })
        .collect();
    if let Ok(Some(primary)) = app.primary_monitor() {
        let first = Screen {
            frame: rect(primary.position(), primary.size()),
            work: rect(&primary.work_area().position, &primary.work_area().size),
        };
        list.retain(|s| *s != first);
        list.insert(0, first);
    }
    list
}

fn pointer_screen(app: &AppHandle) -> Result<Screen, String> {
    let list = screens(app);
    let pointer = app.cursor_position().map_err(|e| e.to_string())?;
    screen_at(&list, pointer.x, pointer.y).ok_or_else(|| "sem monitor para a janela rápida".to_string())
}

/// A move of the panel's top edge from `from` to `to`, at the column `x`.
struct Motion {
    x: i32,
    from: i32,
    to: i32,
    duration: Duration,
    curve: Curve,
    /// A panel that goes out ends with its window hidden.
    then_hide: bool,
}

/// Run a move on a thread of its own. A newer one, which bumps `Quick::slide`, ends this one where it has got to.
fn slide(app: AppHandle, window: WebviewWindow, id: u64, motion: Motion) {
    std::thread::spawn(move || {
        let quick = app.state::<Quick>();
        let current = || quick.slide.load(Ordering::SeqCst) == id;
        let began = Instant::now();
        while current() {
            let t = began.elapsed().as_secs_f64() / motion.duration.as_secs_f64();
            if t >= 1.0 {
                break;
            }
            let y = lerp(motion.from, motion.to, motion.curve.at(t));
            let _ = window.set_position(PhysicalPosition::new(motion.x, y));
            std::thread::sleep(STEP);
        }
        if !current() {
            return;
        }
        let _ = window.set_position(PhysicalPosition::new(motion.x, motion.to));
        if motion.then_hide {
            let _ = window.hide();
            let _ = window.emit_to(LABEL, "quick-hidden", ());
        }
    });
}

fn show_now(app: &AppHandle) -> Result<(), String> {
    let quick = app.state::<Quick>();
    let made = app.get_webview_window(LABEL).is_none();
    let window = window(app)?;
    // The first time the page has to load; coming in blank and filling in is worse than a moment more.
    if made {
        quick.wait_for_page();
    }
    let rest = resting(&pointer_screen(app)?);
    // A panel still on its way out turns round where it is, if it is on the same screen.
    let from = match window.outer_position() {
        Ok(at) if window.is_visible().unwrap_or(false) && at.x == rest.x => at.y,
        _ => above(rest),
    };
    let id = quick.slide.fetch_add(1, Ordering::SeqCst) + 1;
    // Before the window can be heard from: the focus it is about to be given is not yet a click away.
    quick.focused.store(false, Ordering::SeqCst);
    quick.visible.store(true, Ordering::SeqCst);
    let fail = |e: tauri::Error| e.to_string();
    window
        .set_size(PhysicalSize::new(rest.width, rest.height))
        .map_err(fail)?;
    window.set_position(PhysicalPosition::new(rest.x, from)).map_err(fail)?;
    window.show().map_err(fail)?;
    // Again after showing: a window shown again can come back on the taskbar, or off the top.
    let _ = window.set_skip_taskbar(true);
    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();
    let _ = window.emit_to(LABEL, "quick-shown", ());
    let motion = Motion {
        x: rest.x,
        from,
        to: rest.y,
        duration: SLIDE_IN,
        curve: Curve::EASE_OUT,
        then_hide: false,
    };
    slide(app.clone(), window, id, motion);
    Ok(())
}

fn hide_now(app: &AppHandle) {
    let quick = app.state::<Quick>();
    quick.visible.store(false, Ordering::SeqCst);
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let id = quick.slide.fetch_add(1, Ordering::SeqCst) + 1;
    match (window.outer_position(), window.outer_size()) {
        (Ok(at), Ok(size)) => {
            let gone = at.y - i32::try_from(size.height).unwrap_or(i32::MAX);
            let motion = Motion {
                x: at.x,
                from: at.y,
                to: gone,
                duration: SLIDE_OUT,
                curve: Curve::EASE_IN,
                then_hide: true,
            };
            slide(app.clone(), window, id, motion);
        }
        _ => {
            let _ = window.hide();
        }
    }
}

/// In if it is out, out if it is in; whether it is in after.
pub fn toggle(app: &AppHandle) -> Result<bool, String> {
    let quick = app.state::<Quick>();
    let _turn = quick
        .turn
        .lock()
        .map_err(|_| "a janela rápida está ocupada".to_string())?;
    if quick.visible.load(Ordering::SeqCst) {
        hide_now(app);
        Ok(false)
    } else {
        show_now(app)?;
        Ok(true)
    }
}

/// Away, if it is in view.
fn dismiss(app: &AppHandle) {
    let quick = app.state::<Quick>();
    let Ok(_turn) = quick.turn.lock() else { return };
    if quick.visible.load(Ordering::SeqCst) {
        hide_now(app);
    }
}

/// The page says it has drawn.
pub fn page_ready(app: &AppHandle) {
    let quick = app.state::<Quick>();
    let Ok(mut ready) = quick.page.lock() else { return };
    *ready = true;
    quick.page_ready.notify_all();
}

// ------------------------------------------------------------------ the app's events

/// Clicking away dismisses — the panel is a visitor, and a visitor that stays after you have turned to something
/// else is in the way. Closing it (Alt+F4) only puts it away: it is reused.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != LABEL {
        return;
    }
    let app = window.app_handle();
    let quick = app.state::<Quick>();
    match event {
        WindowEvent::Focused(true) => quick.focused.store(true, Ordering::SeqCst),
        WindowEvent::Focused(false) => {
            // Not the focus that never came (a system that would not give it) but one that was had and is gone.
            if quick.visible.load(Ordering::SeqCst) && quick.focused.swap(false, Ordering::SeqCst) {
                let app = app.clone();
                std::thread::spawn(move || dismiss(&app));
            }
        }
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let app = app.clone();
            std::thread::spawn(move || dismiss(&app));
        }
        _ => {}
    }
}

/// The app ends with its last real window. The panel, kept hidden between uses, counts to the runtime as a window
/// still open, so the runtime alone would leave a process with nothing to see once the others are gone.
pub fn on_run_event(app: &AppHandle, event: &RunEvent) {
    if let RunEvent::WindowEvent {
        label,
        event: WindowEvent::Destroyed,
        ..
    } = event
    {
        if label != LABEL && app.webview_windows().keys().all(|other| other == LABEL) {
            app.exit(0);
        }
    }
}

// ------------------------------------------------------------------ what the page asks

/// Bring the panel in or put it away (the palette's "Janela rápida"); whether it is in after.
#[tauri::command]
pub async fn quick_toggle(app: AppHandle) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || toggle(&app))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn quick_status(app: AppHandle) -> Status {
    status(&app)
}

/// Choose the shortcut; the state after, or why the system would not.
#[tauri::command]
pub async fn quick_set_shortcut(app: AppHandle, shortcut: String) -> Result<Status, String> {
    tauri::async_runtime::spawn_blocking(move || set_shortcut(&app, &shortcut))
        .await
        .map_err(|e| e.to_string())?
}

/// The quick page has drawn: the first show was waiting for it.
#[tauri::command]
pub fn quick_ready(app: AppHandle) {
    page_ready(&app);
}

/// How long to wait between tries at reading the state while something holds it for a moment.
const LOOK_PAUSES: [Duration; 3] = [
    Duration::from_millis(50),
    Duration::from_millis(100),
    Duration::from_millis(200),
];

/// The state file as a JSON object, with or without a byte order mark; anything else is nothing.
fn parse_state(bytes: &[u8]) -> Option<serde_json::Value> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .filter(serde_json::Value::is_object)
}

fn read_look(path: &Path) -> Option<serde_json::Value> {
    for pause in std::iter::once(Duration::ZERO).chain(LOOK_PAUSES) {
        std::thread::sleep(pause);
        match std::fs::read(path) {
            Ok(bytes) => return parse_state(&bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(_) => {}
        }
    }
    None
}

/// The window's state as it stands on the disk — what the panel wears: the theme, the face, the zoom — read and
/// nothing more. `load_state` also puts a copy back when the file is bad, tells the person so, and stops the
/// saving when it cannot be read; those are for the main window to do when it opens, and a second reader that did
/// them could turn a moment's trouble reading into the main window's silence.
#[tauri::command]
pub async fn quick_look(app: AppHandle) -> Option<serde_json::Value> {
    let path = crate::state_path(&app).ok()?;
    tauri::async_runtime::spawn_blocking(move || read_look(&path))
        .await
        .ok()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect { x, y, width, height }
    }

    /// A 1920x1080 monitor at the origin whose taskbar, at the bottom, takes 48 pixels.
    const MAIN: Screen = Screen {
        frame: rect(0, 0, 1920, 1080),
        work: rect(0, 0, 1920, 1032),
    };
    /// A second monitor to the left, with its taskbar on top.
    const LEFT: Screen = Screen {
        frame: rect(-1280, -100, 1280, 1024),
        work: rect(-1280, -52, 1280, 976),
    };

    #[test]
    fn the_panel_is_the_screens_width_and_42_percent_of_the_work_areas_height() {
        // 1032 * 0.42 = 433.44, rounded as the Mac's `.rounded()`.
        assert_eq!(resting(&MAIN), rect(0, 0, 1920, 433));
        // Hung from the work area's top (under a taskbar on top), at the monitor's own left edge.
        // 976 * 0.42 = 409.92.
        assert_eq!(resting(&LEFT), rect(-1280, -52, 1280, 410));
    }

    #[test]
    fn the_height_rounds_half_up() {
        let screen = Screen {
            frame: rect(0, 0, 100, 100),
            work: rect(0, 0, 100, 50),
        };
        // 50 * 0.42 = 21 exactly, and 25 * 0.42 = 10.5 goes up.
        assert_eq!(resting(&screen).height, 21);
        let screen = Screen {
            frame: rect(0, 0, 100, 100),
            work: rect(0, 0, 100, 25),
        };
        assert_eq!(resting(&screen).height, 11);
    }

    #[test]
    fn it_slides_in_from_one_height_above() {
        let rest = resting(&MAIN);
        assert_eq!(above(rest), -433);
        assert_eq!(above(resting(&LEFT)), -52 - 410);
    }

    #[test]
    fn the_screen_is_the_one_the_pointer_is_on() {
        let both = [MAIN, LEFT];
        assert_eq!(screen_at(&both, 10.0, 10.0), Some(MAIN));
        assert_eq!(screen_at(&both, -5.0, 300.0), Some(LEFT));
        assert_eq!(screen_at(&both, 1919.0, 1079.0), Some(MAIN));
        // The edge between two screens belongs to the one that starts there.
        assert_eq!(screen_at(&both, -1.0, 10.0), Some(LEFT));
        assert_eq!(screen_at(&both, 0.0, 10.0), Some(MAIN));
    }

    #[test]
    fn a_pointer_on_no_screen_goes_to_the_nearest() {
        let both = [MAIN, LEFT];
        // Below the left screen (it ends at y = 924), nearer to it than to the main one.
        assert_eq!(screen_at(&both, -600.0, 1100.0), Some(LEFT));
        // Far to the right of everything.
        assert_eq!(screen_at(&both, 5000.0, 500.0), Some(MAIN));
        assert_eq!(screen_at(&[], 0.0, 0.0), None);
    }

    #[test]
    fn a_slide_is_a_curve_from_one_end_to_the_other() {
        for curve in [Curve::EASE_OUT, Curve::EASE_IN] {
            assert!(curve.at(0.0).abs() < 1e-9);
            assert!((curve.at(1.0) - 1.0).abs() < 1e-9);
            let mut last = 0.0;
            for step in 1..=100 {
                let at = curve.at(f64::from(step) / 100.0);
                assert!(at >= last - 1e-12, "{curve:?} goes back at {step}");
                last = at;
            }
        }
        // Out starts fast and settles; in starts slow and ends fast.
        assert!(Curve::EASE_OUT.at(0.5) > 0.5);
        assert!(Curve::EASE_IN.at(0.5) < 0.5);
        // Outside the time it is held at the ends.
        assert!(Curve::EASE_OUT.at(-1.0).abs() < 1e-9);
        assert!((Curve::EASE_OUT.at(2.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn lerp_goes_from_one_edge_to_the_other() {
        assert_eq!(lerp(-433, 0, 0.0), -433);
        assert_eq!(lerp(-433, 0, 1.0), 0);
        assert_eq!(lerp(-433, 0, 0.5), -216);
        assert_eq!(lerp(0, -433, 1.0), -433);
    }

    #[test]
    fn the_default_is_ctrl_and_backquote() {
        let shortcut = parse(DEFAULT_SHORTCUT).unwrap();
        assert_eq!(shortcut.mods, Modifiers::CONTROL);
        assert_eq!(shortcut.key, Code::Backquote);
        assert_eq!(canonical(&shortcut), DEFAULT_SHORTCUT);
    }

    #[test]
    fn every_alternative_is_a_shortcut_that_reads_back_the_same() {
        for text in ALTERNATIVES {
            let shortcut = parse(text).unwrap_or_else(|why| panic!("{text}: {why}"));
            assert_eq!(canonical(&shortcut), text);
        }
        // And none is the default.
        assert!(!ALTERNATIVES.contains(&DEFAULT_SHORTCUT));
    }

    #[test]
    fn the_modifiers_are_written_in_one_order_whatever_the_text_gave() {
        let shortcut = parse("shift+alt+ctrl+KeyK").unwrap();
        assert_eq!(canonical(&shortcut), "Control+Alt+Shift+KeyK");
        assert_eq!(canonical(&parse("Super+Digit1").unwrap()), "Super+Digit1");
        assert_eq!(
            canonical(&parse("CmdOrCtrl+Space").unwrap()),
            if cfg!(target_os = "macos") {
                "Super+Space"
            } else {
                "Control+Space"
            }
        );
    }

    #[test]
    fn a_shortcut_needs_ctrl_alt_or_win_unless_it_is_a_function_key() {
        assert!(parse("KeyK").is_err());
        assert!(parse("Shift+KeyK").is_err());
        assert!(parse("Backquote").is_err());
        assert!(parse("Control+KeyK").is_ok());
        assert!(parse("Alt+Backquote").is_ok());
        assert!(parse("Super+KeyK").is_ok());
        assert!(parse("F12").is_ok());
        assert!(parse("Shift+F9").is_ok());
        assert!(parse("Control+Escape").is_err());
    }

    #[test]
    fn what_is_not_a_shortcut_is_refused_in_words() {
        for text in [
            "",
            "Control+",
            "Control+Alt",
            "Control+NoSuchKey",
            "Control+KeyA+KeyB",
            "+",
        ] {
            let why = parse(text).unwrap_err();
            assert!(why.starts_with("não entendi"), "{text:?}: {why}");
        }
    }

    #[test]
    fn the_choice_is_kept_whole_and_a_bad_file_reads_as_no_choice() {
        let dir = std::env::temp_dir().join(format!("keep-quick-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("quick.json");
        assert!(read_config(&file).shortcut.is_none());
        write_config(
            &file,
            &Config {
                shortcut: Some("Control+Alt+KeyK".into()),
            },
        )
        .unwrap();
        assert_eq!(read_config(&file).shortcut.as_deref(), Some("Control+Alt+KeyK"));
        assert!(!dir.join("quick.json.tmp").exists());
        std::fs::write(&file, "isto não é json").unwrap();
        assert!(read_config(&file).shortcut.is_none());
        std::fs::write(&file, "{}").unwrap();
        assert!(read_config(&file).shortcut.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_state_is_read_as_it_is_and_never_touched() {
        let dir = std::env::temp_dir().join(format!("keep-quick-look-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("state.json");
        assert_eq!(read_look(&file), None, "no file yet");
        std::fs::write(&file, b"\xEF\xBB\xBF{\"zoom\":1.25}").unwrap();
        assert_eq!(read_look(&file).unwrap()["zoom"], 1.25);
        // A file that is not a state is nothing, and is left where it is: no copy put back, no file kept aside.
        std::fs::write(&file, "isto não é json").unwrap();
        assert_eq!(read_look(&file), None);
        std::fs::write(&file, "[1, 2]").unwrap();
        assert_eq!(read_look(&file), None);
        let mut names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        names.sort();
        assert_eq!(names, vec![std::ffi::OsString::from("state.json")]);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "[1, 2]");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_status_is_said_to_the_page_in_camel_case() {
        let status = Status {
            shortcut: "Control+Backquote".into(),
            registered: false,
            problem: Some("x".into()),
            suggestion: None,
            visible: false,
            window: Some(Placement {
                frame: rect(0, 0, 10, 4),
                always_on_top: true,
                focused: false,
                shown: true,
            }),
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["window"]["alwaysOnTop"], true);
        assert_eq!(json["window"]["frame"]["width"], 10);
        assert_eq!(json["problem"], "x");
        assert!(json["suggestion"].is_null());
    }
}
