// Going to a line of history: the terminal scrolled to it, and the line marked
// for a moment so the eye finds it.

import type { Terminal } from "@xterm/xterm";
import { palette, terminals } from "../store";
import { columnOf, locateLine, type Found } from "./hits";

/** How long the mark stays. */
const FLASH_MS = 1800;

/**
 * The line a muted band and the match a bright one, in colours that read on a
 * dark theme and on a light one: the decorations take `#RRGGBB` only, so the
 * theme is chosen for rather than blended into.
 */
function colours(dark: boolean) {
  return {
    band: dark ? { backgroundColor: "#4d4117", foregroundColor: "#f5f5f5" } : { backgroundColor: "#fff0b3", foregroundColor: "#1f2328" },
    match: { backgroundColor: "#ffd60a", foregroundColor: "#1d1f21" },
  };
}

/** Mark a line of a terminal's buffer, and the match in it; the function takes the mark away. */
export function markLine(term: Terminal, index: number, span: [number, number] | null | undefined, dark: boolean): () => void {
  const buffer = term.buffer.active;
  const marker = term.registerMarker(index - (buffer.baseY + buffer.cursorY));
  if (!marker) return () => {};
  const tint = colours(dark);
  const made = [term.registerDecoration({ marker, width: term.cols, layer: "bottom", ...tint.band })];
  const row = buffer.getLine(index);
  if (span && row) {
    const from = columnOf(row, span[0]);
    const to = columnOf(row, span[1]);
    if (to > from) made.push(term.registerDecoration({ marker, x: from, width: to - from, layer: "top", ...tint.match }));
  }
  return () => {
    for (const decoration of made) decoration?.dispose();
    marker.dispose();
  };
}

/** One mark at a time per terminal: asking again moves it. */
const flashing = new WeakMap<Terminal, () => void>();

/**
 * Put a line of a terminal's history on screen. Counted from the newest line
 * (see `locateLine`), and a few rows short of the bottom, so it arrives with
 * what follows it rather than pinned to the last row. The line is marked when
 * it is certainly the one: a guess scrolls near it and claims nothing.
 */
export function showLine(term: Terminal, hit: Found, dark = palette.value.dark): void {
  const buffer = term.buffer.active;
  // A program on the alternate screen has no history to scroll through.
  if (buffer.type === "alternate") return;
  const located = locateLine({ length: buffer.length, text: (i) => buffer.getLine(i)?.translateToString(true) ?? "" }, hit);
  if (!located) return;
  term.scrollToLine(Math.min(Math.max(0, located.index - (term.rows - 5)), buffer.baseY));
  flashing.get(term)?.();
  flashing.delete(term);
  if (!located.exact) return;
  const clear = markLine(term, located.index, hit.span, dark);
  const timer = setTimeout(() => {
    clear();
    flashing.delete(term);
  }, FLASH_MS);
  flashing.set(term, () => {
    clearTimeout(timer);
    clear();
  });
}

/**
 * Go to a hit's line in its pane. Asked twice: a pane opened by the search
 * may still be receiving its screen, whose arrival lands the view back at the
 * bottom, and asking again once it has settled costs nothing when the first
 * time already worked.
 */
export function revealHit(workspace: string, pane: number, hit: Found): void {
  const apply = () => {
    const view = terminals.existing(workspace, pane);
    if (view) showLine(view.term, hit);
  };
  apply();
  setTimeout(apply, 700);
}
