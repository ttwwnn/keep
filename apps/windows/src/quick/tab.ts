// Which tab of the daemon's `quick` workspace the panel shows, and how big to
// make one when there is none.

import type { WorkspaceInfo } from "../api";
import { isQuickWorkspace } from "../quickWorkspace";

/**
 * The tab the panel looks at: the first live one of the workspace, which is
 * what the daemon's TAB_ANY means and the Mac asks for. A panel has one
 * terminal; whatever else the workspace holds (a pane made inside it with
 * `keep split`) is behind it. Null when there is none, and one is to be made.
 */
export function quickTabOf(list: readonly WorkspaceInfo[]): number | null {
  const workspace = list.find((w) => isQuickWorkspace(w.name));
  return workspace?.tabs.find((tab) => !tab.finished)?.id ?? null;
}

/**
 * The grid a terminal of this box would have, to make a tab before there is a
 * terminal to measure: the real size is sent when the terminal attaches, so
 * this is only where the shell starts out.
 */
export function guessGrid(width: number, height: number, fontSize: number): { cols: number; rows: number } {
  return {
    cols: Math.max(20, Math.floor(width / (fontSize * 0.6))),
    rows: Math.max(5, Math.floor(height / (fontSize * 1.2))),
  };
}

/** The shell the panel shows: the daemon's tab, and which terminal of the page is on it. */
export interface Shell {
  id: number;
  /** Counts the terminals the page has built for the panel: a new one for a number it has used before is still new. */
  born: number;
}

/**
 * What the panel does about the tab the daemon has named: keep the terminal it
 * has, or build another. Another when the tab is a different one — and also
 * when it is the same number with a dead terminal on it, because the daemon
 * counts from one again in a workspace it has emptied: the shell that ended
 * and the one that took its number are two, and the terminal of the first
 * never attaches to the second.
 */
export function nextShell(current: Shell | null, id: number, deadHere: boolean): Shell {
  if (current !== null && current.id === id && !deadHere) return current;
  return { id, born: (current?.born ?? 0) + 1 };
}
