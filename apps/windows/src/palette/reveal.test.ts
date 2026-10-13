// Going to a line of history, against a real terminal buffer (xterm.js
// without a screen): the scroll lands where the Mac's does, the mark goes, and
// the arithmetic of columns holds for a wide letter.
import { Terminal } from "@xterm/xterm";
import { afterEach, describe, expect, it, vi } from "vitest";
import { columnOf } from "./hits";
import { markLine, showLine } from "./reveal";

/** A terminal of `rows` rows holding `lines`, as the daemon's snapshot would write them. */
async function terminalWith(lines: string[], rows = 6): Promise<Terminal> {
  const term = new Terminal({ allowProposedApi: true, cols: 40, rows, scrollback: 200 });
  await new Promise<void>((done) => term.write(lines.join("\r\n"), done));
  return term;
}

const history = (count: number) => Array.from({ length: count }, (_, i) => `linha ${i}`);

afterEach(() => vi.useRealTimers());

describe("the scroll", () => {
  it("puts the line five rows from the bottom, with what follows it", async () => {
    const term = await terminalWith(history(30));
    showLine(term, { text: "linha 10", line: 10, total: 30 }, true);
    // Six rows: the line is the second from the top, the four after it below.
    expect(term.buffer.active.viewportY).toBe(9);
    expect(term.buffer.active.getLine(10)?.translateToString(true)).toBe("linha 10");
    term.dispose();
  });

  it("a line near the end is the bottom, whatever the count", async () => {
    const term = await terminalWith(history(30));
    showLine(term, { text: "linha 28", line: 28, total: 30 }, true);
    expect(term.buffer.active.viewportY).toBe(term.buffer.active.baseY);
    term.dispose();
  });

  it("a line near the start is the top", async () => {
    const term = await terminalWith(history(30));
    showLine(term, { text: "linha 1", line: 1, total: 30 }, true);
    expect(term.buffer.active.viewportY).toBe(0);
    term.dispose();
  });

  it("counts from the newest line when the terminal kept less than the daemon did", async () => {
    // The daemon's copy has 40 lines; this terminal trimmed the first 12.
    const term = await terminalWith(history(40).slice(12));
    showLine(term, { text: "linha 20", line: 20, total: 40 }, true);
    // linha 20 is the ninth line here, at buffer index 8: the view starts at 7.
    expect(term.buffer.active.getLine(8)?.translateToString(true)).toBe("linha 20");
    expect(term.buffer.active.viewportY).toBe(7);
    term.dispose();
  });

  it("a guess still scrolls near the line", async () => {
    const term = await terminalWith(history(30).map((l) => l.toUpperCase()));
    showLine(term, { text: "linha 10", line: 10, total: 30 }, false);
    expect(term.buffer.active.viewportY).toBe(9);
    term.dispose();
  });

  it("leaves a program on the alternate screen where it is", async () => {
    const term = await terminalWith(history(30));
    await new Promise<void>((done) => term.write("\x1b[?1049h\x1b[Hum editor", done));
    const before = term.buffer.active.viewportY;
    showLine(term, { text: "linha 10", line: 10, total: 30 }, true);
    expect(term.buffer.active.type).toBe("alternate");
    expect(term.buffer.active.viewportY).toBe(before);
    term.dispose();
  });

  it("says nothing of an empty terminal", async () => {
    const term = await terminalWith([]);
    expect(() => showLine(term, { text: "x", line: 0, total: 1 }, true)).not.toThrow();
    term.dispose();
  });
});

describe("the mark", () => {
  it("goes after a moment, and moves when asked for again", async () => {
    // The terminal reads what it is written on a timer of its own: it is made first.
    const term = await terminalWith(history(30));
    vi.useFakeTimers();
    const first = vi.spyOn(term, "registerDecoration");
    showLine(term, { text: "linha 10", line: 10, total: 30, span: [0, 5] }, true);
    // The line, and the match in it.
    expect(first).toHaveBeenCalledTimes(2);
    const made = first.mock.results.map((r) => r.value as { isDisposed: boolean });
    showLine(term, { text: "linha 11", line: 11, total: 30, span: [0, 5] }, true);
    expect(made.every((d) => d.isDisposed)).toBe(true);
    vi.advanceTimersByTime(2000);
    const second = first.mock.results.slice(2).map((r) => r.value as { isDisposed: boolean });
    expect(second.every((d) => d.isDisposed)).toBe(true);
    term.dispose();
  });

  it("a match that is not in the line marks the line alone", async () => {
    const term = await terminalWith(history(30));
    const spy = vi.spyOn(term, "registerDecoration");
    const clear = markLine(term, 10, null, false);
    expect(spy).toHaveBeenCalledTimes(1);
    clear();
    term.dispose();
  });

  it("puts the match where the letters are, a wide one counting two columns", async () => {
    const term = await terminalWith(["あbc def"]);
    const spy = vi.spyOn(term, "registerDecoration");
    // "def" is characters 4 to 7 of the text and columns 5 to 8 of the row.
    const clear = markLine(term, 0, [4, 7], true);
    const match = spy.mock.calls[1][0];
    expect([match.x, match.width]).toEqual([5, 3]);
    clear();
    term.dispose();
  });
});

describe("columns in a real row", () => {
  it("a wide letter takes two columns", async () => {
    const term = await terminalWith(["あbc"]);
    const row = term.buffer.active.getLine(0)!;
    expect(columnOf(row, 0)).toBe(0);
    expect(columnOf(row, 1)).toBe(2);
    expect(columnOf(row, 2)).toBe(3);
    term.dispose();
  });
});
