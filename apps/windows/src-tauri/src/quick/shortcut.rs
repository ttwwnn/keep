//! The drop-down terminal's shortcut: what text is one, how it is spelled, and which key of the keyboard in use it
//! is. quick.rs registers what this says with the system.

use std::str::FromStr;

use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

/// What the Mac registers: ⌃`.
pub const DEFAULT_SHORTCUT: &str = "Control+Backquote";

/// Offered in its place when the system will not give the shortcut, in this order. Ctrl and Shift
/// come before Ctrl and Alt, which a layout with AltGr (ABNT2 has it) types its own characters with.
pub const ALTERNATIVES: [&str; 4] = [
    "Control+Shift+Backquote",
    "Control+Alt+Backquote",
    "Control+Shift+KeyK",
    "Control+Alt+Space",
];

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
pub fn registerable(shortcut: Shortcut) -> Shortcut {
    on_layout(shortcut, virtual_key_at)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
