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
/// How many times, and how far apart, the panel asks again for the keyboard when the system has not given it.
const FOCUS_TRIES: u32 = 4;
const FOCUS_EVERY: Duration = Duration::from_millis(80);
/// The window's colour before the page has painted: Keep's dark ground, as the main window's.
const GROUND: Color = Color(0x28, 0x2c, 0x34, 0xff);

/// A line in the app's log about the panel, for what is worth finding there afterwards.
fn note(line: &str) {
    crate::settings::log(&format!("janela rápida: {line}"));
}

/// The same, only while somebody is looking into how the panel moves: one line a step would fill the log.
fn trace(line: &str) {
    if std::env::var_os("KEEP_QUICK_TRACE").is_some() {
        note(line);
    }
}

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

// ------------------------------------------------------------------ the key where it was seen

/// The scancode — what every PC keyboard sends from a key, whatever the layout — of the keys whose virtual key is
/// the layout's to say: the letters, the digits and the punctuation. The rest (the function keys, the arrows, the
/// space) are one virtual key on every layout.
fn scancode(code: Code) -> Option<u32> {
    Some(match code {
        Code::Backquote => 0x29,
        Code::Minus => 0x0C,
        Code::Equal => 0x0D,
        Code::BracketLeft => 0x1A,
        Code::BracketRight => 0x1B,
        Code::Backslash => 0x2B,
        Code::Semicolon => 0x27,
        Code::Quote => 0x28,
        Code::Comma => 0x33,
        Code::Period => 0x34,
        Code::Slash => 0x35,
        Code::Digit1 => 0x02,
        Code::Digit2 => 0x03,
        Code::Digit3 => 0x04,
        Code::Digit4 => 0x05,
        Code::Digit5 => 0x06,
        Code::Digit6 => 0x07,
        Code::Digit7 => 0x08,
        Code::Digit8 => 0x09,
        Code::Digit9 => 0x0A,
        Code::Digit0 => 0x0B,
        Code::KeyQ => 0x10,
        Code::KeyW => 0x11,
        Code::KeyE => 0x12,
        Code::KeyR => 0x13,
        Code::KeyT => 0x14,
        Code::KeyY => 0x15,
        Code::KeyU => 0x16,
        Code::KeyI => 0x17,
        Code::KeyO => 0x18,
        Code::KeyP => 0x19,
        Code::KeyA => 0x1E,
        Code::KeyS => 0x1F,
        Code::KeyD => 0x20,
        Code::KeyF => 0x21,
        Code::KeyG => 0x22,
        Code::KeyH => 0x23,
        Code::KeyJ => 0x24,
        Code::KeyK => 0x25,
        Code::KeyL => 0x26,
        Code::KeyZ => 0x2C,
        Code::KeyX => 0x2D,
        Code::KeyC => 0x2E,
        Code::KeyV => 0x2F,
        Code::KeyB => 0x30,
        Code::KeyN => 0x31,
        Code::KeyM => 0x32,
        _ => return None,
    })
}

const DIGITS: [Code; 10] = [
    Code::Digit0,
    Code::Digit1,
    Code::Digit2,
    Code::Digit3,
    Code::Digit4,
    Code::Digit5,
    Code::Digit6,
    Code::Digit7,
    Code::Digit8,
    Code::Digit9,
];

const LETTERS: [Code; 26] = [
    Code::KeyA,
    Code::KeyB,
    Code::KeyC,
    Code::KeyD,
    Code::KeyE,
    Code::KeyF,
    Code::KeyG,
    Code::KeyH,
    Code::KeyI,
    Code::KeyJ,
    Code::KeyK,
    Code::KeyL,
    Code::KeyM,
    Code::KeyN,
    Code::KeyO,
    Code::KeyP,
    Code::KeyQ,
    Code::KeyR,
    Code::KeyS,
    Code::KeyT,
    Code::KeyU,
    Code::KeyV,
    Code::KeyW,
    Code::KeyX,
    Code::KeyY,
    Code::KeyZ,
];

/// The key `global-hotkey` registers as a given Windows virtual key. Its table is the US layout's: `Backquote` is
/// `VK_OEM_3`, `KeyQ` is `VK_Q`, whichever keys those are on the keyboard at hand.
fn code_of_virtual_key(vk: u32) -> Option<Code> {
    Some(match vk {
        0x30..=0x39 => DIGITS[(vk - 0x30) as usize],
        0x41..=0x5A => LETTERS[(vk - 0x41) as usize],
        0xBA => Code::Semicolon,
        0xBB => Code::Equal,
        0xBC => Code::Comma,
        0xBD => Code::Minus,
        0xBE => Code::Period,
        0xBF => Code::Slash,
        0xC0 => Code::Backquote,
        0xDB => Code::BracketLeft,
        0xDC => Code::Backslash,
        0xDD => Code::BracketRight,
        0xDE => Code::Quote,
        _ => return None,
    })
}

/// The shortcut as the layout in use has its key. What the page records is the key's place on the keyboard (the
/// Mac's `kVK_ANSI_Grave` is the same: the place, not the character), and Windows registers a virtual key, which is
/// the layout's: the key left of the 1 is `VK_OEM_3` on a US keyboard and another on a German one, and the key that
/// `global-hotkey`'s table calls `Backquote` is the Ö of a German keyboard. So the place is turned into the virtual
/// key the layout gives it (`virtual_key_at`, by scancode) and that into the key of the table that stands for it.
/// A key the layout gives no virtual key the table knows is left as it was.
fn on_layout(shortcut: Shortcut, virtual_key_at: impl Fn(u32) -> Option<u32>) -> Shortcut {
    let key = scancode(shortcut.key)
        .and_then(virtual_key_at)
        .and_then(code_of_virtual_key)
        .unwrap_or(shortcut.key);
    if key == shortcut.key {
        shortcut
    } else {
        Shortcut::new(Some(shortcut.mods), key)
    }
}

/// The virtual key the keyboard layout in use gives to a scancode.
#[cfg(windows)]
fn virtual_key_at(scancode: u32) -> Option<u32> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MapVirtualKeyW, MAPVK_VSC_TO_VK};
    // SAFETY: a plain query of the system with two numbers.
    let vk = unsafe { MapVirtualKeyW(scancode, MAPVK_VSC_TO_VK) };
    (vk != 0).then_some(vk)
}

/// Where the keys are registered by the key's place already (the Mac's), there is nothing to turn.
#[cfg(not(windows))]
fn virtual_key_at(_scancode: u32) -> Option<u32> {
    None
}

/// What the system is handed for a shortcut.
fn registerable(shortcut: Shortcut) -> Shortcut {
    on_layout(shortcut, virtual_key_at)
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
    /// Where the panel was last sent to rest, for the scripted check to compare with where it is.
    rest: Mutex<Option<Rect>>,
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
    /// The panel as it is drawn: the part of the window that has content.
    pub frame: Rect,
    /// The window itself, which on Windows is larger than what it draws (see `Margins`).
    pub outer: Rect,
    /// Where it was last sent to rest: the frame, once it has got there.
    pub rest: Option<Rect>,
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
        let rect = |at: PhysicalPosition<i32>, size: PhysicalSize<u32>| Rect {
            x: at.x,
            y: at.y,
            width: size.width,
            height: size.height,
        };
        Some(Placement {
            frame: rect(w.inner_position().ok()?, w.inner_size().ok()?),
            outer: rect(w.outer_position().ok()?, w.outer_size().ok()?),
            rest: quick.rest.lock().ok().and_then(|rest| *rest),
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
        let shortcut = registerable(parse(text).ok()?);
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
    let shortcut = registerable(shortcut);
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
    let chosen = parse(text)?;
    let shortcut = registerable(chosen);
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
                crate::settings::log(&format!("janela rápida: atalho {} recusado: {why}", canonical(&chosen)));
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
        setting.wanted = canonical(&chosen);
        setting.registered = Some(shortcut);
        setting.problem = None;
        setting.suggestion = None;
        crate::settings::log(&format!("janela rápida: atalho {} registrado", setting.wanted));
    }
    if let Some(path) = config_file(app) {
        let config = Config {
            shortcut: Some(canonical(&chosen)),
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
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?quick=1".into()))
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
        .map_err(|e| format!("não deu para criar a janela rápida: {e}"))?;
    soften_the_frame(&window);
    Ok(window)
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

// ------------------------------------------------------------------ what only Windows does

/// The system's frame around a borderless window with a shadow (which is how the panel gets its edge, as on the
/// Mac): Windows 11 rounds its corners and draws a hairline round it. The Mac's panel is flush with the top of the
/// screen, which a rounded top corner would leave a gap at, and the shadow is edge enough: no rounding, no hairline.
/// Where the system does not know these (before Windows 11) it says no, which is no matter.
#[cfg(windows)]
fn soften_the_frame(window: &WebviewWindow) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
    };
    let Ok(hwnd) = window.hwnd() else { return };
    let size = std::mem::size_of::<u32>() as u32;
    let corners = DWMWCP_DONOTROUND as u32;
    let border = DWMWA_COLOR_NONE;
    // SAFETY: the handle is the panel's own, and each value is a `u32` of the size said.
    unsafe {
        DwmSetWindowAttribute(
            hwnd.0,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&raw const corners).cast(),
            size,
        );
        DwmSetWindowAttribute(hwnd.0, DWMWA_BORDER_COLOR as u32, (&raw const border).cast(), size);
    }
}

#[cfg(not(windows))]
fn soften_the_frame(_window: &WebviewWindow) {}

/// The panel on the virtual desktop the person is on (the Mac's `canJoinAllSpaces`). A window belongs to one
/// desktop, the one it was made on; shown again from another it would come up there, out of sight, and the keyboard
/// shortcut would seem to do nothing. The person is where the window in front is, so the panel is moved to that
/// window's desktop. Nothing happens, and nothing is lost, where there is no such thing (one desktop, an older
/// Windows) or the window in front is the shell's own.
#[cfg(windows)]
fn follow_the_desktop(window: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let Ok(own) = window.hwnd() else { return };
    // SAFETY: calls into the system with handles that came from it. COM is started on this thread for the call, and
    // the object is let go of before it is stopped.
    unsafe {
        // Started already on this thread, in another way, is no reason not to go on: only what was started here is stopped.
        let started = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let moved = (|| -> windows::core::Result<()> {
            let manager: IVirtualDesktopManager = CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL)?;
            let desktop = manager.GetWindowDesktopId(HWND(GetForegroundWindow()))?;
            manager.MoveWindowToDesktop(own, &desktop)
        })();
        if let Err(error) = moved {
            trace(&format!("não levou a janela para a área de trabalho em uso: {error}"));
        }
        if started {
            CoUninitialize();
        }
    }
}

#[cfg(not(windows))]
fn follow_the_desktop(_window: &WebviewWindow) {}

/// How far the window reaches beyond what it draws, at the left and the top. A borderless window with a shadow is, on
/// Windows, inset in the window the system gives it: the client area is smaller than the window by the thickness
/// of the frame (the invisible border every window has, which the shadow is drawn from), so the window is wider
/// than the panel and a window put at the screen's edge draws a gap of that thickness. What is placed is therefore
/// what is drawn, and the window is put where that lands it. Measured on the window, which knows it for the
/// screen it is on (the thickness follows the pixel density); nothing on the Mac, where the two are one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
struct Margins {
    left: i32,
    top: i32,
}

impl Margins {
    fn of(window: &WebviewWindow) -> Margins {
        match (window.inner_position(), window.outer_position()) {
            (Ok(inner), Ok(outer)) => Margins {
                left: inner.x - outer.x,
                top: inner.y - outer.y,
            },
            _ => Margins::default(),
        }
    }

    /// Where the window goes to have its drawn part at `(x, y)`.
    fn window_at(self, x: i32, y: i32) -> (i32, i32) {
        (x - self.left, y - self.top)
    }
}

/// Put the panel's drawn part at a place and a size, and say how the window sits round it. The window goes to the
/// place first, to be on the screen whose pixel density the size is made at (a window moved to another density is
/// resized by the system to keep its logical size, and it is the physical size asked for that has to stay); then
/// the size, whose inner part is the panel; then the window is moved by what is found of its margins.
fn place(window: &WebviewWindow, x: i32, y: i32, size: Rect) -> Result<Margins, String> {
    let fail = |e: tauri::Error| e.to_string();
    window.set_position(PhysicalPosition::new(x, y)).map_err(fail)?;
    window
        .set_size(PhysicalSize::new(size.width, size.height))
        .map_err(fail)?;
    let margins = Margins::of(window);
    let (wx, wy) = margins.window_at(x, y);
    window.set_position(PhysicalPosition::new(wx, wy)).map_err(fail)?;
    Ok(margins)
}

/// A move of the window's top edge from `from` to `to`, at the column `x` (the window's own coordinates).
struct Motion {
    x: i32,
    from: i32,
    to: i32,
    duration: Duration,
    curve: Curve,
    /// Where the panel's drawn part is to be once it has got there, size included: a screen of another density on
    /// the way may have changed it.
    rest: Option<Rect>,
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
            trace(&format!("passo t={t:.2} y={y}"));
            let _ = window.set_position(PhysicalPosition::new(motion.x, y));
            std::thread::sleep(STEP);
        }
        if !current() {
            return;
        }
        match motion.rest {
            Some(rest) => drop(place(&window, rest.x, rest.y, rest)),
            None => drop(window.set_position(PhysicalPosition::new(motion.x, motion.to))),
        }
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
    let mut rest = resting(&pointer_screen(app)?);
    // The first time the page has to load, and it lays itself out at the size it finds: so it finds the panel's,
    // and the panel does not come in blank and then fill in — which is worse than a moment more.
    if made {
        place(&window, rest.x, above(rest), rest)?;
        let asked = Instant::now();
        quick.wait_for_page();
        note(&format!(
            "aberta pela primeira vez; a página ficou pronta em {} ms",
            asked.elapsed().as_millis()
        ));
        // The pointer may have gone to another screen meanwhile.
        rest = resting(&pointer_screen(app)?);
    }
    // A panel still on its way out turns round where it is, if it is on the same screen.
    let current = Margins::of(&window);
    let from = match window.outer_position() {
        Ok(at) if window.is_visible().unwrap_or(false) && at.x + current.left == rest.x => at.y + current.top,
        _ => above(rest),
    };
    let id = quick.slide.fetch_add(1, Ordering::SeqCst) + 1;
    // Before the window can be heard from: the focus it is about to be given is not yet a click away.
    quick.focused.store(false, Ordering::SeqCst);
    if let Ok(mut last) = quick.rest.lock() {
        *last = Some(rest);
    }
    let margins = place(&window, rest.x, from, rest)?;
    follow_the_desktop(&window);
    // Off the taskbar before it is shown, and again after: Windows puts the button back when a window is shown.
    let _ = window.set_skip_taskbar(true);
    window.show().map_err(|e| e.to_string())?;
    // In view only once it is: a showing that failed leaves the next press to try again, not to put away what is not there.
    quick.visible.store(true, Ordering::SeqCst);
    let _ = window.set_skip_taskbar(true);
    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();
    trace(&format!(
        "entra em {rest:?}; com o foco logo depois: {}",
        window.is_focused().unwrap_or(false)
    ));
    let _ = window.emit_to(LABEL, "quick-shown", ());
    ask_again_for_focus(app.clone(), window.clone(), id);
    let (x, from) = margins.window_at(rest.x, from);
    let motion = Motion {
        x,
        from,
        to: rest.y - margins.top,
        duration: SLIDE_IN,
        curve: Curve::EASE_OUT,
        rest: Some(rest),
        then_hide: false,
    };
    slide(app.clone(), window, id, motion);
    Ok(())
}

/// A system that is slow to give the keyboard — Windows will not hand it to a process that was not the last to be
/// given input — is asked again a few times, rather than leaving a panel in view that cannot be typed into.
fn ask_again_for_focus(app: AppHandle, window: WebviewWindow, id: u64) {
    std::thread::spawn(move || {
        let quick = app.state::<Quick>();
        for _ in 0..FOCUS_TRIES {
            std::thread::sleep(FOCUS_EVERY);
            // Given, or the panel has been shown or put away since: nothing to ask.
            if quick.focused.load(Ordering::SeqCst) || quick.slide.load(Ordering::SeqCst) != id {
                return;
            }
            trace("pede o foco de novo");
            let _ = window.set_focus();
        }
    });
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
                rest: None,
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
    trace(&format!("alternar: em vista {}", quick.visible.load(Ordering::SeqCst)));
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
    trace(&format!("dispensar: em vista {}", quick.visible.load(Ordering::SeqCst)));
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
        WindowEvent::Focused(true) => {
            trace("recebeu o foco");
            quick.focused.store(true, Ordering::SeqCst);
        }
        WindowEvent::Focused(false) => {
            trace(&format!(
                "perdeu o foco (em vista: {}, tinha o foco: {})",
                quick.visible.load(Ordering::SeqCst),
                quick.focused.load(Ordering::SeqCst)
            ));
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

    /// A layout as the virtual keys it gives to scancodes, the rest as the US layout has them.
    fn layout(changes: &'static [(u32, u32)]) -> impl Fn(u32) -> Option<u32> {
        move |sc| {
            if let Some((_, vk)) = changes.iter().find(|(s, _)| *s == sc) {
                return Some(*vk);
            }
            // The US layout, for the keys these tests ask about.
            Some(match sc {
                0x29 => 0xC0,
                0x15 => 0x59,
                0x2C => 0x5A,
                0x10 => 0x51,
                0x1E => 0x41,
                0x02 => 0x31,
                0x0C => 0xBD,
                _ => return None,
            })
        }
    }

    #[test]
    fn on_the_layout_the_default_is_the_key_left_of_the_one() {
        let default = parse(DEFAULT_SHORTCUT).unwrap();
        // US, and ABNT2 as far as is known: the key at that place is the table's `Backquote`.
        assert_eq!(on_layout(default, layout(&[])), default);
        // German: the key at that place is `VK_OEM_5`, which the table calls `Backslash`; `Backquote` there would
        // be the Ö.
        let german = on_layout(default, layout(&[(0x29, 0xDC)]));
        assert_eq!(canonical(&german), "Control+Backslash");
    }

    #[test]
    fn on_the_layout_a_letter_is_the_one_at_its_place_not_the_one_it_types() {
        let q = parse("Alt+KeyQ").unwrap();
        // AZERTY: the key at the Q's place types A.
        let azerty = layout(&[(0x10, 0x41), (0x1E, 0x51)]);
        assert_eq!(canonical(&on_layout(q, &azerty)), "Alt+KeyA");
        assert_eq!(canonical(&on_layout(parse("Alt+KeyA").unwrap(), &azerty)), "Alt+KeyQ");
        // QWERTZ: Y and Z trade places.
        let qwertz = layout(&[(0x15, 0x5A), (0x2C, 0x59)]);
        assert_eq!(
            canonical(&on_layout(parse("Control+KeyY").unwrap(), &qwertz)),
            "Control+KeyZ"
        );
        assert_eq!(
            canonical(&on_layout(parse("Control+KeyZ").unwrap(), &qwertz)),
            "Control+KeyY"
        );
    }

    #[test]
    fn on_the_layout_the_modifiers_and_the_unremapped_keys_stay() {
        let by_layout = layout(&[(0x29, 0xDC)]);
        let shortcut = parse("Control+Alt+Shift+Super+Backquote").unwrap();
        let turned = on_layout(shortcut, &by_layout);
        assert_eq!(turned.mods, shortcut.mods);
        // Function keys, the space and the arrows are one virtual key on every layout.
        for text in ["F12", "Control+Space", "Alt+ArrowUp", "Control+Enter"] {
            let shortcut = parse(text).unwrap();
            assert_eq!(on_layout(shortcut, &by_layout), shortcut, "{text}");
        }
        // A key the layout does not say, or says as one the table has no key for, is left as it was.
        let digit = parse("Control+Digit2").unwrap();
        assert_eq!(on_layout(digit, layout(&[])), digit);
        let odd = parse("Control+Backquote").unwrap();
        assert_eq!(on_layout(odd, layout(&[(0x29, 0xDF)])), odd);
        assert_eq!(on_layout(odd, |_| None), odd);
    }

    #[test]
    fn every_key_that_has_a_scancode_comes_back_as_itself_on_the_us_layout() {
        // The two tables are inverse where the US layout has them: no key is turned into another by a layout that
        // changes nothing.
        let us = |sc: u32| {
            [
                Code::Backquote,
                Code::Minus,
                Code::Equal,
                Code::BracketLeft,
                Code::BracketRight,
                Code::Backslash,
            ]
            .into_iter()
            .chain([Code::Semicolon, Code::Quote, Code::Comma, Code::Period, Code::Slash])
            .chain(DIGITS)
            .chain(LETTERS)
            .find(|code| scancode(*code) == Some(sc))
            .and_then(|code| {
                Some(match code {
                    Code::Backquote => 0xC0,
                    Code::Minus => 0xBD,
                    Code::Equal => 0xBB,
                    Code::BracketLeft => 0xDB,
                    Code::BracketRight => 0xDD,
                    Code::Backslash => 0xDC,
                    Code::Semicolon => 0xBA,
                    Code::Quote => 0xDE,
                    Code::Comma => 0xBC,
                    Code::Period => 0xBE,
                    Code::Slash => 0xBF,
                    digit if DIGITS.contains(&digit) => 0x30 + DIGITS.iter().position(|d| *d == digit)? as u32,
                    letter => 0x41 + LETTERS.iter().position(|l| *l == letter)? as u32,
                })
            })
        };
        for code in [
            Code::Backquote,
            Code::Slash,
            Code::Quote,
            Code::Digit0,
            Code::Digit7,
            Code::KeyA,
            Code::KeyZ,
            Code::KeyM,
        ] {
            let shortcut = Shortcut::new(Some(Modifiers::CONTROL), code);
            assert_eq!(on_layout(shortcut, us), shortcut, "{code}");
        }
    }

    #[test]
    fn no_two_keys_share_a_scancode() {
        let mut seen = std::collections::HashSet::new();
        for code in [
            Code::Backquote,
            Code::Minus,
            Code::Equal,
            Code::BracketLeft,
            Code::BracketRight,
            Code::Backslash,
        ]
        .into_iter()
        .chain([Code::Semicolon, Code::Quote, Code::Comma, Code::Period, Code::Slash])
        .chain(DIGITS)
        .chain(LETTERS)
        {
            let sc = scancode(code).unwrap_or_else(|| panic!("{code} sem scancode"));
            assert!(seen.insert(sc), "{code} repete o scancode {sc:#x}");
        }
        assert_eq!(seen.len(), 11 + 10 + 26);
    }

    #[test]
    fn the_window_goes_where_the_drawn_part_is_to_land() {
        // Windows 11 at 100%: the frame is 8 pixels at the sides and below, and 1 above.
        let margins = Margins { left: 8, top: 1 };
        assert_eq!(margins.window_at(0, 0), (-8, -1));
        assert_eq!(margins.window_at(-1280, -433), (-1288, -434));
        // Nothing round it, as on the Mac: the window is the panel.
        assert_eq!(Margins::default().window_at(40, 50), (40, 50));
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
                outer: rect(-8, -1, 26, 13),
                rest: Some(rect(0, 0, 10, 6)),
                always_on_top: true,
                focused: false,
                shown: true,
            }),
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["window"]["alwaysOnTop"], true);
        assert_eq!(json["window"]["frame"]["width"], 10);
        assert_eq!(json["window"]["rest"]["height"], 6);
        assert_eq!(json["window"]["outer"]["x"], -8);
        assert_eq!(json["problem"], "x");
        assert!(json["suggestion"].is_null());
    }
}
