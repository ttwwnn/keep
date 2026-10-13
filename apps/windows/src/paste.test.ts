// When a paste is asked about, and what it loses on the way in.
import { describe, expect, it } from "vitest";
import { PREVIEW_CHARS, isSafePaste, pasteQuestion, preparePaste, preview } from "./paste";

describe("a paste that would run itself", () => {
  it("is asked about when it has line breaks and the program did not ask for bracketed paste", () => {
    expect(isSafePaste("ls\nrm -rf x", false)).toBe(false);
    expect(isSafePaste("ls\r\nrm", false)).toBe(false);
    expect(isSafePaste("ls\r", false)).toBe(false);
    expect(isSafePaste("ls -la", false)).toBe(true);
    expect(isSafePaste("", false)).toBe(true);
  });

  it("is not, when bracketed paste frames it", () => {
    expect(isSafePaste("ls\nrm -rf x", true)).toBe(true);
    expect(isSafePaste("one line", true)).toBe(true);
  });

  it("is, whatever the mode, when it carries the end of the frame", () => {
    expect(isSafePaste("a\x1b[201~rm -rf x", true)).toBe(false);
    expect(isSafePaste("a\x1b[201~rm -rf x", false)).toBe(false);
    expect(isSafePaste("a\x1b[200~b", true)).toBe(true);
  });
});

describe("the text that is pasted", () => {
  it("loses the control characters a tty acts on", () => {
    expect(preparePaste("a\x1bb\x03c\x15d\x7fe\x00f")).toBe("a b c d e f");
    expect(preparePaste("\x04\x05\x08\x0f\x11\x12\x13\x16\x17\x1a\x1c")).toBe(" ".repeat(11));
  });

  it("keeps what is text: tabs, line breaks, accents, emoji", () => {
    expect(preparePaste("a\tb\r\nc\nd áéíóú ção 🇧🇷")).toBe("a\tb\r\nc\nd áéíóú ção 🇧🇷");
  });
});

describe("the question", () => {
  it("shows the first characters, and says it cut them", () => {
    const long = "x".repeat(PREVIEW_CHARS + 50);
    expect(preview(long)).toBe("x".repeat(PREVIEW_CHARS) + "…");
    expect(preview("curto")).toBe("curto");
    const emoji = "🇧🇷".repeat(300);
    expect(Array.from(preview(emoji)).length).toBe(PREVIEW_CHARS + 1);
  });

  it("says why", () => {
    const lines = pasteQuestion("a\nb");
    expect(lines.title).toBe("Colar isto?");
    expect(lines.message).toMatch(/quebras de linha/);
    expect(lines.preview).toBe("a\nb");
    const framed = pasteQuestion("a\x1b[201~b");
    expect(framed.message).toMatch(/encerra a colagem/);
    expect([lines.yes, lines.no]).toEqual(["Colar", "Cancelar"]);
  });
});
