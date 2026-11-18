// Cutting out of what a program is editing: the macOS app's cases
// (tools/cut-test/main.swift), and the real prompt boxes it recorded from
// Claude Code, each cut tried on the real editor and found right.
import { describe, expect, it } from "vitest";
import { arrowStep, charactersBeforeCell, keysAcrossRows, keysEmptying, keysToCut, logical, markSpan, promptBoxRows } from "./cut";

// Shift with an arrow: where the cursor goes, one character at a time (the macOS app's cases), and the terminal's
// selection drawn for the mark.
describe("Shift with the side arrows in Claude Code's prompt box", () => {
  const stepped = (row: number, column: number, left: boolean) => {
    const next = arrowStep({ row, column }, left, [5, 3]);
    return `${next.row},${next.column}`;
  };
  it("moves one character, across the rows, and not past the box", () => {
    expect(stepped(0, 3, true)).toBe("0,2");
    expect(stepped(0, 3, false)).toBe("0,4");
    expect(stepped(1, 0, true)).toBe("0,5");
    expect(stepped(0, 5, false)).toBe("1,0");
    expect(stepped(0, 0, true)).toBe("0,0");
    expect(stepped(1, 3, false)).toBe("1,3");
  });

  it("selects from where the mark began to the cursor, the gutter put back", () => {
    // The box's first row is screen row 10, 80 columns wide.
    expect(markSpan({ row: 0, column: 2 }, { row: 0, column: 5 }, [8, 4], 10, 80)).toEqual({ row: 10, column: 4, length: 3 });
    // Marked leftwards: the same cells.
    expect(markSpan({ row: 0, column: 5 }, { row: 0, column: 2 }, [8, 4], 10, 80)).toEqual({ row: 10, column: 4, length: 3 });
    // Across a row: from the anchor to the end of the screen row, then the next row's gutter and text.
    expect(markSpan({ row: 0, column: 6 }, { row: 1, column: 2 }, [8, 4], 10, 80)).toEqual({ row: 10, column: 8, length: 76 });
    // Stopping at a row's start: the mark ends with the row above.
    expect(markSpan({ row: 0, column: 6 }, { row: 1, column: 0 }, [8, 4], 10, 80)).toEqual({ row: 10, column: 8, length: 2 });
  });

  it("an empty mark, or one the box no longer holds, selects nothing", () => {
    expect(markSpan({ row: 0, column: 3 }, { row: 0, column: 3 }, [8], 10, 80)).toBeUndefined();
    expect(markSpan({ row: 0, column: 3 }, { row: 2, column: 1 }, [8, 4], 10, 80)).toBeUndefined();
  });
});
// The Mac's own recording, where it is kept: tools/cut-test.
import recording from "../../../tools/cut-test/claude-prompt-box.jsonl?raw";

const R = "\x1b[C";
const L = "\x1b[D";
const B = "\x7f";
const hex = (text: string) => [...new TextEncoder().encode(text)].map((b) => b.toString(16).padStart(2, "0")).join("");

describe("the row the cursor is on", () => {
  const row = "❯ ola mundo cruel";
  it("backspaces over the selection from where the cursor is", () => {
    expect(keysToCut("mundo", [row], 0, 17)).toBe(L.repeat(6) + B.repeat(5));
    expect(keysToCut("mundo", [row], 0, 11)).toBe(B.repeat(5));
    expect(keysToCut("mundo", [row], 0, 2)).toBe(R.repeat(9) + B.repeat(5));
  });

  it("only copies what it cannot tell apart or find", () => {
    expect(keysToCut("a", ["banana"], 0, 6)).toBeUndefined();
    expect(keysToCut("a\nb", ["a"], 0, 1)).toBeUndefined();
    expect(keysToCut("zzz", [row], 0, 5)).toBeUndefined();
    expect(keysToCut("", [row], 0, 5)).toBeUndefined();
    expect(keysToCut("x", [row], 7, 5)).toBeUndefined();
  });
});

const rule = "─".repeat(40);
const screen = ["conversa", rule, "❯ primeira linha do pedido que", "  continua aqui", rule, "  ⏵⏵ bypass permissions on"];

describe("the prompt box", () => {
  it("is found around the cursor, gutter off", () => {
    expect(promptBoxRows(screen, 3)?.rows).toEqual(["primeira linha do pedido que", "continua aqui"]);
    expect(promptBoxRows(screen, 3)?.first).toBe(2);
    expect(promptBoxRows(screen, 0)).toBeUndefined();
    expect(promptBoxRows(screen, 1)).toBeUndefined();
    expect(promptBoxRows([rule, "❯", rule], 1)).toBeUndefined();
    expect(promptBoxRows(["❯ sem regra"], 0)).toBeUndefined();
  });

  it("is a shell's `> ` too", () => {
    expect(promptBoxRows([rule, "> um", "  dois", rule], 2)?.rows).toEqual(["um", "dois"]);
  });

  it("is emptied by selecting the whole of it, wrapped or not", () => {
    const box = ["primeira linha do pedido que", "continua aqui"];
    const n = box[0].length + box[1].length + 2;
    expect(keysEmptying("primeira linha do pedido que\ncontinua aqui", box)).toBe(R.repeat(n) + B.repeat(n));
    expect(keysEmptying("primeira linha do pedido que continua aqui", box)).toBe(R.repeat(n) + B.repeat(n));
    expect(keysEmptying("pedido que\ncontinua", box)).toBeUndefined();
    expect(keysEmptying("", box)).toBeUndefined();
  });

  it("is pieced back from its rows: a break stands for a blank, unless a word was cut in the middle", () => {
    const wrapped = logical(["alfa beta", "gama"], 20);
    expect(wrapped.text.join("")).toBe("alfa beta\ngama");
    expect(wrapped.starts).toEqual([0, 10]);
    const cut = logical(["alfabetagama", "delta"], 12);
    expect(cut.text.join("")).toBe("alfabetagamadelta");
    expect(cut.starts).toEqual([0, 12]);
  });

  it("finds a selection once, and says nothing of one it finds twice", () => {
    const twice = [rule, "❯ alfa beta alfa beta", rule];
    const box = promptBoxRows(twice, 1)?.rows ?? [];
    expect(keysAcrossRows("alfa", box, 17, 0, 20)).toBeUndefined();
  });
});

describe("the cursor's cell and its character", () => {
  it("counts an emoji as one character over two cells", () => {
    expect(charactersBeforeCell("❯ ola mundo", 6)).toBe(6);
    expect(charactersBeforeCell("❯ ola 😀 mundo", 9)).toBe(8);
    expect(charactersBeforeCell("❯ ola", 7)).toBe(7);
  });
});

describe("Claude Code's prompt box, as recorded", () => {
  const recorded = recording
    .split("\n")
    .filter((line: string) => line.trim() !== "")
    .map((line: string) => JSON.parse(line) as { lines: string[]; cursorRow: number; cursorCol: number; selected: string; keys: string });

  it("has the ten cases the Mac was tried on", () => {
    expect(recorded.length).toBe(10);
  });

  for (const [index, c] of [...Array(10).keys()].entries()) {
    it(`cut ${index + 1}: the same keys the Mac sent`, () => {
      const fixture = recorded[c];
      const keys = keysToCut(fixture.selected, fixture.lines, fixture.cursorRow, fixture.cursorCol);
      expect(keys === undefined ? "nothing" : hex(keys), fixture.selected.slice(0, 30)).toBe(fixture.keys);
    });
  }
});
