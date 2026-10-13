// What is done with panes: moved, taken out to a tab of their own, and moved
// about by keyboard. The arrangement is the daemon's; this asks it, and
// believes what it then says rather than guessing what it did.

import * as api from "./api";
import { movesForDetach, movesForDrop, type DropSide, type Move, type PaneRecord } from "./paneMoves";
import { clampShare, dividerFor, paneBeside, setShare, shareOf, splitKey, stepShare, type Box, type Direction } from "./splits";
import { tabKey } from "./state";
import {
  activePane,
  activeTab,
  activeView,
  activeWorkspace,
  focusActive,
  focusPane,
  refresh,
  say,
  select,
  tabsByWorkspace,
  terminals,
  workspaces,
} from "./store";

/** Every pane of a workspace as the daemon records it, in the order it lists them. */
function recordsOf(workspace: string): PaneRecord[] {
  const tabs = workspaces.value.find((w) => w.name === workspace)?.tabs ?? [];
  return tabs.filter((t) => !t.finished).map((t) => ({ id: t.id, splitOf: t.splitOf, splitDir: t.splitDir }));
}

/**
 * Ask the daemon to move panes, then believe the daemon rather than guessing
 * what it did: the arrangement is its fact, and a rejected move must leave
 * the window showing what is actually there. The tab that holds the pane now
 * comes forward, with the pane holding the keyboard.
 */
async function arrange(workspace: string, moves: Move[], pane: number): Promise<void> {
  try {
    await api.rearrange(workspace, moves);
  } catch (error) {
    say(`Não deu para mover o painel: ${String(error)}`);
  }
  await refresh();
  const holder = (tabsByWorkspace.value.get(workspace) ?? []).find((t) => t.panes.some((p) => p.id === pane));
  if (holder) {
    select(workspace, holder.root.id);
    focusPane(workspace, holder.root.id, pane);
  }
  requestAnimationFrame(() => focusActive());
}

/** Drop a pane on a side of another, in the same tab or in another of the workspace. */
export async function movePane(workspace: string, pane: number, target: number, side: DropSide): Promise<void> {
  const moves = movesForDrop(recordsOf(workspace), pane, target, side);
  if (moves) await arrange(workspace, moves, pane);
}

/** Take a pane out of its tab, to be a tab of its own. */
export async function detachPane(workspace: string, pane: number): Promise<void> {
  const moves = movesForDetach(recordsOf(workspace), pane);
  if (moves) await arrange(workspace, moves, pane);
}

/** A pane to hang something off when a drop names the tab and not a place in it: the one with the keyboard, or the root. */
export function anchorPane(workspace: string, root: number, focused: number | undefined): number {
  const tab = (tabsByWorkspace.value.get(workspace) ?? []).find((t) => t.root.id === root);
  if (!tab) return root;
  return focused !== undefined && tab.panes.some((p) => p.id === focused) ? focused : tab.root.id;
}

// ------------------------------------------------------------------ by keyboard

/** The panes of the tab in front, where they are on screen. */
function boxesOnScreen(): Map<number, Box> {
  const boxes = new Map<number, Box>();
  for (const el of document.querySelectorAll<HTMLElement>(".tab-view.shown .pane[data-pane]")) {
    const r = el.getBoundingClientRect();
    boxes.set(Number(el.dataset.pane), { left: r.left, top: r.top, right: r.right, bottom: r.bottom });
  }
  return boxes;
}

/** Whether the tab in front has panes to move about: only then are the arrow chords the window's. */
export function activeTabHasPanes(): boolean {
  return (activeTab.value?.panes.length ?? 0) > 1;
}

/** Give the keyboard to the pane beside the one that has it. */
export function focusBeside(direction: Direction): boolean {
  const workspace = activeWorkspace.value;
  const tab = activeTab.value;
  const from = activePane.value;
  if (!workspace || !tab || !from || tab.panes.length < 2) return false;
  const next = paneBeside(boxesOnScreen(), from.id, direction);
  if (next === null) return false;
  focusPane(workspace, tab.root.id, next);
  terminals.existing(workspace, next)?.focus();
  return true;
}

/** How far an arrow with Shift moves a divider, in pixels: the person's own binding for it on the Mac is twenty. */
export const RESIZE_STEP = 20;

/** The fewest columns and rows a pane may be left with by a divider. */
export const LEAST_COLS = 12;
export const LEAST_ROWS = 4;

/** What a side of a split needs, as a share of the split's length along the way it runs. */
export function leastShare(vertical: boolean, length: number): number {
  if (length <= 0) return 0;
  const view = activeView();
  const screen = view?.element.querySelector<HTMLElement>(".xterm-screen");
  const cell = screen && view ? (vertical ? screen.clientWidth / Math.max(1, view.term.cols) : screen.clientHeight / Math.max(1, view.term.rows)) : vertical ? 9 : 18;
  // The terminal's own padding is a pixel or two each side.
  return (cell * (vertical ? LEAST_COLS : LEAST_ROWS) + (vertical ? 4 : 0)) / length;
}

/** Move the divider the pane that has the keyboard sits against, the way the arrow points. */
export function resizeBeside(direction: Direction): boolean {
  const workspace = activeWorkspace.value;
  const tab = activeTab.value;
  const pane = activePane.value;
  if (!workspace || !tab || !pane || tab.panes.length < 2) return false;
  const split = dividerFor(tab.tree, pane.id, direction);
  if (!split) return false;
  const key = splitKey(tabKey(workspace, tab.root.id), split);
  const element = [...document.querySelectorAll<HTMLElement>(".tab-view.shown .split")].find((e) => e.dataset.split === key);
  if (!element) return false;
  const box = element.getBoundingClientRect();
  const length = split.vertical ? box.width : box.height;
  const least = leastShare(split.vertical, length);
  setShare(key, clampShare(stepShare(shareOf(key), direction, RESIZE_STEP, length), least, least));
  return true;
}
