// The panel's keyboard, mouse wheel, menu and dropped files. The main window's
// (input.ts) acts on the window it is in — tabs, the sidebar, the cards —; the
// panel has one terminal and none of that, so it keeps the chords that are
// the terminal's own and swallows the rest, so that a habit (Ctrl+Shift+P)
// does not type its control character into the shell.

import { getCurrentWebview } from "@tauri-apps/api/webview";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { dropText, shellKind } from "../drop";
import { terminalKey } from "../input";
import { imagePasteSequence, shortcut, type Action } from "../keys";
import { isMac } from "../store";
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

/**
 * A key on its way into the terminal: true lets xterm.js have it. The window's chords that mean something to a
 * terminal on its own are done here, the rest are kept from the program; everything that is not a chord is a tab's
 * key as in any other (input.ts `terminalKey`: Ctrl+C on a selection, Ctrl+X, the Kitty keyboard, Shift+Enter).
 */
export function quickKey(view: TermView, event: KeyboardEvent): boolean {
  const action = shortcut(event, isMac.value);
  // The arrows between panes are the program's here: the panel has none, and Alt and an arrow is a word in a shell.
  if (action !== null && action.kind !== "focusPane" && action.kind !== "resizePane") {
    if (event.type === "keydown") act(action, view, event);
    return false;
  }
  return terminalKey(view, event);
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

/**
 * The page is an app, not a document: no browser menu, no zoom of the page by the wheel; files dropped are typed as
 * paths, said as the shell in the panel reads them (`command` is what runs in it).
 */
export function installQuickInput(current: () => TermView | undefined, command: () => string): void {
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
      view.paste(dropText(event.payload.paths, shellKind(command())) + " ");
      view.focus();
    })
    .catch(() => {});
}
