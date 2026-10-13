// The panel's keyboard, mouse wheel, menu and dropped files. The main window's
// (input.ts) acts on the window it is in — tabs, the sidebar, the cards —; the
// panel has one terminal and none of that, so it keeps the chords that are
// the terminal's own and swallows the rest, so that a habit (Ctrl+Shift+P)
// does not type its control character into the shell.

import { getCurrentWebview } from "@tauri-apps/api/webview";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { SHIFT_ENTER_SEQUENCE, imagePasteSequence, isPlainCopy, isPlainCut, isShiftEnter, shortcut, type Action } from "../keys";
import { cutSelection } from "../input";
import { isMac } from "../store";
import { quotePaths } from "../text";
import type { TermView } from "../terminals";
import { openMenu } from "../ui/common";

async function copy(view: TermView): Promise<void> {
  const text = view.term.getSelection();
  if (text) await writeText(text).catch(() => navigator.clipboard?.writeText(text));
}

/** The window's chords that mean something to a terminal on its own. */
function act(action: Action, view: TermView, event: KeyboardEvent): void {
  switch (action.kind) {
    case "copy":
      event.preventDefault();
      void copy(view);
      break;
    // The browser's own paste follows the key, and xterm.js takes it as a paste — bracketed, if the program asked.
    case "paste":
      break;
    case "pasteImage":
      event.preventDefault();
      void view.type(imagePasteSequence(isMac.value)).catch(() => {});
      break;
    case "selectAll":
      event.preventDefault();
      view.term.selectAll();
      break;
    case "clearScreen":
      event.preventDefault();
      void view.type("\x0c").catch(() => {});
      break;
    default:
      event.preventDefault();
  }
}

/** A key on its way into the terminal: true lets xterm.js have it (the counterpart of input.ts `terminalKey`). */
export function quickKey(view: TermView, event: KeyboardEvent): boolean {
  const mac = isMac.value;
  const action = shortcut(event, mac);
  if (action) {
    if (event.type === "keydown") act(action, view, event);
    return false;
  }
  if (isShiftEnter(event)) {
    if (event.type === "keydown") {
      event.preventDefault();
      void view.type(SHIFT_ENTER_SEQUENCE).catch(() => {});
    }
    return false;
  }
  if (isPlainCopy(event, mac) && view.term.hasSelection()) {
    if (event.type === "keydown") {
      event.preventDefault();
      void copy(view);
      view.term.clearSelection();
    }
    return false;
  }
  if (isPlainCut(event, mac) && view.term.hasSelection()) {
    if (event.type === "keydown") {
      event.preventDefault();
      cutSelection(view);
    }
    return false;
  }
  return true;
}

/** The terminal's menu, opened where the pointer is. */
export function openQuickMenu(event: MouseEvent, view: TermView | undefined): void {
  if (!view) return;
  openMenu(event, [
    { label: "Copiar", hint: "Ctrl+C", disabled: !view.term.hasSelection(), action: () => void copy(view) },
    {
      label: "Colar",
      hint: "Ctrl+V",
      action: () => void readText().then((text) => text && view.paste(text)).catch(() => {}),
    },
    "separator",
    { label: "Selecionar tudo", hint: "Ctrl+Shift+A", action: () => view.term.selectAll() },
    { label: "Limpar a tela", hint: "Ctrl+Shift+L", action: () => void view.type("\x0c").catch(() => {}) },
  ]);
}

/** The page is an app, not a document: no browser menu, no zoom of the page by the wheel; files dropped are typed as paths. */
export function installQuickInput(current: () => TermView | undefined): void {
  window.addEventListener("contextmenu", (event) => event.preventDefault());
  window.addEventListener(
    "wheel",
    (event) => {
      if (!(event.ctrlKey || event.metaKey)) return;
      event.preventDefault();
      event.stopPropagation();
    },
    { passive: false, capture: true },
  );
  void getCurrentWebview()
    .onDragDropEvent((event) => {
      if (event.payload.type !== "drop" || event.payload.paths.length === 0) return;
      const view = current();
      if (!view) return;
      view.paste(quotePaths(event.payload.paths) + " ");
      view.focus();
    })
    .catch(() => {});
}
