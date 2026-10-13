// A terminal's text, scrollback and all, as the escape sequences that draw it
// again — for a tab put to sleep (hibernation.ts), whose terminal is let go of
// and whose daemon repaints only the screen. Written into the terminal that
// takes its place, it gives back the lines that had scrolled off, with their
// colours; the repaint then clears the screen and draws what is there now.
//
// What the screen held at the moment of sleeping is written too, so that the
// lines above it land in the scrollback exactly as they were. The cursor,
// the modes and the alternate screen are the repaint's to say.

import type { IBuffer, IBufferCell } from "@xterm/xterm";

const RESET = "\x1b[0m";

/** The cell's look, as the parts a change of it can be told by. */
interface Look {
  fgMode: number;
  fg: number;
  bgMode: number;
  bg: number;
  flags: number;
}

const NONE: Look = { fgMode: 0, fg: 0, bgMode: 0, bg: 0, flags: 0 };

function lookOf(cell: IBufferCell): Look {
  const flags =
    (cell.isBold() ? 1 : 0) |
    (cell.isItalic() ? 2 : 0) |
    (cell.isDim() ? 4 : 0) |
    (cell.isUnderline() ? 8 : 0) |
    (cell.isBlink() ? 16 : 0) |
    (cell.isInverse() ? 32 : 0) |
    (cell.isInvisible() ? 64 : 0) |
    (cell.isStrikethrough() ? 128 : 0) |
    (cell.isOverline() ? 256 : 0);
  // A default colour has no number of its own (xterm says -1): it is none.
  return {
    fgMode: cell.getFgColorMode(),
    fg: cell.isFgDefault() ? 0 : cell.getFgColor(),
    bgMode: cell.getBgColorMode(),
    bg: cell.isBgDefault() ? 0 : cell.getBgColor(),
    flags,
  };
}

const same = (a: Look, b: Look) =>
  a.fgMode === b.fgMode && a.fg === b.fg && a.bgMode === b.bgMode && a.bg === b.bg && a.flags === b.flags;

/** A colour as the parameters that set it: the 16 by their short codes, the rest by index or by value. */
function colour(rgb: boolean, palette: boolean, value: number, base: number, bright: number, extended: number): string[] {
  if (rgb) return [`${extended};2;${(value >> 16) & 255};${(value >> 8) & 255};${value & 255}`];
  if (!palette) return [];
  if (value < 8) return [String(base + value)];
  if (value < 16) return [String(bright + value - 8)];
  return [`${extended};5;${value}`];
}

/** The sequence that sets a look from nothing at all. */
function setLook(cell: IBufferCell, look: Look): string {
  if (same(look, NONE)) return RESET;
  const codes: string[] = [];
  const names = [1, 3, 2, 4, 5, 7, 8, 9, 53];
  for (let bit = 0; bit < names.length; bit++) if (look.flags & (1 << bit)) codes.push(String(names[bit]));
  codes.push(...colour(cell.isFgRGB(), cell.isFgPalette(), look.fg, 30, 90, 38));
  codes.push(...colour(cell.isBgRGB(), cell.isBgPalette(), look.bg, 40, 100, 48));
  return `\x1b[0;${codes.join(";")}m`;
}

/**
 * Every line of the buffer, the scrolled-off ones and the screen's, as text
 * and the sequences that colour it. A line that wrapped is written without a
 * break, so that it wraps again at the width of the terminal it is written
 * into, whatever that is.
 */
export function serializeBuffer(buffer: IBuffer, cols: number): string {
  const out: string[] = [];
  const cell = buffer.getNullCell();
  let current: Look = NONE;
  for (let y = 0; y < buffer.length; y++) {
    const line = buffer.getLine(y);
    if (!line) continue;
    const continues = buffer.getLine(y + 1)?.isWrapped === true;
    // The last cell worth writing: past it a line is the default blank, which
    // a line that goes on without a break cannot do without.
    let end = Math.min(line.length, cols);
    if (!continues) {
      while (end > 0) {
        const c = line.getCell(end - 1, cell);
        if (!c || (c.getChars() !== "" && c.getChars() !== " ") || !same(lookOf(c), NONE)) break;
        end--;
      }
    }
    for (let x = 0; x < end; x++) {
      const c = line.getCell(x, cell);
      if (!c || c.getWidth() === 0) continue;
      const look = lookOf(c);
      if (!same(look, current)) {
        out.push(setLook(c, look));
        current = look;
      }
      out.push(c.getChars() || " ");
    }
    if (y < buffer.length - 1 && !continues) {
      // The break does not paint: a colour left on would fill the line it opens.
      if (!same(current, NONE)) {
        out.push(RESET);
        current = NONE;
      }
      out.push("\r\n");
    }
  }
  if (!same(current, NONE)) out.push(RESET);
  return out.join("");
}
