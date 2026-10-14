// What a key is sent as once a program has turned the Kitty keyboard protocol on.
//
// A port of the encoder of Ghostty (src/input/key_encode.zig, `kitty`), which
// is what the macOS app's terminal runs, with the choices the macOS app makes
// about what it hands that encoder (Display/SurfaceView.swift, GhosttyApp.swift),
// fed from the events of a browser instead of an NSEvent. The protocol:
// https://sw.kovidgoyal.net/kitty/keyboard-protocol/
//
// The encoder answers a key with what to send, or with `undefined` when the
// key is not its to encode: the text a key types is typed by the terminal
// (xterm.js) as it always was, a key under composition — a dead key, an IME —
// is the terminal's until it is done, and a key the protocol says nothing for
// says nothing.

/** The flags a program turns on, with `CSI > flags u` and the rest. */
export const DISAMBIGUATE = 1;
export const REPORT_EVENTS = 2;
export const REPORT_ALTERNATES = 4;
export const REPORT_ALL = 8;
export const REPORT_ASSOCIATED = 16;

export type Action = "press" | "repeat" | "release";

/** A key press or release, as the encoder reads it. */
export interface KittyKey {
  action: Action;
  /**
   * The physical key, by its W3C `code` ("KeyA", "ArrowUp", "Numpad1"). The
   * keypad's navigation legends, with NumLock off, are named "NumpadLeft",
   * "NumpadHome" and so on, as the protocol has them.
   */
  key: string;
  shift: boolean;
  ctrl: boolean;
  alt: boolean;
  /** The Windows key, or ⌘: the protocol's super. */
  meta: boolean;
  /** Shift went into making `text` (it made "A" of "a"), so it is not a modifier of the key. */
  shiftSpent: boolean;
  /** The key is part of a dead key or an IME composition. */
  composing: boolean;
  /** What the key typed, as the layout has it; empty for a key that types nothing. */
  text: string;
  /** The code point of the key with nothing held, in the layout; 0 when it has none. */
  unshifted: number;
}

interface Entry {
  code: number;
  final: string;
  /** Pressing it changes the modifiers: reported only when every key is. */
  modifier: boolean;
}

const entry = (code: number, final = "u", modifier = false): Entry => ({ code, final, modifier });

/** The protocol's functional keys (Ghostty's kitty.zig, from foot's table), by `code`. */
const ENTRIES: Record<string, Entry> = {
  Escape: entry(27),
  Enter: entry(13),
  Tab: entry(9),
  Backspace: entry(127),
  Insert: entry(2, "~"),
  Delete: entry(3, "~"),
  ArrowLeft: entry(1, "D"),
  ArrowRight: entry(1, "C"),
  ArrowUp: entry(1, "A"),
  ArrowDown: entry(1, "B"),
  PageUp: entry(5, "~"),
  PageDown: entry(6, "~"),
  Home: entry(1, "H"),
  End: entry(1, "F"),
  CapsLock: entry(57358, "u", true),
  ScrollLock: entry(57359),
  NumLock: entry(57360, "u", true),
  PrintScreen: entry(57361),
  Pause: entry(57362),
  F1: entry(1, "P"),
  F2: entry(1, "Q"),
  F3: entry(13, "~"),
  F4: entry(1, "S"),
  F5: entry(15, "~"),
  F6: entry(17, "~"),
  F7: entry(18, "~"),
  F8: entry(19, "~"),
  F9: entry(20, "~"),
  F10: entry(21, "~"),
  F11: entry(23, "~"),
  F12: entry(24, "~"),
  NumpadDecimal: entry(57409),
  NumpadDivide: entry(57410),
  NumpadMultiply: entry(57411),
  NumpadSubtract: entry(57412),
  NumpadAdd: entry(57413),
  NumpadEnter: entry(57414),
  NumpadEqual: entry(57415),
  NumpadLeft: entry(57417),
  NumpadRight: entry(57418),
  NumpadUp: entry(57419),
  NumpadDown: entry(57420),
  NumpadPageUp: entry(57421),
  NumpadPageDown: entry(57422),
  NumpadHome: entry(57423),
  NumpadEnd: entry(57424),
  NumpadInsert: entry(57425),
  NumpadDelete: entry(57426),
  NumpadBegin: entry(57427),
  ShiftLeft: entry(57441, "u", true),
  ControlLeft: entry(57442, "u", true),
  AltLeft: entry(57443, "u", true),
  MetaLeft: entry(57444, "u", true),
  ShiftRight: entry(57447, "u", true),
  ControlRight: entry(57448, "u", true),
  AltRight: entry(57449, "u", true),
  MetaRight: entry(57450, "u", true),
};
for (let n = 13; n <= 25; n++) ENTRIES[`F${n}`] = entry(57376 + n - 13);
for (let n = 0; n <= 9; n++) ENTRIES[`Numpad${n}`] = entry(57399 + n);

/** The character of a physical key on a US keyboard: what the protocol calls the key's base layout. */
const BASE: Record<string, number> = {
  Semicolon: 59,
  Space: 32,
  Quote: 39,
  Comma: 44,
  Backquote: 96,
  Period: 46,
  Slash: 47,
  Minus: 45,
  Equal: 61,
  BracketLeft: 91,
  BracketRight: 93,
  Backslash: 92,
  Tab: 9,
  NumpadDecimal: 46,
  NumpadDivide: 47,
  NumpadMultiply: 42,
  NumpadSubtract: 45,
  NumpadAdd: 43,
  NumpadEqual: 61,
};
for (let n = 0; n < 26; n++) BASE[`Key${String.fromCharCode(65 + n)}`] = 97 + n;
for (let n = 0; n <= 9; n++) {
  BASE[`Digit${n}`] = 48 + n;
  BASE[`Numpad${n}`] = 48 + n;
}

const isControl = (cp: number) => cp < 0x20 || cp === 0x7f;

const codepoints = (text: string) => Array.from(text, (c) => c.codePointAt(0) ?? 0);

/**
 * What to send for a key, given the flags the program turned on; undefined
 * when the terminal's own way is the right one (see the top of the file).
 */
export function encodeKitty(event: KittyKey, flags: number): string | undefined {
  if (flags === 0) return undefined;
  const all = (flags & REPORT_ALL) !== 0;
  const reportEvents = (flags & REPORT_EVENTS) !== 0;

  if (event.action === "release") {
    if (!reportEvents) return undefined;
    // Enter, Backspace and Tab say nothing on release unless every key is reported.
    if (!all && (event.key === "Enter" || event.key === "Backspace" || event.key === "Tab")) return undefined;
  }

  const text = codepoints(event.text);
  // The modifiers the key is pressed with, less the one that made its text.
  const shiftHeld = event.shift && !(text.length > 0 && event.shiftSpent);
  const bare = !shiftHeld && !event.ctrl && !event.alt && !event.meta;
  const found: Entry | undefined =
    ENTRIES[event.key] ?? (event.unshifted > 0 ? { code: event.unshifted, final: "u", modifier: false } : undefined);

  if (event.composing) {
    // A composition sends nothing but the modifiers it is made with.
    if (!found?.modifier) return undefined;
  } else {
    // An IME's confirmation is an Enter with the text: the text, then.
    if (text.length > 0 && (event.key === "Enter" || event.key === "Backspace")) {
      const control = text.length === 1 && isControl(text[0]);
      if (!control) return event.key === "Backspace" ? "" : event.text;
    }
    if (!all) {
      // "The only exceptions are the Enter, Tab and Backspace keys which still generate the same bytes as in legacy
      // mode": so that a shell can be typed in after a program that set the mode has crashed.
      if (bare) {
        if (event.key === "Enter") return "\r";
        if (event.key === "Tab") return "\t";
        if (event.key === "Backspace") return "\x7f";
      }
      // What a key types, with nothing held that would make it something else, is typed.
      if (text.length > 0 && bare && event.action !== "release" && text.every((cp) => !isControl(cp))) return undefined;
    }
  }

  // A key with no code of its own that typed something is composed text, which the terminal types.
  if (!found) return undefined;
  // A modifier alone is not a key, unless every key is reported.
  if (found.modifier && !all) return undefined;

  const mods = (event.shift ? 1 : 0) | (event.alt ? 2 : 0) | (event.ctrl ? 4 : 0) | (event.meta ? 8 : 0);
  const kind = event.action === "release" ? 3 : event.action === "repeat" ? 2 : 1;

  // The key as shifted, and as a US keyboard has it.
  let shifted: number | undefined;
  let layout: number | undefined;
  if ((flags & REPORT_ALTERNATES) !== 0 && !isControl(found.code)) {
    const base = BASE[event.key];
    if (text.length > 0) {
      if (text[0] !== found.code && event.shift) shifted = text[0];
      if (base !== undefined && base !== found.code && text[0] !== base && text.length === 1) layout = base;
    } else if (base !== undefined && base !== found.code) {
      layout = base;
    }
  }

  // The text the key types goes along, except where a modifier says it was not typing.
  const associated =
    (flags & REPORT_ASSOCIATED) !== 0 && event.action !== "release" && !event.alt && !event.ctrl && !event.meta ? text : [];

  if (found.final !== "u" && found.final !== "~") {
    // The keys that have a letter of their own: arrows, home, end, F1-F4, F4.
    if (reportEvents) return `\x1b[1;${mods + 1}:${kind}${found.final}`;
    if (mods > 0) return `\x1b[1;${mods + 1}${found.final}`;
    return `\x1b[${found.final}`;
  }

  let out = `\x1b[${found.code}`;
  if (shifted !== undefined) out += `:${shifted}`;
  if (layout !== undefined) out += shifted === undefined ? `::${layout}` : `:${layout}`;
  let wrote = false;
  if (reportEvents && kind !== 1) {
    out += `;${mods + 1}:${kind}`;
    wrote = true;
  } else if (mods > 0) {
    out += `;${mods + 1}`;
    wrote = true;
  }
  let count = 0;
  for (const cp of associated) {
    if (isControl(cp)) continue;
    // Two semicolons, the first time, when the modifiers did not already write one.
    if (count === 0) out += wrote ? ";" : ";;";
    else out += ":";
    out += cp;
    count++;
  }
  return out + found.final;
}

/** The shape of a browser's KeyboardEvent that the keys below read. */
export interface BrowserKey {
  type: string;
  code: string;
  key: string;
  keyCode: number;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  repeat: boolean;
  isComposing: boolean;
  getModifierState(key: string): boolean;
}

/** The keypad's other legends, when NumLock is off: the `code` says the digit, the `key` says what it does now. */
const NUMPAD_LEGEND: Record<string, Record<string, string>> = {
  Numpad0: { Insert: "NumpadInsert" },
  NumpadDecimal: { Delete: "NumpadDelete" },
  Numpad1: { End: "NumpadEnd" },
  Numpad2: { ArrowDown: "NumpadDown" },
  Numpad3: { PageDown: "NumpadPageDown" },
  Numpad4: { ArrowLeft: "NumpadLeft" },
  Numpad5: { Clear: "NumpadBegin" },
  Numpad6: { ArrowRight: "NumpadRight" },
  Numpad7: { Home: "NumpadHome" },
  Numpad8: { ArrowUp: "NumpadUp" },
  Numpad9: { PageUp: "NumpadPageUp" },
};

/** What the layout puts on a physical key with nothing held (`navigator.keyboard`, where there is one). */
let layoutMap: ReadonlyMap<string, string> | undefined;

/** Take the layout from `map`; undefined goes back to reading it off the events. */
export function useLayout(map: ReadonlyMap<string, string> | undefined): void {
  layoutMap = map;
}

interface KeyboardApi {
  getLayoutMap(): Promise<ReadonlyMap<string, string>>;
  addEventListener(type: "layoutchange", listener: () => void): void;
}

/** Ask the browser for the layout, now and whenever it changes: Chromium has it, WebKit does not. */
export function watchLayout(): void {
  const keyboard = (typeof navigator === "undefined" ? undefined : (navigator as { keyboard?: KeyboardApi }).keyboard);
  if (!keyboard?.getLayoutMap) return;
  const load = () => {
    keyboard
      .getLayoutMap()
      .then((map) => (layoutMap = map))
      .catch(() => (layoutMap = undefined));
  };
  load();
  try {
    keyboard.addEventListener("layoutchange", load);
  } catch {
    // A layout that changes while the window is open is read again at the next start.
  }
}

/** The one character in `text`, if it is a character a key types. */
function character(text: string): string {
  const chars = Array.from(text);
  return chars.length === 1 && !isControl(chars[0].codePointAt(0) ?? 0) ? text : "";
}

/**
 * What a browser key is sent as, with the protocol on (`flags`); undefined when
 * the terminal is to handle it. `mac`: Option is Alt here (it composes
 * characters, which a terminal's Alt does not).
 */
export function kittyKey(event: BrowserKey, flags: number, env: { mac: boolean }): string | undefined {
  if (flags === 0 || (event.type !== "keydown" && event.type !== "keyup")) return undefined;
  // Under composition, or a key the browser could not name: the terminal's.
  if (event.isComposing || event.keyCode === 229) return undefined;
  if (event.key === "Dead" || event.key === "Process" || event.key === "Unidentified") return undefined;
  // AltGr types a character: with Windows it is Ctrl and Alt together, which are not modifiers here.
  if (character(event.key) !== "" && event.getModifierState("AltGraph")) return undefined;

  // Two chords the macOS app spells out, because its encoder drops the shift or the alt in this mode.
  if (event.type === "keydown" && !event.ctrlKey && !event.metaKey) {
    if (event.code === "Tab" && event.shiftKey && !event.altKey) return "\x1b[Z";
    if (event.code === "Backspace" && event.altKey && !event.shiftKey) return "\x1b\x7f";
  }

  const spelled = character(event.key);
  // Ctrl makes a control code, which is the terminal's to make from the key; Option composes ("å").
  const text = event.ctrlKey || (env.mac && event.altKey) ? "" : spelled;
  const mapped = layoutMap?.get(event.code);
  let unshifted = mapped !== undefined ? (character(mapped).codePointAt(0) ?? 0) : 0;
  if (unshifted === 0) {
    const plain = !event.shiftKey && !(env.mac && event.altKey) ? spelled.toLowerCase() : "";
    unshifted = plain !== "" ? (plain.codePointAt(0) ?? 0) : (BASE[event.code] ?? 0);
  }
  if (isControl(unshifted)) unshifted = 0;
  const typed = unshifted > 0 ? String.fromCodePoint(unshifted) : "";

  return encodeKitty(
    {
      action: event.type === "keyup" ? "release" : event.repeat ? "repeat" : "press",
      key: NUMPAD_LEGEND[event.code]?.[event.key] ?? event.code,
      shift: event.shiftKey,
      ctrl: event.ctrlKey,
      alt: event.altKey,
      meta: event.metaKey,
      // Shift is spent where it made a different character of the key.
      shiftSpent: event.shiftKey && text !== "" && text !== typed,
      composing: false,
      text,
      unshifted,
    },
    flags,
  );
}
