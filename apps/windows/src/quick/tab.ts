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
