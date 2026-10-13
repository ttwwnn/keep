// A line of history the daemon found, and where it is: in the line, in the
// terminal that shows it.

/** What the daemon says of a hit that placing it needs. */
export interface Found {
  text: string;
  /** Counted from the oldest line of the tab's history. */
  line: number;
  /** How many lines that history holds. */
  total: number;
}

/**
 * A match as the daemon reports it — a byte range of the line's UTF-8 — as
 * the range of UTF-16 units a JavaScript string is cut by, or null when it
 * does not fit the line.
 */
export function matchSpan(text: string, start: number, length: number): [number, number] | null {
  const bytes = new TextEncoder().encode(text);
  if (length <= 0 || start < 0 || start + length > bytes.length) return null;
  const decoder = new TextDecoder();
  const from = decoder.decode(bytes.subarray(0, start)).length;
  return [from, from + decoder.decode(bytes.subarray(start, start + length)).length];
}

/** The lines of a terminal's buffer, as text. */
export interface Lines {
  length: number;
  text(index: number): string;
}

/** How far from where it should be a line is looked for when it is not there. */
const NEARBY = 200;

/**
 * Where in a terminal's buffer a hit is. Counted back from the newest line,
 * not forward from the oldest: a terminal trims its history from the oldest
 * end, and the daemon's copy and the terminal's disagree about how much is
 * left there, so the end they share is the one that can be counted from.
 * The newest line is the last one with something on it — the daemon leaves
 * out the blank rows under the output, and the screen has them.
 *
 * Said back is whether the line found is the line asked for: the text there
 * is the hit's own. When it is not (the terminal printed more since, or
 * reflowed), the closest line that is wins; failing that, the count's answer
 * stands, flagged as a guess. Null when there is nothing in the buffer.
 */
export function locateLine(lines: Lines, hit: Found): { index: number; exact: boolean } | null {
  let newest = lines.length - 1;
  while (newest >= 0 && lines.text(newest).trim() === "") newest--;
  if (newest < 0) return null;
  const fromEnd = hit.total > hit.line ? hit.total - 1 - hit.line : 0;
  const guess = newest - fromEnd;
  // Older than anything this terminal kept: the oldest line it has is as near as it gets.
  if (guess < 0) return { index: 0, exact: false };
  const wanted = hit.text.trimEnd();
  const is = (index: number) => index >= 0 && index <= newest && lines.text(index).trimEnd() === wanted;
  if (is(guess)) return { index: guess, exact: true };
  for (let away = 1; away <= NEARBY; away++) {
    if (is(guess - away)) return { index: guess - away, exact: true };
    if (is(guess + away)) return { index: guess + away, exact: true };
  }
  return { index: guess, exact: false };
}
