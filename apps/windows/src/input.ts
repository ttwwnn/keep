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
import { ARROW_LEFT, ARROW_RIGHT, arrowStep, keysToCut, markSpan, promptBoxRows, type BoxCell } from "./cut";
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
  const shiftSide =
    (event.key === "ArrowLeft" || event.key === "ArrowRight") && event.shiftKey && !event.ctrlKey && !event.altKey && !event.metaKey;
  if (shiftSide) {
    // Shift with a side arrow marks text in Claude Code's prompt box, as in any text box.
    if (event.type === "keydown" && markByArrow(view, event.key === "ArrowLeft")) {
      event.preventDefault();
      return false;
    }
    if (event.type !== "keydown" && marks.has(view)) return false;
  } else if (event.type === "keydown" && marks.has(view) && !MODIFIERS.has(event.key)) {
    // Any other key lets the mark go: an arrow without Shift moves off it, Backspace and Delete take it out.
    marks.delete(view);
    const erase = (event.key === "Backspace" || event.key === "Delete") && !event.ctrlKey && !event.altKey && !event.metaKey && !event.shiftKey;
    if (erase && view.term.hasSelection()) {
      event.preventDefault();
      const buffer = view.term.buffer.active;
      const keys = keysToCut(view.term.getSelection(), view.screenText().split("\n"), buffer.cursorY, buffer.cursorX);
      view.term.clearSelection();
      if (keys) void view.type(keys).catch(() => {});
      return false;
    }
    if (event.key.startsWith("Arrow")) view.term.clearSelection();
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

const MODIFIERS = new Set(["Shift", "Control", "Alt", "AltGraph", "Meta", "CapsLock"]);

/** The mark Shift and the side arrows are making in Claude Code's prompt box, per pane: where it began and where the cursor is. */
const marks = new Map<TermView, { anchor: BoxCell; focus: BoxCell }>();
/** Only the latest arrow's drawing runs. */
let markTurn = 0;

// A click lets every mark go: a selection made with the mouse is the person's own.
if (typeof window !== "undefined") window.addEventListener("mousedown", () => marks.clear(), true);

/**
 * Shift with a side arrow in Claude Code's prompt box marks text from the cursor, as in any text box (the macOS app's
 * `markByArrow`, 0.3.53). Claude Code only moves its cursor for it, so the arrow goes to it plain, and once its cursor
 * has moved the terminal's selection is drawn from where the mark began to there, drawn whole each time (the program
 * writes its line again at every arrow). Ctrl+C, Ctrl+X, Backspace and Delete act on it. False outside the box: the
 * key goes to the program as it is.
 */
function markByArrow(view: TermView, left: boolean): boolean {
  const buffer = view.term.buffer.active;
  const box = promptBoxRows(view.screenText().split("\n"), buffer.cursorY);
  if (!box) return false;
  const lengths = box.rows.map((row) => Array.from(row).length);
  const row = buffer.cursorY - box.first;
  const here = { row, column: Math.min(Math.max(buffer.cursorX - 2, 0), lengths[row]) };
  let mark = marks.get(view) ?? { anchor: here, focus: here };
  // The box was drawn again with fewer rows than the mark spans: start over from the cursor.
  if (mark.anchor.row >= lengths.length || mark.focus.row >= lengths.length) mark = { anchor: here, focus: here };
  const next = arrowStep(mark.focus, left, lengths);
  marks.set(view, mark);
  // At the box's first or last character there is nowhere to go.
  if (next.row === mark.focus.row && next.column === mark.focus.column) return true;
  marks.set(view, { anchor: mark.anchor, focus: next });
  void view.type(left ? ARROW_LEFT : ARROW_RIGHT).catch(() => {});
  drawArrowMark(view, ++markTurn, 30);
  return true;
}

/** The mark drawn once the program's cursor is where the mark ends (or after a third of a second, at a soft wrap where the two may differ). */
function drawArrowMark(view: TermView, turn: number, tries: number): void {
  setTimeout(() => {
    const mark = marks.get(view);
    if (turn !== markTurn || !mark) return;
    const buffer = view.term.buffer.active;
    const box = promptBoxRows(view.screenText().split("\n"), buffer.cursorY);
    if (!box) return;
    const at = { row: buffer.cursorY - box.first, column: buffer.cursorX - 2 };
    if ((at.row !== mark.focus.row || at.column !== mark.focus.column) && tries > 0) {
      drawArrowMark(view, turn, tries - 1);
      return;
    }
    view.term.clearSelection();
    const span = markSpan(mark.anchor, mark.focus, box.rows.map((row) => Array.from(row).length), box.first, view.term.cols);
    if (span) view.term.select(span.column, buffer.baseY + span.row, span.length);
  }, 10);
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
  if (!keys) return;
  // Claude Code takes its own selection out on a single backspace: the keys worked out for the text would take it out on
  // the first and then delete as many characters again before it.
  void view.type(own === undefined ? "\x7f" : keys).catch(() => {});
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
      typeDropped(payload.paths, under);
    })
    .catch(() => {});
}

/**
 * Files let go over a pane (`under`, else the pane in front): their paths typed where the cursor is, said as the
 * pane's shell reads them, and the keyboard given to it. From the system's drop and from the files panel's rows.
 */
export function typeDropped(paths: string[], under: HTMLElement | null): void {
  if (paths.length === 0) return;
  const view =
    (under?.dataset.workspace && under.dataset.pane
      ? terminals.existing(under.dataset.workspace, Number(under.dataset.pane))
      : undefined) ?? activeView();
  if (!view) return;
  const command = workspaces.value.find((w) => w.name === view.workspace)?.tabs.find((t) => t.id === view.tab)?.command ?? "";
  view.paste(dropText(paths, shellKind(command)) + " ");
  view.focus();
}
