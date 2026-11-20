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
    const cutMidWord = cellsOf(row) >= width && tail > 0 && head > 0 && tail + head > width;
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
export function keysToCut(selected: string, lines: string[], cursorRow: number, cursorCell: number): string | undefined {
  if (cursorRow < 0 || cursorRow >= lines.length) return undefined;
  // The cursor's place in its row's text: an emoji is one character over two cells.
  const cursorColumn = charactersBeforeCell(lines[cursorRow], cursorCell);
  const box = promptBoxRows(lines, cursorRow);
  if (box) {
    // The box's text is as wide as the rule over it, less the gutter and the margin.
    const width = Array.from(lines[box.first - 1].trim()).length - 4;
    return (
      keysAcrossRows(selected, box.rows, width, cursorRow - box.first, cursorColumn - 2) ?? keysEmptying(selected, box.rows)
    );
  }
  return cutKeys(selected, lines[cursorRow], cursorColumn);
}

/** The ranges of the wide East Asian scripts, two cells to a character. */
const WIDE: [number, number][] = [
  [0x1100, 0x115f], [0x2e80, 0x303e], [0x3041, 0x33ff], [0x3400, 0x4dbf], [0x4e00, 0x9fff], [0xa000, 0xa4cf],
  [0xac00, 0xd7a3], [0xf900, 0xfaff], [0xfe30, 0xfe4f], [0xff00, 0xff60], [0xffe0, 0xffe6], [0x20000, 0x3fffd],
];

const graphemes = new Intl.Segmenter(undefined, { granularity: "grapheme" });

/**
 * The cells a cluster takes on screen (graphemes.ts shares one cell between its code points): two for an emoji —
 * a skin tone or a VS16 with it included — and the wide East Asian scripts, one for the rest.
 */
export function cellWidth(cluster: string): number {
  const code = cluster.codePointAt(0) ?? 0;
  if (/\p{Emoji_Presentation}/u.test(String.fromCodePoint(code)) || cluster.includes("\uFE0F")) return 2;
  return WIDE.some(([lo, hi]) => code >= lo && code <= hi) ? 2 : 1;
}

const cellsOf = (row: string) => Array.from(graphemes.segment(row)).reduce((sum, g) => sum + cellWidth(g.segment), 0);

/**
 * How many characters (code points, as everywhere in this file) of `row` come before the cell `cell`, which is where
 * the terminal puts the cursor: an emoji is one cluster over two cells. Past the row's text, each cell is a blank.
 */
export function charactersBeforeCell(row: string, cell: number): number {
  let cells = 0;
  let count = 0;
  for (const { segment } of graphemes.segment(row)) {
    if (cells >= cell) return count;
    cells += cellWidth(segment);
    count += Array.from(segment).length;
  }
  return count + Math.max(0, cell - cells);
}

// ---------------------------------------------------------------- Shift and the side arrows

/** A place in Claude Code's prompt box: a row of the box and a column of its text (the gutter off). */
export interface BoxCell {
  row: number;
  column: number;
}

export const ARROW_LEFT = LEFT;
export const ARROW_RIGHT = RIGHT;

/**
 * Where a left or right arrow takes the cursor in Claude Code's prompt box, given each row's length: one character
 * over, onto the end of the row above or the start of the row below at a row's edge, and nowhere past the box's first
 * or last character (the macOS app's `CutSelection.step`). Shift with an arrow marks from where it began to here.
 * shortcut: a row the box wrapped is taken to end where its text does, so the mark can be a character off at a soft
 * wrap; the text a cut takes is still found by its words (`keysAcrossRows`).
 */
export function arrowStep(cell: BoxCell, left: boolean, rowLengths: number[]): BoxCell {
  if (cell.row < 0 || cell.row >= rowLengths.length) return cell;
  const column = Math.min(Math.max(cell.column, 0), rowLengths[cell.row]);
  if (left) {
    if (column > 0) return { row: cell.row, column: column - 1 };
    return cell.row > 0 ? { row: cell.row - 1, column: rowLengths[cell.row - 1] } : { row: cell.row, column: 0 };
  }
  if (column < rowLengths[cell.row]) return { row: cell.row, column: column + 1 };
  return cell.row + 1 < rowLengths.length ? { row: cell.row + 1, column: 0 } : { row: cell.row, column };
}

/**
 * The terminal's selection for a mark from `anchor` to `focus` in the box whose first row is screen row `first`: the
 * screen cell it starts at (the gutter's two columns put back) and how many cells it runs, row after row, in a screen
 * `cols` wide. The cell at the later end is where the mark stops, not a character in it. Undefined for an empty mark
 * or one the box no longer holds.
 */
export function markSpan(
  anchor: BoxCell,
  focus: BoxCell,
  rowLengths: number[],
  first: number,
  cols: number,
): { row: number; column: number; length: number } | undefined {
  const before = (a: BoxCell, b: BoxCell) => a.row < b.row || (a.row === b.row && a.column < b.column);
  if (anchor.row === focus.row && anchor.column === focus.column) return undefined;
  const [low, high] = before(anchor, focus) ? [anchor, focus] : [focus, anchor];
  if (low.row < 0 || high.row >= rowLengths.length || (high.column === 0 && high.row === 0)) return undefined;
  const last = high.column > 0 ? { row: high.row, column: high.column - 1 } : { row: high.row - 1, column: Math.max(0, rowLengths[high.row - 1] - 1) };
  const start = { row: first + low.row, column: low.column + 2 };
  const end = { row: first + last.row, column: last.column + 2 };
  const length = (end.row - start.row) * cols + end.column - start.column + 1;
  return length > 0 ? { ...start, length } : undefined;
}
