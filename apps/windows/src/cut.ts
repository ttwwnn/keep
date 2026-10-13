// Cutting what is selected out of the line a program is editing.
//
// A port of CutSelection (Model/CutSelection.swift) and of how the Mac's cut
// uses it (Display/SurfaceView.swift). The terminal's selection is a view of
// the screen, not of the program's text, so it cannot be deleted from here;
// the program is told with keys instead, as a person would: move to the end
// of the selection, then backspace over it. That works on the row the cursor
// is on — the prompt of a shell — and across the rows of Claude Code's prompt
// box, whose text is pieced back from its rows; anywhere else a cut is only a
// copy.

import { cutKeys } from "./text";

const RIGHT = "\x1b[C";
const LEFT = "\x1b[D";
const BACKSPACE = "\x7f";

/** A rule across the screen: Claude Code draws one above and one below its prompt box. */
const isRule = (line: string) => line.trim().startsWith("─".repeat(12));

const isBlank = (c: string) => /\s/.test(c);

const squeezed = (text: string) => Array.from(text).filter((c) => !isBlank(c)).join("");

/**
 * The rows of the prompt box the cursor is in — between the rule above it and
 * the rule below — with the box's gutter (`❯` and a blank on the first row,
 * two blanks on the rest) taken off, and the screen row the box's first row is
 * on. Undefined when the cursor is in no box.
 */
export function promptBoxRows(lines: string[], cursorRow: number): { rows: string[]; first: number } | undefined {
  if (cursorRow < 0 || cursorRow >= lines.length || isRule(lines[cursorRow])) return undefined;
  let above = -1;
  for (let i = cursorRow - 1; i >= 0; i--) {
    if (isRule(lines[i])) {
      above = i;
      break;
    }
  }
  let below = -1;
  for (let i = cursorRow + 1; i < lines.length; i++) {
    if (isRule(lines[i])) {
      below = i;
      break;
    }
  }
  if (above < 0 || below < 0) return undefined;
  const rows = lines.slice(above + 1, below).map((line) => {
    const chars = Array.from(line);
    // Claude Code writes `❯` and a no-break space; a shell, `> `.
    if (chars[0] === "❯" || chars[0] === ">") {
      return chars.slice(chars.length > 1 && isBlank(chars[1]) ? 2 : 1).join("");
    }
    return line.startsWith("  ") ? line.slice(2) : line;
  });
  if (rows.length === 0 || rows[0].trim() === "") return undefined;
  return { rows, first: above + 1 };
}

/**
 * The keys that empty the prompt when what was selected is the whole of it
 * (however its rows were wrapped): to the end, and back over all of it. More of
 * either than needed does nothing at the ends, so the rows' breaks — a typed new
 * line or only a wrap, which the screen cannot tell apart — are covered by
 * counting each as one.
 */
export function keysEmptying(selected: string, box: string[]): string | undefined {
  const wanted = squeezed(selected);
  if (wanted === "" || wanted !== squeezed(box.join("\n"))) return undefined;
  const count = box.reduce((sum, row) => sum + Array.from(row).length, 0) + box.length;
  return RIGHT.repeat(count) + BACKSPACE.repeat(count);
}

/**
 * The text of a prompt box as the program holds it, rebuilt from its rows:
 * each break between two rows stands for one character — the space the wrap
 * swallowed, or a new line typed there — except where a word too long for the
 * row was cut in the middle, which stands for none. Breaks are kept as `\n`, a
 * blank either way. `starts[i]` is where row `i` begins in the text.
 */
export function logical(box: string[], width: number): { text: string[]; starts: number[] } {
  const text: string[] = [];
  const starts: number[] = [];
  box.forEach((row, i) => {
    starts.push(text.length);
    const chars = Array.from(row);
    text.push(...chars);
    if (i === box.length - 1) return;
    // A word longer than the row is cut where the row ends (Claude Code wraps
    // hard): the row is full, and the word across the break — its end here,
    // its start below — is longer than a row. A word that fits would have
    // gone down whole, leaving a blank.
    let tail = 0;
    for (let k = chars.length - 1; k >= 0 && !isBlank(chars[k]); k--) tail++;
    const nextChars = Array.from(box[i + 1]);
    let head = 0;
    while (head < nextChars.length && !isBlank(nextChars[head])) head++;
    const cutMidWord = chars.length >= width && tail > 0 && head > 0 && tail + head > width;
    if (!cutMidWord) text.push("\n");
  });
  return { text, starts };
}

/**
 * The keys that delete `selected` from Claude Code's prompt box when it spans
 * rows. The selection is found in the box's text with the blanks set aside — a
 * copy of wrapped rows may carry new lines where the text has spaces — and must
 * be found there exactly once. The cursor is at `cursorRow`, `cursorColumn` of
 * the box (gutter off).
 */
export function keysAcrossRows(
  selected: string,
  box: string[],
  width: number,
  cursorRow: number,
  cursorColumn: number,
): string | undefined {
  if (cursorRow < 0 || cursorRow >= box.length) return undefined;
  const { text, starts } = logical(box, width);
  const solid: number[] = [];
  text.forEach((c, i) => {
    if (!isBlank(c)) solid.push(i);
  });
  const wanted = Array.from(selected).filter((c) => !isBlank(c));
  if (wanted.length === 0 || solid.length < wanted.length) return undefined;
  const found: number[] = [];
  for (let at = 0; at + wanted.length <= solid.length; at++) {
    if (wanted.every((c, k) => text[solid[at + k]] === c)) found.push(at);
  }
  if (found.length !== 1) return undefined;
  const at = found[0];
  let start = solid[at];
  let end = solid[at + wanted.length - 1] + 1;
  // Blanks the selection begins or ends with go too.
  const chars = Array.from(selected);
  let leading = 0;
  while (leading < chars.length && isBlank(chars[leading])) leading++;
  while (leading > 0 && start > 0 && isBlank(text[start - 1])) {
    start--;
    leading--;
  }
  let trailing = 0;
  while (trailing < chars.length && isBlank(chars[chars.length - 1 - trailing])) trailing++;
  while (trailing > 0 && end < text.length && isBlank(text[end])) {
    end++;
    trailing--;
  }
  const cursor = starts[cursorRow] + Math.min(Math.max(cursorColumn, 0), Array.from(box[cursorRow]).length);
  const move = end - cursor;
  return (move >= 0 ? RIGHT.repeat(move) : LEFT.repeat(-move)) + BACKSPACE.repeat(end - start);
}

/**
 * The keys that take `selected` out of what the program is editing, given the
 * screen (`lines`, its rows) and where the cursor is on it: across the rows of
 * Claude Code's prompt box when it is in one, else on the cursor's own row.
 * Undefined when that cannot be told, where a cut is only a copy.
 */
export function keysToCut(selected: string, lines: string[], cursorRow: number, cursorColumn: number): string | undefined {
  const box = promptBoxRows(lines, cursorRow);
  if (box) {
    // The box's text is as wide as the rule over it, less the gutter and the margin.
    const width = Array.from(lines[box.first - 1].trim()).length - 4;
    return (
      keysAcrossRows(selected, box.rows, width, cursorRow - box.first, cursorColumn - 2) ?? keysEmptying(selected, box.rows)
    );
  }
  if (cursorRow < 0 || cursorRow >= lines.length) return undefined;
  return cutKeys(selected, lines[cursorRow], cursorColumn);
}
