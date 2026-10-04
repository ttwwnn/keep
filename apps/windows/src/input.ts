// The keyboard, the mouse wheel, the clipboard and dropped files, for the
// whole window.
//
// A chord of the window's own is acted on at the window and kept from the
// program in the tab; everything else is the program's. The browser's own
// chords — reload, print, find in page — are kept from the page, which is an
// app, not a document.

import { getCurrentWebview } from "@tauri-apps/api/webview";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { SPLIT_DOWN, SPLIT_RIGHT } from "./api";
import { SHIFT_ENTER_SEQUENCE, isPlainCopy, isShiftEnter, shortcut, type Action } from "./keys";
import {
  activeTab,
  activeView,
  activeWorkspace,
  copySelection,
  dialog,
  editing,
  findOpen,
  goToTab,
  isMac,
  newTab,
  newWorkspace,
  overlay,
  split,
  stepTab,
  terminals,
  zoom,
  zoomReset,
} from "./store";
import type { TermView } from "./terminals";
import { quotePaths } from "./text";

export function perform(action: Action): void {
  switch (action.kind) {
    case "newTab":
      void newTab();
      break;
    case "newWorkspace":
      void newWorkspace();
      break;
    case "nextTab":
      stepTab(1);
      break;
    case "previousTab":
      stepTab(-1);
      break;
    case "goToTab":
      goToTab(action.index);
      break;
    case "lastTab":
      goToTab("last");
      break;
    case "picker":
      overlay.value = overlay.value === "picker" ? null : "picker";
      break;
    case "searchAll":
      overlay.value = overlay.value === "search" ? null : "search";
      break;
    case "findInTab":
      findOpen.value = true;
      break;
    case "zoomIn":
      zoom(1);
      break;
    case "zoomOut":
      zoom(-1);
      break;
    case "zoomReset":
      zoomReset();
      break;
    case "splitRight":
      void split(SPLIT_RIGHT);
      break;
    case "splitDown":
      void split(SPLIT_DOWN);
      break;
    case "rename": {
      const ws = activeWorkspace.value;
      const tab = activeTab.value;
      if (ws && tab) editing.value = { kind: "tab", workspace: ws, tab: tab.root.id };
      break;
    }
    case "copy":
      void copySelection();
      break;
    case "paste": {
      const view = activeView();
      void readText()
        .then((text) => text && view?.paste(text))
        .catch(() => {});
      view?.focus();
      break;
    }
  }
}

/**
 * A key on its way into a terminal: true lets xterm.js have it. The window's
 * chords go on to the window; Shift+Enter becomes what Claude Code reads as a
 * new line; Ctrl+C with something selected copies it rather than
 * interrupting the program.
 */
export function terminalKey(view: TermView, event: KeyboardEvent): boolean {
  const mac = isMac.value;
  if (shortcut(event, mac)) return false;
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
      const text = view.term.getSelection();
      void writeText(text).catch(() => navigator.clipboard?.writeText(text));
      view.term.clearSelection();
    }
    return false;
  }
  return true;
}

const BROWSER_CHORDS = new Set(["KeyR", "KeyP", "KeyJ", "KeyU", "KeyS", "KeyO", "KeyG", "KeyH", "KeyF"]);

function isTerminalTarget(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && target.classList.contains("xterm-helper-textarea");
}

function isFieldTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement) || isTerminalTarget(target)) return false;
  return target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT" || target.isContentEditable;
}

/** Chords that act even while a field has the keyboard. */
const EVERYWHERE = new Set<Action["kind"]>([
  "newTab",
  "newWorkspace",
  "nextTab",
  "previousTab",
  "goToTab",
  "lastTab",
  "picker",
  "searchAll",
  "zoomIn",
  "zoomOut",
  "zoomReset",
]);

export function installInput(): void {
  window.addEventListener("keydown", (event) => {
    // A question on screen has the keyboard to itself.
    if (dialog.value) return;
    const inTerminal = isTerminalTarget(event.target);
    const inField = isFieldTarget(event.target);
    const action = shortcut(event, isMac.value);
    if (action) {
      if (inField && !EVERYWHERE.has(action.kind)) return;
      // In a terminal, Ctrl+V is the browser's own paste, which xterm.js
      // takes as a paste — bracketed, if the program asked for that.
      if (action.kind === "paste" && inTerminal) return;
      event.preventDefault();
      event.stopPropagation();
      perform(action);
      return;
    }
    if (inTerminal || inField) return;
    if ((event.ctrlKey || event.metaKey) && BROWSER_CHORDS.has(event.code)) event.preventDefault();
    if (event.code === "F5" || event.code === "F3" || event.code === "F7" || event.code === "F12") event.preventDefault();
    if (event.key === "Escape") {
      if (overlay.value) overlay.value = null;
      activeView()?.focus();
    }
  });

  // Ctrl and the wheel zoom the terminals, as in a browser, rather than the page.
  window.addEventListener(
    "wheel",
    (event) => {
      if (!(event.ctrlKey || event.metaKey)) return;
      event.preventDefault();
      event.stopPropagation();
      if (event.deltaY !== 0) zoom(event.deltaY < 0 ? 1 : -1);
    },
    { passive: false, capture: true },
  );

  // The page's own menu offers "reload" and "print"; the window's menus are
  // opened where they make sense. Fields keep theirs, for cut and paste.
  window.addEventListener("contextmenu", (event) => {
    if (!isFieldTarget(event.target)) event.preventDefault();
  });

  // Files dropped on a terminal arrive as their paths, typed where the
  // cursor is, as in any terminal.
  void getCurrentWebview()
    .onDragDropEvent((event) => {
      if (event.payload.type !== "drop" || event.payload.paths.length === 0) return;
      const ratio = window.devicePixelRatio || 1;
      const { x, y } = event.payload.position;
      const under = document.elementFromPoint(x / ratio, y / ratio)?.closest<HTMLElement>(".pane");
      const view =
        (under?.dataset.workspace && under.dataset.pane
          ? terminals.existing(under.dataset.workspace, Number(under.dataset.pane))
          : undefined) ?? activeView();
      if (!view) return;
      view.paste(quotePaths(event.payload.paths) + " ");
      view.focus();
    })
    .catch(() => {});
}
