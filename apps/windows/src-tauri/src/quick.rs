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
//!
//! The pieces that are plain numbers and text are apart, so that they are tested
//! without a window: `geometry` (the screen the pointer is on, where the panel
//! rests, the curves of the slide) and `shortcut` (what text is a shortcut, how
//! it is spelled, which key of the layout in use it is).

use std::path::{Path, PathBuf};
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
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

mod geometry;
mod shortcut;

use geometry::{above, lerp, resting, screen_at, Curve, Rect, Screen};
use shortcut::{canonical, parse, registerable, ALTERNATIVES, DEFAULT_SHORTCUT};

/// The window's label, and the name the capability (capabilities/quick.json) gives its permissions to.
pub const LABEL: &str = "quick";

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
