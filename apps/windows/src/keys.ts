// The window's keyboard, by the conventions of Windows Terminal and Chrome.
//
// Matched on `code` — the key's place on the keyboard — rather than on the
// character it types, so a chord works the same on an ABNT2 layout as on a
// US one, and a dead key never stands in the way of one.

export type Action =
  | { kind: "newTab" }
  | { kind: "newWorkspace" }
  | { kind: "nextTab" }
  | { kind: "previousTab" }
  | { kind: "goToTab"; index: number }
  | { kind: "lastTab" }
  | { kind: "picker" }
  | { kind: "searchAll" }
  | { kind: "findInTab" }
  | { kind: "zoomIn" }
  | { kind: "zoomOut" }
  | { kind: "zoomReset" }
  | { kind: "splitRight" }
  | { kind: "splitDown" }
  | { kind: "rename" }
  | { kind: "copy" }
  | { kind: "paste" };

export interface KeyLike {
  key: string;
  code: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}

/**
 * The window's own chord, if this is one. `mac` takes ⌘ where Windows takes
 * Ctrl — the app is built for Windows and only developed on a Mac.
 *
 * Nothing closes a tab from the keyboard: on the Mac the person turned ⌘W
 * off, and a tab holds work that a slipped finger should not end.
 */
export function shortcut(ev: KeyLike, mac = false): Action | null {
  const mod = mac ? ev.metaKey : ev.ctrlKey;
  const otherMod = mac ? ev.ctrlKey : ev.metaKey;
  if (ev.code === "F2" && !mod && !ev.shiftKey && !ev.altKey) return { kind: "rename" };
  if (!mod || ev.altKey || otherMod) return null;
  if (ev.code === "Tab") return ev.shiftKey ? { kind: "previousTab" } : { kind: "nextTab" };
  if (ev.shiftKey) {
    switch (ev.code) {
      case "KeyT":
        return { kind: "newTab" };
      case "KeyN":
        return { kind: "newWorkspace" };
      case "KeyP":
        return { kind: "picker" };
      case "KeyF":
        return { kind: "searchAll" };
      case "KeyD":
        return { kind: "splitRight" };
      case "KeyE":
        return { kind: "splitDown" };
      case "KeyC":
        return { kind: "copy" };
      case "KeyV":
        return { kind: "paste" };
      // Shift with = types +: Ctrl+Shift+= is Ctrl++.
      case "Equal":
        return { kind: "zoomIn" };
    }
    return null;
  }
  switch (ev.code) {
    case "Equal":
    case "NumpadAdd":
      return { kind: "zoomIn" };
    case "Minus":
    case "NumpadSubtract":
      return { kind: "zoomOut" };
    case "Digit0":
    case "Numpad0":
      return { kind: "zoomReset" };
    case "KeyF":
      return { kind: "findInTab" };
    case "KeyV":
      return { kind: "paste" };
  }
  const digit = /^(?:Digit|Numpad)([1-9])$/.exec(ev.code);
  if (digit) {
    // Chrome's: the ninth is the last, however many there are.
    const n = Number(digit[1]);
    return n === 9 ? { kind: "lastTab" } : { kind: "goToTab", index: n - 1 };
  }
  return null;
}

/** Shift+Enter: Claude Code reads ESC+CR as a new line rather than as send. */
export const isShiftEnter = (ev: KeyLike) =>
  (ev.code === "Enter" || ev.code === "NumpadEnter") && ev.shiftKey && !ev.ctrlKey && !ev.altKey && !ev.metaKey;

export const SHIFT_ENTER_SEQUENCE = "\x1b\r";

/** Ctrl+C on its own: copy when something is selected, ^C otherwise. */
export const isPlainCopy = (ev: KeyLike, mac = false) =>
  ev.code === "KeyC" && (mac ? ev.metaKey : ev.ctrlKey) && !ev.shiftKey && !ev.altKey;
