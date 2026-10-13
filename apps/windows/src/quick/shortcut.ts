// The drop-down terminal's shortcut as the page handles it: read from a chord
// being pressed, checked, and drawn as keys. It is kept and registered by the
// app (src-tauri/src/quick.rs, which has the last word on whether a shortcut
// will do); the page only spells it the way the app reads it:
// `Control+Alt+Backquote`, the modifiers in that order, then the key by its
// `code` — the key's place on the keyboard, so that a layout where the
// character is a dead key (ABNT2's `) names it all the same.

/** ⌃` on the Mac. */
export const DEFAULT_SHORTCUT = "Control+Backquote";

/** The keys that are modifiers and nothing else: pressing one is not yet a chord. */
const MODIFIER_CODES = new Set([
  "ControlLeft",
  "ControlRight",
  "ShiftLeft",
  "ShiftRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
  "OSLeft",
  "OSRight",
]);

export interface ChordLike {
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

/** The shortcut a chord spells; null while it is only modifiers (or says no key). */
export function shortcutFromChord(chord: ChordLike): string | null {
  if (chord.code === "" || chord.code === "Unidentified" || MODIFIER_CODES.has(chord.code)) return null;
  const parts: string[] = [];
  if (chord.ctrlKey) parts.push("Control");
  if (chord.altKey) parts.push("Alt");
  if (chord.shiftKey) parts.push("Shift");
  if (chord.metaKey) parts.push("Super");
  parts.push(chord.code);
  return parts.join("+");
}

const FUNCTION_KEY = /^F(?:[1-9]|1\d|2[0-4])$/;

/**
 * Why a shortcut would not do, in words; null when it will. A shortcut takes
 * its key from every program, so it must not be one a person types with: some
 * Ctrl, Alt or Win in it (Shift alone would take the capital letters), or a
 * function key. The app asks the same (quick.rs `parse`) and decides.
 */
export function whyUnusable(shortcut: string): string | null {
  const parts = shortcut.split("+");
  const key = parts[parts.length - 1] ?? "";
  if (key === "") return "não entendi o atalho";
  if (key === "Escape") return "a tecla Esc não serve de atalho";
  const held = parts.slice(0, -1).some((m) => m === "Control" || m === "Alt" || m === "Super");
  if (!held && !FUNCTION_KEY.test(key)) return "o atalho precisa de Ctrl, Alt ou Win (ou ser uma tecla de função)";
  return null;
}

const WINDOWS_NAMES: Readonly<Record<string, string>> = { Control: "Ctrl", Alt: "Alt", Shift: "Shift", Super: "Win" };
const MAC_NAMES: Readonly<Record<string, string>> = { Control: "⌃", Alt: "⌥", Shift: "⇧", Super: "⌘" };

const KEY_NAMES: Readonly<Record<string, string>> = {
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Space: "Espaço",
  Enter: "Enter",
  Tab: "Tab",
  Backspace: "Backspace",
  Delete: "Del",
  Insert: "Ins",
  Home: "Home",
  End: "End",
  PageUp: "PgUp",
  PageDown: "PgDn",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
};

/** A key as it is drawn on its cap: the letter, the digit, the sign. */
export function keyName(code: string): string {
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1];
  const digit = /^Digit(\d)$/.exec(code);
  if (digit) return digit[1];
  const pad = /^Numpad(.+)$/.exec(code);
  if (pad) return `Num ${pad[1]}`;
  return KEY_NAMES[code] ?? code;
}

/** A shortcut as the keys to press, one cap each: `Control+Backquote` is `["Ctrl", "`"]`. */
export function shortcutKeys(shortcut: string, mac = false): string[] {
  const names = mac ? MAC_NAMES : WINDOWS_NAMES;
  return shortcut
    .split("+")
    .filter((part) => part !== "")
    .map((part, i, all) => (i < all.length - 1 ? (names[part] ?? part) : keyName(part)));
}

/** The same in a line of text, for a sentence: `Ctrl+Shift+K`. */
export function shortcutText(shortcut: string, mac = false): string {
  return shortcutKeys(shortcut, mac).join(mac ? "" : "+");
}

/** What the person is told when the system did not give the shortcut, and what they can do about it. */
export function refusalNotice(status: { shortcut: string; problem: string | null; suggestion: string | null }, mac = false): string {
  const what = `A janela rápida não pôde tomar ${shortcutText(status.shortcut, mac)}.`;
  const why = status.problem ? ` ${status.problem}` : "";
  const instead = status.suggestion ? `, por exemplo ${shortcutText(status.suggestion, mac)}` : "";
  return `${what}${why} Em Configurações dá para escolher outro atalho${instead}.`;
}
