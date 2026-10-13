// The look the settings change: the face, what a program that asks is told of
// the ground, the window's title, and what the state keeps of it all.
import { describe, expect, it } from "vitest";
import { SYMBOLS_FAMILY, isFixedPitch, isTextFace, monospacedNames, quoteFamily, terminalFontStack } from "./fonts";
import { SchemeReporter, schemeReport } from "./schemeReport";
import { DEFAULT_SCROLLBACK, SCROLLBACK_MAX, defaultState, normalizeScrollback, normalizeState } from "./state";
import { windowTitle } from "./windowTitle";

describe("the font stack", () => {
  const symbols = `"${SYMBOLS_FAMILY}"`;

  it("is the platform's own, with the symbols font before the monospace, when no face was chosen", () => {
    expect(terminalFontStack("", "windows")).toBe(`'Cascadia Mono', 'Cascadia Code', Consolas, ${symbols}, monospace`);
    expect(terminalFontStack("   ", "macos")).toBe(`Menlo, ${symbols}, monospace`);
  });
  it("puts the face chosen first, quoted, and the platform's behind it", () => {
    expect(terminalFontStack("JetBrains Mono", "windows")).toBe(
      `"JetBrains Mono", 'Cascadia Mono', 'Cascadia Code', Consolas, ${symbols}, monospace`,
    );
  });
  it("takes a CSS list kept by the old free field as it is", () => {
    expect(terminalFontStack("'Fira Code', Consolas, monospace", "windows")).toBe(`'Fira Code', Consolas, ${symbols}, monospace`);
    expect(terminalFontStack("Hack, monospace", "windows")).toBe(`Hack, ${symbols}, monospace`);
  });
  it("adds the symbols font after a list that has no generic family", () => {
    expect(terminalFontStack('"Hack", Consolas', "windows")).toBe(`"Hack", Consolas, ${symbols}, monospace`);
  });
  it("does not add it twice", () => {
    const once = terminalFontStack("Hack", "windows");
    expect(terminalFontStack(once, "windows")).toBe(once);
  });
  it("quotes a name safely", () => {
    expect(quoteFamily("Fira Code")).toBe('"Fira Code"');
    expect(quoteFamily('A "quoted" one\\')).toBe('"A \\"quoted\\" one\\\\"');
  });
});

describe("which faces are monospaced", () => {
  it("takes equal widths for fixed-pitch", () => {
    expect(isFixedPitch([24, 24, 24, 24, 24])).toBe(true);
    // The rounding of a width at a fractional size is no difference.
    expect(isFixedPitch([24.002, 24, 23.999, 24, 24.001])).toBe(true);
  });
  it("takes a narrow i beside a wide W for proportional", () => {
    expect(isFixedPitch([9, 9.4, 11, 31, 38])).toBe(false);
  });
  it("takes nothing measured, or nothing wide, for neither", () => {
    expect(isFixedPitch([])).toBe(false);
    expect(isFixedPitch([0, 0, 0])).toBe(false);
  });
});

describe("the monospaced families of a machine", () => {
  const fixed = (...drawn: string[]) => (name: string) => drawn.includes(name);

  it("takes the first name of each font that the page draws fixed-pitch", () => {
    const fonts = [
      ["Cascadia Mono", "Cascadia Mono SemiLight"],
      ["Cascadia Mono", "Cascadia Mono Light"],
      ["JetBrainsMono Nerd Font Mono", "JetBrainsMono NFM"],
      ["Arial"],
    ];
    expect(monospacedNames(fonts, fixed("Cascadia Mono", "Cascadia Mono Light", "JetBrainsMono NFM"))).toEqual([
      "Cascadia Mono",
      "JetBrainsMono NFM",
    ]);
  });
  it("gives up the typographic name for the legacy one a page cannot draw it by, and keeps a family once", () => {
    const fonts = [["Foo Mono", "Foo Mono Light"], ["Foo Mono", "Foo Mono Medium"]];
    expect(monospacedNames(fonts, fixed("Foo Mono Light", "Foo Mono Medium"))).toEqual(["Foo Mono Light", "Foo Mono Medium"]);
  });
  it("leaves out what is not drawn fixed-pitch, and what is not for text, and sorts without regard to case", () => {
    const fonts = [["zed Mono"], ["Arial"], ["Wingdings"], ["Ábaco Mono"], ["Andale Mono"]];
    expect(monospacedNames(fonts, fixed("zed Mono", "Wingdings", "Ábaco Mono", "Andale Mono"))).toEqual([
      "Ábaco Mono",
      "Andale Mono",
      "zed Mono",
    ]);
  });
  it("is empty for a machine that lists nothing fixed-pitch", () => {
    expect(monospacedNames([], fixed())).toEqual([]);
    expect(monospacedNames([["Arial"]], fixed())).toEqual([]);
  });
});

describe("which families are for text", () => {
  it("leaves out the ones that draw symbols where the letters would be", () => {
    for (const name of ["Zapf Dingbats", "Wingdings 2", "Webdings", "Apple Braille", "Bodoni Ornaments", "Segoe UI Emoji", "Apple Symbols", "Symbol", SYMBOLS_FAMILY, "Marlett", "Segoe MDL2 Icons"]) {
      expect(isTextFace(name), name).toBe(false);
    }
  });
  it("keeps the monospaced faces people set a terminal in", () => {
    for (const name of ["Cascadia Mono", "Cascadia Code", "Consolas", "Courier New", "Menlo", "Monaco", "JetBrains Mono", "FiraCode Nerd Font Mono", "Source Code Pro", "Lucida Console", "DejaVu Sans Mono"]) {
      expect(isTextFace(name), name).toBe(true);
    }
  });
});

describe("telling a program the ground is dark or light", () => {
  // Enough of a terminal to register the handlers and to hear the reports.
  function terminal() {
    const csi = new Map<string, (params: (number | number[])[]) => boolean>();
    const esc = new Map<string, () => boolean>();
    const heard: [string, boolean | undefined][] = [];
    const term = {
      parser: {
        registerCsiHandler: (id: { prefix?: string; intermediates?: string; final: string }, callback: (params: (number | number[])[]) => boolean) => {
          csi.set(`${id.prefix ?? ""}${id.intermediates ?? ""}${id.final}`, callback);
          return { dispose() {} };
        },
        registerEscHandler: (id: { final: string }, callback: () => boolean) => {
          esc.set(id.final, callback);
          return { dispose() {} };
        },
      },
      input: (data: string, wasUserInput?: boolean) => void heard.push([data, wasUserInput]),
    };
    return { term: term as never, csi, esc, heard };
  }

  it("says CSI ? 997 ; 1 n for a dark ground and 2 for a light one", () => {
    expect(schemeReport(true)).toBe("\x1b[?997;1n");
    expect(schemeReport(false)).toBe("\x1b[?997;2n");
  });
  it("tells a program that switches the mode on, at once, and not as if the person had typed it", () => {
    const { term, csi, heard } = terminal();
    new SchemeReporter(term, true);
    expect(csi.get("?h")!([2031])).toBe(false);
    expect(heard).toEqual([["\x1b[?997;1n", false]]);
  });
  it("finds the mode among others in the same sequence, and ignores sequences that do not set it", () => {
    const { term, csi, heard } = terminal();
    new SchemeReporter(term, false);
    csi.get("?h")!([1049]);
    csi.get("?h")!([25, 12]);
    expect(heard).toEqual([]);
    csi.get("?h")!([1049, 2031]);
    expect(heard).toEqual([["\x1b[?997;2n", false]]);
  });
  it("tells it again each time the ground turns, while it asks, and not when it stays", () => {
    const { term, csi, heard } = terminal();
    const reporter = new SchemeReporter(term, true);
    reporter.setDark(false);
    expect(heard).toEqual([]);
    csi.get("?h")!([2031]);
    expect(heard).toEqual([["\x1b[?997;2n", false]]);
    reporter.setDark(false);
    reporter.setDark(true);
    reporter.setDark(false);
    expect(heard.map(([data]) => data)).toEqual(["\x1b[?997;2n", "\x1b[?997;1n", "\x1b[?997;2n"]);
  });
  it("stops when the program switches the mode off", () => {
    const { term, csi, heard } = terminal();
    const reporter = new SchemeReporter(term, true);
    csi.get("?h")!([2031]);
    csi.get("?l")!([2031]);
    heard.length = 0;
    reporter.setDark(false);
    expect(heard).toEqual([]);
  });
  it("stops when the terminal is reset, which puts the mode back as it was", () => {
    for (const reset of [(c: ReturnType<typeof terminal>) => c.esc.get("c")!(), (c: ReturnType<typeof terminal>) => c.csi.get("!p")!([])]) {
      const rig = terminal();
      const reporter = new SchemeReporter(rig.term, true);
      rig.csi.get("?h")!([2031]);
      rig.heard.length = 0;
      expect(reset(rig)).toBe(false);
      reporter.setDark(false);
      expect(rig.heard).toEqual([]);
    }
  });
  it("answers a question about the scheme whether or not the mode is on", () => {
    const { term, csi, heard } = terminal();
    new SchemeReporter(term, false);
    expect(csi.get("?n")!([996])).toBe(false);
    expect(heard).toEqual([["\x1b[?997;2n", false]]);
    csi.get("?n")!([6]);
    expect(heard).toHaveLength(1);
  });
  it("lets the terminal's own handler see every one of these sequences", () => {
    const { term, csi, esc } = terminal();
    new SchemeReporter(term, true);
    for (const handler of csi.values()) expect(handler([2031, 996])).toBe(false);
    for (const handler of esc.values()) expect(handler()).toBe(false);
  });
});

describe("the window's title", () => {
  it("is the tab in front, and the workspace it is in", () => {
    expect(windowTitle("keep", "claude")).toBe("claude — keep");
  });
  it("is the tab alone when the tab is called what the workspace is", () => {
    expect(windowTitle("keep", "keep")).toBe("keep");
  });
  it("is the workspace when it has no tab to show, and Keep when it has nothing", () => {
    expect(windowTitle("keep", null)).toBe("keep");
    expect(windowTitle(null, "zsh")).toBe("zsh");
    expect(windowTitle(null, null)).toBe("Keep");
    expect(windowTitle("  ", "  ")).toBe("Keep");
  });
  it("is cut where the system would cut it anyway", () => {
    const title = windowTitle("keep", "x".repeat(300));
    expect(title).toHaveLength(120);
    expect(title.endsWith("…")).toBe(true);
  });
});

describe("what the state keeps of the look", () => {
  it("starts as the Mac does: a bar that blinks, ten thousand lines, Keep's own theme", () => {
    const state = defaultState();
    expect(state.theme).toBe("");
    expect(state.terminal).toEqual({ cursor: "bar", blink: true, scrollback: DEFAULT_SCROLLBACK });
    expect(DEFAULT_SCROLLBACK).toBe(10000);
  });
  it("is read back from a file", () => {
    const state = normalizeState({
      theme: "  Dracula ",
      terminal: { cursor: "block", blink: false, scrollback: 2500.4 },
    });
    expect(state.theme).toBe("Dracula");
    expect(state.terminal).toEqual({ cursor: "block", blink: false, scrollback: 2500 });
    expect(normalizeState({ terminal: { cursor: "underline" } }).terminal.cursor).toBe("underline");
  });
  it("is the default for a file from before the look was kept, and for what is wrongly kept", () => {
    expect(normalizeState({ zoom: 1.1 }).theme).toBe("");
    expect(normalizeState({ zoom: 1.1 }).terminal).toEqual(defaultState().terminal);
    expect(normalizeState({ theme: 7, terminal: { cursor: "triangle", blink: "yes", scrollback: "many" } })).toMatchObject({
      theme: "",
      terminal: { cursor: "bar", blink: true, scrollback: DEFAULT_SCROLLBACK },
    });
    expect(normalizeState({ terminal: "no" }).terminal).toEqual(defaultState().terminal);
  });
  it("holds the history to what a window can carry", () => {
    expect(normalizeScrollback(-5)).toBe(0);
    expect(normalizeScrollback(0)).toBe(0);
    expect(normalizeScrollback(1e9)).toBe(SCROLLBACK_MAX);
    expect(normalizeScrollback(NaN)).toBe(DEFAULT_SCROLLBACK);
    expect(normalizeScrollback(Infinity)).toBe(DEFAULT_SCROLLBACK);
    expect(normalizeScrollback("500")).toBe(DEFAULT_SCROLLBACK);
  });
});
