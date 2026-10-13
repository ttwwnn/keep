// The picture of a screen beside the list: what it is a picture of, and the
// arithmetic of fitting it in its frame.

import type { Found } from "./hits";

/** A screen to draw: a pane's, on the grid the daemon holds it on — cut to a smaller one, a full-screen program would be drawn wrong. */
export interface PreviewTarget {
  workspace: string;
  pane: number;
  cols: number;
  rows: number;
  /** A line of history to bring up, and the match in it. */
  hit?: Found;
}

/** The most history a preview of a hit keeps; past it the oldest lines are not drawn, and a hit among them is shown as the oldest line there is. */
export const MAX_HISTORY = 20_000;

/** What `preview` says, as a terminal reads it: lines end in CR LF. */
export function plainAsScreen(text: string): string {
  return text.replace(/\r?\n/g, "\r\n");
}

/**
 * How many rows at the top of a screen to leave out of its frame so what is
 * last on it is in view: none when it all fits (a fresh shell has its prompt at
 * the top and blank rows under it), and otherwise just enough to bring the last
 * row with something on it to the bottom of the frame — where a program that
 * draws a screen of its own keeps its prompt.
 */
export function rowsToSkip(lastRow: number, cell: number, room: number): number {
  if (cell <= 0 || lastRow < 0) return 0;
  const shown = Math.max(1, Math.floor(room / cell));
  return Math.max(0, lastRow + 1 - shown);
}

/** The first row of a view that puts `index` in the middle of `shown` rows, without reaching past either end of the buffer. */
export function topFor(index: number, shown: number, bottom: number): number {
  return Math.min(Math.max(0, index - Math.floor(shown / 2)), Math.max(0, bottom));
}

/** How much history a preview of this target keeps: none for a plain screen, all there is (up to the limit) for a hit. */
export function historyFor(target: PreviewTarget): number {
  return target.hit ? Math.min(MAX_HISTORY, target.hit.total + target.rows) : 0;
}
