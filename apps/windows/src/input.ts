// The keyboard, the mouse wheel, the clipboard and dropped files, for the
// whole window.
//
// A chord of the window's own is acted on at the window and kept from the
// program in the tab; everything else is the program's. The browser's own
// chords — reload, print, find in page — are kept from the page, which is an
// app, not a document.

import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { SPLIT_DOWN, SPLIT_RIGHT } from "./api";
import { SHIFT_ENTER_SEQUENCE, imagePasteSequence, isPlainCopy, isPlainCut, isShiftEnter, shortcut, type Action } from "./keys";
import { activeTabHasPanes, focusBeside, resizeBeside } from "./paneActions";
import {
  activeTab,
  activeView,
  activeWorkspace,
  copySelection,
  dialog,
  findOpen,
  goToTab,
  isMac,
  newTab,
  newWorkspace,
  overlay,
  split,
  startRename,
  stepTab,
  stepWorkspace,
  toggleSidebar,
  terminals,
  zoom,
  zoomReset,
} from "./store";
import { toggleOverlay } from "./palette/overlay";
import type { TermView } from "./terminals";
import { cutKeys, quotePaths } from "./text";
import { openNewWindow } from "./windowSync";

export function perform(action: Action): void {
  switch (action.kind) {
    case "newTab":
      void newTab();
      break;
    case "newWorkspace":
      void newWorkspace();
      break;
    case "newWindow":
      void openNewWindow();
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
      toggleOverlay("picker");
      break;
    case "palette":
      toggleOverlay("palette");
      break;
    case "searchAll":
      toggleOverlay("search");
      break;
    case "findInTab":
      toggleOverlay("find");
      break;
    case "findOnScreen":
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
    case "nextWorkspace":
      stepWorkspace(1);
      break;
    case "previousWorkspace":
      stepWorkspace(-1);
      break;
    case "toggleSidebar":
      toggleSidebar();
      break;
    case "selectAll":
      activeView()?.term.selectAll();
      break;
    case "clearScreen":
      // A form feed, as the Mac sends it: the screen is the daemon's, and a
      // program drawing its own keeps its own account of what is on it.
      void activeView()?.type("\x0c").catch(() => {});
      break;
    case "fullscreen": {
      const win = getCurrentWindow();
      void win.isFullscreen().then((on) => win.setFullscreen(!on));
      break;
    }
    case "settings":
      toggleOverlay("settings");
      break;
    case "focusPane":
      focusBeside(action.direction);
      break;
    case "resizePane":
      resizeBeside(action.direction);
      break;
    case "rename": {
      const ws = activeWorkspace.value;
      const tab = activeTab.value;
      if (ws && tab) startRename(ws, tab.root.id);
      break;
    }
    case "copy":
      void copySelection();
      break;
    case "pasteImage":
      void activeView()?.type(imagePasteSequence(isMac.value)).catch(() => {});
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
 * The window's own chord for a key, if it is one just now. The arrows between
 * panes are the window's only in a tab that has panes: in any other the
 * program has them, and Alt and an arrow is a word's distance in a shell.
 */
function chordOf(event: KeyboardEvent): Action | null {
  const action = shortcut(event, isMac.value);
  if ((action?.kind === "focusPane" || action?.kind === "resizePane") && !activeTabHasPanes()) return null;
  return action;
}

/**
 * A key on its way into a terminal: true lets xterm.js have it. The window's
 * chords go on to the window; Shift+Enter becomes what Claude Code reads as a
 * new line; Ctrl+C with something selected copies it rather than
 * interrupting the program.
 */
export function terminalKey(view: TermView, event: KeyboardEvent): boolean {
  const mac = isMac.value;
  if (chordOf(event)) return false;
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
  if (isPlainCut(event, mac) && view.term.hasSelection()) {
    if (event.type === "keydown") {
      event.preventDefault();
      cutSelection(view);
    }
    return false;
  }
  return true;
}

/**
 * Ctrl+X on a selection: copied, and taken out of the line the program is
 * editing when the selection is on the row the cursor is on (see `cutKeys`).
 */
function cutSelection(view: TermView): void {
  const text = view.term.getSelection();
  void writeText(text).catch(() => navigator.clipboard?.writeText(text));
  const buffer = view.term.buffer.active;
  const row = buffer.getLine(buffer.baseY + buffer.cursorY)?.translateToString(true) ?? "";
  const keys = cutKeys(text, row, buffer.cursorX);
  view.term.clearSelection();
  if (keys) void view.type(keys).catch(() => {});
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
  "newWindow",
  "nextTab",
  "previousTab",
  "goToTab",
  "lastTab",
  "picker",
  "palette",
  "searchAll",
  "findInTab",
  "zoomIn",
  "zoomOut",
  "zoomReset",
  "nextWorkspace",
  "previousWorkspace",
  "toggleSidebar",
  "fullscreen",
  "settings",
]);

export function installInput(): void {
  window.addEventListener("keydown", (event) => {
    // A question on screen has the keyboard to itself.
    if (dialog.value) return;
    const inTerminal = isTerminalTarget(event.target);
    const inField = isFieldTarget(event.target);
    const action = chordOf(event);
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
