// A terminal's text, scrollback and colours, written out and read back into
// another terminal — against xterm.js itself, which runs headless here.
import { Terminal } from "@xterm/xterm";
import { describe, expect, it } from "vitest";
import { serializeBuffer } from "./serialize";

const made = (cols: number, rows: number) => new Terminal({ cols, rows, scrollback: 1000, allowProposedApi: true });
const written = (term: Terminal, data: string) => new Promise<void>((resolve) => term.write(data, resolve));

/** Every line of the normal buffer, as text and whether it wraps on from the one before. */
function lines(term: Terminal) {
  const buffer = term.buffer.normal;
  return Array.from({ length: buffer.length }, (_, y) => {
    const line = buffer.getLine(y)!;
    return { text: line.translateToString(true), wrapped: line.isWrapped };
  });
}

/** The look of every cell with something in it. */
function looks(term: Terminal) {
  const buffer = term.buffer.normal;
  const cell = buffer.getNullCell();
  const out: string[] = [];
  for (let y = 0; y < buffer.length; y++) {
    const line = buffer.getLine(y)!;
    for (let x = 0; x < term.cols; x++) {
      const c = line.getCell(x, cell)!;
      if (c.getChars() === "") continue;
      out.push(
        [y, x, c.getChars(), c.getFgColorMode(), c.isFgDefault() ? 0 : c.getFgColor(), c.getBgColorMode(), c.isBgDefault() ? 0 : c.getBgColor(), c.isBold(), c.isItalic(), c.isUnderline(), c.isInverse()].join(","),
      );
    }
  }
  return out;
}

async function roundTrip(feed: string, cols = 20, rows = 5, intoCols = cols, intoRows = rows) {
  const before = made(cols, rows);
  await written(before, feed);
  const text = serializeBuffer(before.buffer.normal, before.cols);
  const after = made(intoCols, intoRows);
  await written(after, text);
  return { before, after, text };
}

describe("a terminal's buffer written out and back", () => {
  const history = Array.from({ length: 30 }, (_, i) => `linha ${i + 1}`).join("\r\n");

  it("brings back the scrollback and the screen, line for line", async () => {
    const { before, after } = await roundTrip(history);
    expect(before.buffer.normal.length).toBeGreaterThan(5);
    expect(lines(after)).toEqual(lines(before));
  });

  it("brings back the colours and the styles", async () => {
    const feed = [
      "\x1b[31mvermelho\x1b[0m e \x1b[1;4mnegrito\x1b[0m",
      "\x1b[38;5;208mlaranja\x1b[0m \x1b[48;2;10;20;30;38;2;250;240;230mtruecolor\x1b[0m",
      "\x1b[7minverso\x1b[0m \x1b[3mitálico\x1b[0m \x1b[92mverde claro\x1b[0m \x1b[44mfundo\x1b[0m",
      history,
    ].join("\r\n");
    const { before, after } = await roundTrip(feed, 40, 6);
    expect(looks(after)).toEqual(looks(before));
  });

  it("keeps a line that wrapped as one that wraps, and writes it as it fits a different width", async () => {
    const long = "x".repeat(45);
    const { before, after } = await roundTrip(`${history}\r\n${long}`, 20, 5);
    expect(lines(after)).toEqual(lines(before));
    expect(lines(before).filter((l) => l.wrapped).length).toBeGreaterThan(0);
    // Into a wider terminal the same text joins up again.
    const { after: wide } = await roundTrip(`${history}\r\n${long}`, 20, 5, 50, 5);
    const joined = lines(wide).map((l) => l.text).join("|");
    expect(joined).toContain(long);
  });

  it("brings back wide characters and accents whole", async () => {
    const { before, after } = await roundTrip("日本語 ação ✳ coração\r\nfim", 30, 4);
    expect(lines(after)).toEqual(lines(before));
  });

  it("keeps a coloured blank, which paints, and drops a plain one", async () => {
    const { after, text } = await roundTrip("a\x1b[41m     \x1b[0mb\r\nc   ", 20, 3);
    expect(lines(after)[0].text).toBe("a     b");
    expect(text.endsWith(" ")).toBe(false);
  });

  it("is an empty screen for a terminal with nothing on it", async () => {
    const { before, after } = await roundTrip("", 20, 4);
    expect(lines(after)).toEqual(lines(before));
  });

  it("leaves the screen for the repaint to clear and the lines above it where they are", async () => {
    const { after } = await roundTrip(history, 20, 5);
    const before = lines(after);
    await written(after, "\x1b[H\x1b[2J" + "tela nova");
    const now = lines(after);
    // The five lines of the old screen are the viewport; clearing it leaves everything above.
    expect(now.slice(0, before.length - 5)).toEqual(before.slice(0, before.length - 5));
    expect(now[before.length - 5].text).toBe("tela nova");
  });
});
