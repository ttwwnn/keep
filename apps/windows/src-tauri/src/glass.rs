//! The translucent ground: the window that lets the desktop through, and the material behind it.
//!
//! The Mac gets both from Ghostty's `background-opacity` and `background-blur` in the user's config. Here they are
//! a setting (`glass` in the state, src/glass.ts), off by default, and what the system needs for them is decided
//! here:
//!
//! - A window is made transparent or not when it is made, and cannot be changed after (tao draws a transparent
//!   window through DWM's blur-behind and a surface of its own, which a window made opaque does not have). It is
//!   made transparent only when the state asks for a translucent ground, so that nobody who never turns it on
//!   gets a window different from the one they had. Turning it on in a running app therefore asks for the app to
//!   be opened again (`restart_app`): the daemon and the tabs are not the app's, and stay.
//! - The material (Mica, acrylic) is applied at run time to the windows that were made transparent, and
//!   cleared when it is not wanted (`glass_apply`). The page says which, from the state and the palette.
//! - The drop-down terminal's panel is always transparent on Windows, for its rounded corners (quick.rs), and
//!   wears the material too.
//!
//! What only Windows does is in the small functions at the end, with an empty twin elsewhere.

use std::collections::HashSet;
use std::sync::Mutex;

use serde_json::Value;
use tauri::utils::config::WindowEffectsConfig;
use tauri::window::{Effect, EffectsBuilder};
use tauri::{AppHandle, Manager, WebviewWindow, WebviewWindowBuilder};

/// The first Windows build with the acrylic window-vibrancy applies (1809), and the first of Windows 11 22H2,
/// which has the system backdrops (Mica). The page offers by the same numbers (src/glass.ts).
const ACRYLIC_BUILD: u32 = 17763;
const MICA_BUILD: u32 = 22621;

/// What is put behind a translucent window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blur {
    None,
    Mica,
    Acrylic,
}

impl Blur {
    /// As the page says it; anything else is none.
    pub fn parse(text: &str) -> Blur {
        match text {
            "mica" => Blur::Mica,
            "acrylic" => Blur::Acrylic,
            _ => Blur::None,
        }
    }

    /// The material this system has: the one asked for, or none.
    pub fn supported(self, build: u32) -> Blur {
        match self {
            Blur::Mica if build >= MICA_BUILD => Blur::Mica,
            Blur::Acrylic if build >= ACRYLIC_BUILD => Blur::Acrylic,
            _ => Blur::None,
        }
    }

    /// The effect that makes it, for a ground that is dark or light (Mica takes the colour of the theme it is
    /// told of, not of the system's). None for no material.
    fn effect(self, dark: bool) -> Option<Effect> {
        match self {
            Blur::None => None,
            Blur::Mica if dark => Some(Effect::MicaDark),
            Blur::Mica => Some(Effect::MicaLight),
            Blur::Acrylic => Some(Effect::Acrylic),
        }
    }
}

/// Whether the state asks for a ground that lets the desktop through: an opacity under the whole.
pub fn wants_translucency(state: Option<&Value>) -> bool {
    state
        .and_then(|s| s.get("glass"))
        .and_then(|g| g.get("opacity"))
        .and_then(Value::as_f64)
        .is_some_and(|opacity| opacity.is_finite() && opacity < 1.0)
}

/// What is known of the windows' glass: which were made transparent, and what material they wear now.
#[derive(Default)]
pub struct Glass {
    transparent: Mutex<HashSet<String>>,
    /// The material and ground last applied; none until the page first says (an app that never asks touches nothing).
    applied: Mutex<Option<(Blur, bool)>>,
}

impl Glass {
    fn remember(&self, label: &str, transparent: bool) {
        if let Ok(mut set) = self.transparent.lock() {
            if transparent {
                set.insert(label.to_string());
            } else {
                set.remove(label);
            }
        }
    }

    pub fn is_transparent(&self, label: &str) -> bool {
        self.transparent.lock().is_ok_and(|set| set.contains(label))
    }

    fn labels(&self) -> Vec<String> {
        self.transparent.lock().map(|set| set.iter().cloned().collect()).unwrap_or_default()
    }
}

/// Whether the windows of the app are made transparent in this run: only on Windows (the Mac's `transparent` is a
/// private API, and its translucency is Ghostty's own), and only when the state asks for it.
pub fn made_transparent(app: &AppHandle) -> bool {
    cfg!(windows) && wants_translucency(crate::windows::look(app).as_ref())
}

/// Make the first window, which the configuration leaves to this (`create: false`): its configuration, and
/// transparent if the state asks for a translucent ground. The others follow it (`windows::build`).
pub fn open_main(app: &AppHandle) -> tauri::Result<()> {
    let Some(config) = app.config().app.windows.first().cloned() else { return Ok(()) };
    let transparent = made_transparent(app);
    app.state::<Glass>().remember(&config.label, transparent);
    WebviewWindowBuilder::from_config(app, &config)?.transparent(transparent).build()?;
    Ok(())
}

/// Whether a window of the app made after the first is transparent: as the first was, so that the app is of one
/// kind. Remembered under the label it is given.
pub fn made_like_main(app: &AppHandle, label: &str) -> bool {
    let glass = app.state::<Glass>();
    let transparent = glass.is_transparent(crate::windows::MAIN);
    glass.remember(label, transparent);
    transparent
}

/// Put on a window the material the others wear, if any: a window made after the page said what it wanted.
pub fn wear(app: &AppHandle, window: &WebviewWindow) {
    let glass = app.state::<Glass>();
    let Some((blur, dark)) = glass.applied.lock().ok().and_then(|a| *a) else { return };
    if glass.is_transparent(window.label()) {
        put(window, blur, dark);
    }
}

fn put(window: &WebviewWindow, blur: Blur, dark: bool) {
    // The old material is cleared first: applying another over it leaves both.
    let _ = window.set_effects(Option::<WindowEffectsConfig>::None);
    if let Some(effect) = blur.effect(dark) {
        let _ = window.set_effects(EffectsBuilder::new().effect(effect).build());
    }
}

/// Apply a material to every window that was made transparent; a repeat of what is on is no work, and an app
/// that has never worn one is not asked to clear one.
pub fn apply(app: &AppHandle, blur: Blur, dark: bool) {
    let glass = app.state::<Glass>();
    let blur = blur.supported(crate::settings::windows_build());
    {
        let Ok(mut applied) = glass.applied.lock() else { return };
        let before = *applied;
        if before == Some((blur, dark)) || (before.is_none() && blur == Blur::None) {
            return;
        }
        *applied = Some((blur, dark));
    }
    for label in glass.labels() {
        if let Some(window) = app.get_webview_window(&label) {
            put(&window, blur, dark);
        }
    }
}

/// What the page says about the material behind the window, and whether the ground over it is dark.
#[tauri::command]
pub fn glass_apply(app: AppHandle, blur: String, dark: bool) {
    apply(&app, Blur::parse(&blur), dark);
}

/// The environment variable that tells a new run which one it replaces.
const AFTER: &str = "KEEP_AFTER_PID";

/// Open the app again, as the window is only made transparent when it is made. The new run waits for this one to
/// be gone before it starts (the single-instance guard would send it away otherwise); this one leaves by the way
/// the app always leaves, which lets go of the tabs and keeps the state.
#[tauri::command]
pub fn restart_app(app: AppHandle) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("não achei o executável do Keep: {e}"))?;
    std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env(AFTER, std::process::id().to_string())
        .spawn()
        .map_err(|e| format!("não consegui abrir o Keep de novo: {e}"))?;
    app.exit(0);
    Ok(())
}

/// The process a run replaces, from the variable `restart_app` sets.
fn predecessor(value: Option<&str>) -> Option<u32> {
    value.and_then(|v| v.trim().parse::<u32>().ok()).filter(|pid| *pid != 0 && *pid != std::process::id())
}

/// Before the app starts: wait for the run it replaces to be gone, and forget it was told of it (the shells of
/// the daemon are not the app's, but a variable of the environment reaches whatever the app starts).
pub fn wait_for_predecessor() {
    let value = std::env::var(AFTER).ok();
    std::env::remove_var(AFTER);
    if let Some(pid) = predecessor(value.as_deref()) {
        wait_for_exit(pid);
    }
}

/// Until the process is gone, or ten seconds: an app that will not leave is no reason for this one not to start.
#[cfg(windows)]
fn wait_for_exit(pid: u32) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
    // SAFETY: a handle opened here for the one right asked, waited on and closed here.
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        // Not opened: the process is gone already, or is not ours to wait for.
        if handle.is_null() {
            return;
        }
        WaitForSingleObject(handle, 10_000);
        CloseHandle(handle);
    }
}

#[cfg(not(windows))]
fn wait_for_exit(_pid: u32) {
    std::thread::sleep(std::time::Duration::from_millis(800));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_material_is_read_as_the_page_says_it() {
        assert_eq!(Blur::parse("mica"), Blur::Mica);
        assert_eq!(Blur::parse("acrylic"), Blur::Acrylic);
        assert_eq!(Blur::parse("none"), Blur::None);
        assert_eq!(Blur::parse("vibrancy"), Blur::None);
        assert_eq!(Blur::parse(""), Blur::None);
    }

    #[test]
    fn the_system_gives_the_material_by_its_build() {
        assert_eq!(Blur::Mica.supported(26100), Blur::Mica);
        assert_eq!(Blur::Mica.supported(22621), Blur::Mica);
        assert_eq!(Blur::Mica.supported(22000), Blur::None);
        assert_eq!(Blur::Mica.supported(0), Blur::None);
        assert_eq!(Blur::Acrylic.supported(22000), Blur::Acrylic);
        assert_eq!(Blur::Acrylic.supported(17763), Blur::Acrylic);
        assert_eq!(Blur::Acrylic.supported(17134), Blur::None);
        assert_eq!(Blur::None.supported(26100), Blur::None);
    }

    #[test]
    fn mica_takes_the_colour_of_the_ground_and_none_makes_no_effect() {
        assert_eq!(Blur::Mica.effect(true), Some(Effect::MicaDark));
        assert_eq!(Blur::Mica.effect(false), Some(Effect::MicaLight));
        assert_eq!(Blur::Acrylic.effect(true), Some(Effect::Acrylic));
        assert_eq!(Blur::None.effect(false), None);
    }

    #[test]
    fn only_an_opacity_under_the_whole_asks_for_a_transparent_window() {
        assert!(wants_translucency(Some(&json!({"glass": {"opacity": 0.8, "blur": "none"}}))));
        assert!(wants_translucency(Some(&json!({"glass": {"opacity": 0.5}}))));
        assert!(!wants_translucency(Some(&json!({"glass": {"opacity": 1, "blur": "mica"}}))));
        assert!(!wants_translucency(Some(&json!({"glass": {"opacity": 1.0}}))));
    }

    #[test]
    fn a_state_that_says_nothing_of_it_is_the_opaque_window() {
        assert!(!wants_translucency(None));
        assert!(!wants_translucency(Some(&json!({}))));
        assert!(!wants_translucency(Some(&json!({"glass": null}))));
        assert!(!wants_translucency(Some(&json!({"glass": {"opacity": "0.5"}}))));
        assert!(!wants_translucency(Some(&json!({"glass": "mica"}))));
    }

    #[test]
    fn the_run_replaced_is_the_process_the_variable_names() {
        assert_eq!(predecessor(Some("4242")), Some(4242));
        assert_eq!(predecessor(Some(" 77 ")), Some(77));
        assert_eq!(predecessor(None), None);
        assert_eq!(predecessor(Some("")), None);
        assert_eq!(predecessor(Some("0")), None);
        assert_eq!(predecessor(Some("-3")), None);
        assert_eq!(predecessor(Some("abc")), None);
        // A run is never made to wait for itself.
        assert_eq!(predecessor(Some(&std::process::id().to_string())), None);
    }

    #[test]
    fn the_windows_made_transparent_are_the_ones_remembered() {
        let glass = Glass::default();
        assert!(!glass.is_transparent("main"));
        glass.remember("main", true);
        glass.remember("quick", true);
        assert!(glass.is_transparent("main"));
        let mut labels = glass.labels();
        labels.sort();
        assert_eq!(labels, ["main", "quick"]);
        glass.remember("quick", false);
        assert!(!glass.is_transparent("quick"));
    }
}
