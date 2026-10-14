// Grapheme clusters while a program has mode 2027 on, against xterm.js's own
// terminal: the parser and the width rules run without a page until the
// terminal is opened.
import xterm from "@xterm/xterm";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { describe, expect, it } from "vitest";
import { CLUSTERED, PLAIN, installGraphemes, modeReport } from "./graphemes";

const { Terminal } = xterm as unknown as typeof import("@xterm/xterm");
type Term = InstanceType<typeof Terminal>;

const wrote = (term: Term, data: string) => new Promise<void>((resolve) => term.write(data, resolve));

function rig() {
  const term = new Terminal({ allowProposedApi: true, cols: 40, rows: 5 });
  term.loadAddon(new Unicode11Addon());
  const heard: string[] = [];
  const graphemes = installGraphemes(term, (data) => heard.push(data));
  return { term, heard, graphemes };
}

/** A family of three joined with zero-width joiners, a thumbs-up with a skin tone, a flag, a letter with a mark. */
const FAMILY = "\u{1F468}‍\u{1F469}‍\u{1F467}";
const THUMB = "\u{1F44D}\u{1F3FD}";
const FLAG = "\u{1F1E7}\u{1F1F7}";
const ACCENT = "é";

describe("the mode being switched on and off", () => {
  it("starts with the rule that has no clusters, as the daemon's grid does", () => {
    const { term, graphemes } = rig();
    expect(term.unicode.activeVersion).toBe(PLAIN);
    expect(graphemes.on).toBe(false);
  });

  it("clusters from the moment the program turns it on, and no more once it turns it off", async () => {
    const { term, graphemes } = rig();
    await wrote(term, "\x1b[?2027h");
    expect(term.unicode.activeVersion).toBe(CLUSTERED);
    expect(graphemes.on).toBe(true);
    await wrote(term, "\x1b[?2027l");
    expect(term.unicode.activeVersion).toBe(PLAIN);
    expect(graphemes.on).toBe(false);
  });

  it("finds the mode among others in the sequence, and the terminal still takes the others", async () => {
    const { term } = rig();
    await wrote(term, "\x1b[?2004;2027h");
    expect(term.unicode.activeVersion).toBe(CLUSTERED);
    expect(term.modes.bracketedPasteMode).toBe(true);
    await wrote(term, "\x1b[?2004;2027l");
    expect(term.unicode.activeVersion).toBe(PLAIN);
    expect(term.modes.bracketedPasteMode).toBe(false);
  });

  it("is not moved by other modes, nor by the same numbers without the question mark", async () => {
    const { term } = rig();
    await wrote(term, "\x1b[?1049h\x1b[?25l\x1b[2027h");
    expect(term.unicode.activeVersion).toBe(PLAIN);
  });

  it("goes back to no clusters when the terminal is reset", async () => {
    const { term, graphemes } = rig();
    await wrote(term, "\x1b[?2027h\x1bc");
    expect(term.unicode.activeVersion).toBe(PLAIN);
    expect(graphemes.on).toBe(false);
  });
});

describe("the question about the mode", () => {
  it("is answered 2 while it is off and 1 while it is on", async () => {
    const { term, heard } = rig();
    await wrote(term, "\x1b[?2027$p");
    await wrote(term, "\x1b[?2027h\x1b[?2027$p");
    await wrote(term, "\x1b[?2027l\x1b[?2027$p");
    expect(heard).toEqual(["\x1b[?2027;2$y", "\x1b[?2027;1$y", "\x1b[?2027;2$y"]);
    expect(modeReport(true)).toBe("\x1b[?2027;1$y");
    expect(modeReport(false)).toBe("\x1b[?2027;2$y");
  });

  it("leaves the questions about other modes to the terminal", async () => {
    const { term, heard } = rig();
    await wrote(term, "\x1b[?2004$p");
    // xterm.js answers it itself: the mode is one it knows, and it is reset.
    expect(heard).toEqual([]);
    const own: string[] = [];
    term.onData((data) => own.push(data));
    await wrote(term, "\x1b[?2004$p");
    expect(own).toEqual(["\x1b[?2004;2$y"]);
  });
});

describe("the width of clusters", () => {
  /** The column the cursor is in after writing the text on a fresh line. */
  async function cursorAfter(text: string, on: boolean): Promise<{ x: number; line: string }> {
    const { term } = rig();
    await wrote(term, `${on ? "\x1b[?2027h" : ""}${text}`);
    return { x: term.buffer.active.cursorX, line: term.buffer.active.getLine(0)!.translateToString(true) };
  }

  it("puts a family of joined emoji in two columns while the mode is on", async () => {
    expect((await cursorAfter(FAMILY, true)).x).toBe(2);
    expect((await cursorAfter(`a${FAMILY}b`, true)).x).toBe(4);
  });

  it("puts each code point in a cell of its own while it is off, as the daemon's grid does", async () => {
    expect((await cursorAfter(FAMILY, false)).x).toBe(6);
  });

  it("clusters a skin tone, a flag and a mark", async () => {
    expect((await cursorAfter(THUMB, true)).x).toBe(2);
    expect((await cursorAfter(FLAG, true)).x).toBe(2);
    expect((await cursorAfter(ACCENT, true)).x).toBe(1);
    // Without the mode a skin tone is a wide cell of its own, and a flag is two narrow ones (Unicode 11 gives a regional indicator one column); a mark has no cell.
    expect((await cursorAfter(THUMB, false)).x).toBe(4);
    expect((await cursorAfter(FLAG, false)).x).toBe(2);
    expect((await cursorAfter(ACCENT, false)).x).toBe(1);
  });

  it("agrees with the daemon's grid on a heart with its selector, as the mode decides", async () => {
    // crates/keep-vt/src/handover.rs (`clustered_text_takes_the_columns_the_windows_app_expects`) measures the same text in the grid.
    expect((await cursorAfter("\u2764\uFE0F", true)).x).toBe(2);
    expect((await cursorAfter("\u2764\uFE0F", false)).x).toBe(1);
  });

  it("knows less of Unicode than the grid does, which is a known difference", async () => {
    // The grid gives 2 columns to every one of these. With the mode on xterm.js knows Unicode 13 and not 14 (U+1F979); with it off it
    // is the Unicode 11 table, which gives the flag two narrow letters and does not know the Unicode 12 and 13 emoji (U+1F90C).
    expect((await cursorAfter("\u{1F90C}", true)).x).toBe(2);
    expect((await cursorAfter("\u{1F979}", true)).x).toBe(1);
    expect((await cursorAfter("\u{1F90C}", false)).x).toBe(1);
    expect((await cursorAfter(FLAG, false)).x).toBe(2);
  });

  it("keeps a cluster whole in the line it is read back from", async () => {
    expect((await cursorAfter(`x${FAMILY}y`, true)).line).toBe(`x${FAMILY}y`);
  });

  it("measures by the mode in force when the text is written", async () => {
    const { term } = rig();
    await wrote(term, `${FAMILY}\r\n\x1b[?2027h${FAMILY}\r\n\x1b[?2027l${FAMILY}`);
    // The characters each line holds, a wide one counted once: three emoji, then the cluster, then three emoji again.
    const characters = [0, 1, 2].map((y) => {
      const line = term.buffer.active.getLine(y)!;
      let count = 0;
      for (let x = 0; x < line.length; x++) if (line.getCell(x)!.getChars() !== "") count++;
      return count;
    });
    expect(characters).toEqual([3, 1, 3]);
  });
});
