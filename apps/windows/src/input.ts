// The keyboard, the mouse wheel, the clipboard and dropped files, for the
// whole window.
//
// A chord of the window's own is acted on at the window and kept from the
// program in the tab; everything else is the program's. The browser's own
// chords — reload, print, find in page — are kept from the page, which is an
// app, not a document.

import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
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
  workspaces,
  zoom,
  zoomReset,
} from "./store";
import { toggleOverlay } from "./palette/overlay";
import type { TermView } from "./terminals";
import { keysToCut } from "./cut";
import { dropText, lightPane, paneAt, shellKind } from "./drop";
import { kittyKey, watchLayout } from "./kittyKeys";
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
      void view?.pasteFromSystem().catch(() => {});
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
 * chords go on to the window; Ctrl+C with something selected copies it rather
 * than interrupting the program; with a program speaking the Kitty keyboard
 * protocol, the keys it asked to be told about are sent as it asked; and
 * Shift+Enter becomes what Claude Code reads as a new line where the protocol
 * is not there to say so.
 */
export function terminalKey(view: TermView, event: KeyboardEvent): boolean {
  const mac = isMac.value;
  if (chordOf(event)) return false;
  if (isPlainCopy(event, mac) && view.term.hasSelection()) {
    if (event.type === "keydown") {
      event.preventDefault();
      const text = view.term.getSelection();
      void writeText(text).catch(() => navigator.clipboard?.writeText(text));
      view.term.clearSelection();
    }
    return false;
  }
  if (isPlainCut(event, mac) && (view.term.hasSelection() || view.selectionTheProgramCopied() !== undefined)) {
    if (event.type === "keydown") {
      event.preventDefault();
      cutSelection(view);
    }
    return false;
  }
  const flags = view.kittyFlags();
  if (flags !== 0) {
    const sequence = kittyKey(event, flags, { mac });
    if (sequence !== undefined) {
      if (event.type === "keydown") event.preventDefault();
      if (sequence !== "") void view.type(sequence).catch(() => {});
      return false;
    }
  }
  if (isShiftEnter(event)) {
    if (event.type === "keydown") {
      event.preventDefault();
      void view.type(SHIFT_ENTER_SEQUENCE).catch(() => {});
    }
    return false;
  }
  return true;
}

/**
 * Ctrl+X on a selection: copied, and taken out of what the program is editing
 * when that can be told (see `keysToCut`): the row of the prompt the cursor is
 * on, or the rows of Claude Code's prompt box. The selection is the terminal's,
 * or the text a program that selects with the mouse itself just copied — which
 * is on the clipboard already.
 */
function cutSelection(view: TermView): void {
  const own = view.term.hasSelection() ? view.term.getSelection() : undefined;
  const selected = own ?? view.selectionTheProgramCopied();
  if (!selected) return;
  if (own !== undefined) void writeText(own).catch(() => navigator.clipboard?.writeText(own));
  // Taken: the next Ctrl+X is the program's again.
  view.programCopied = null;
  const buffer = view.term.buffer.active;
  const keys = keysToCut(selected, view.screenText().split("\n"), buffer.cursorY, buffer.cursorX);
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
  // What the layout types on each key, for a program that asks for the keys themselves (kittyKeys.ts).
  watchLayout();
  window.addEventListener("keydown", (event) => {
    // A question on screen has the keyboard to itself.
    if (dialog.value) return;
    const inTerminal = isTerminalTarget(event.target);
    const inField = isFieldTarget(event.target);
    const action = chordOf(event);
    if (action) {
      if (inField && !EVERYWHERE.has(action.kind)) return;
      // In a terminal, Ctrl+V is the browser's own paste, which xterm.js
      // takes as a paste — bracketed, if the program asked for that. The
      // terminal's own listener for it (TermView) is where files copied in
      // the Explorer become their paths: only there is the event at hand.
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

  // Files dropped on a terminal arrive as their paths, typed where the cursor
  // is, as in any terminal, said as the pane's shell reads them. The pane under
  // the pointer is lit while they are over it: it need not be the one with the
  // keyboard.
  let carrying = false;
  void getCurrentWebview()
    .onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === "leave") {
        carrying = false;
        lightPane(null);
        return;
      }
      if (payload.type === "enter") carrying = payload.paths.length > 0;
      const under = paneAt(payload.position);
      if (payload.type !== "drop") {
        lightPane(carrying ? under : null);
        return;
      }
      carrying = false;
      lightPane(null);
      if (payload.paths.length === 0) return;
      const view =
        (under?.dataset.workspace && under.dataset.pane
          ? terminals.existing(under.dataset.workspace, Number(under.dataset.pane))
          : undefined) ?? activeView();
      if (!view) return;
      const command = workspaces.value.find((w) => w.name === view.workspace)?.tabs.find((t) => t.id === view.tab)?.command ?? "";
      view.paste(dropText(payload.paths, shellKind(command)) + " ");
      view.focus();
    })
    .catch(() => {});
}
