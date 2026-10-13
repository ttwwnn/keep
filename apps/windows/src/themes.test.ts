// The themes: a Ghostty file read, the catalogue built from them, and the
// palette the window and the terminal wear for each.
import { beforeAll, describe, expect, it } from "vitest";
import { contrast, isDark, parseHex } from "./color";
import { paletteOf, resolvePalette } from "./theme";
import { isSingleTheme, listThemes, loadThemes, themeColors, themeFor } from "./themes";
import { packTheme, parseGhosttyTheme, unpackTheme } from "./themes/ghostty";

const MOCHA = `
# a comment, which no Ghostty theme has but a person might add
palette = 0=#45475a
palette = 1=#f38ba8
palette = 2=#a6e3a1
palette = 3=#f9e2af
palette = 4=#89b4fa
palette = 5=#f5c2e7
palette = 6=#94e2d5
palette = 7=#bac2de
palette = 8=#585b70
palette = 9=#f7aec2
palette = 10=#c2ecbf
palette = 11=#fcd682
palette = 12=#aeccfc
palette = 13=#f398da
palette = 14=#b1eae1
palette = 15=#a6adc8
background = #1e1e2e
foreground = #cdd6f4
cursor-color = #f5e0dc
cursor-text = #1e1e2e
selection-background = #f5e0dc
selection-foreground = #1e1e2e
`;

describe("a Ghostty theme file", () => {
  it("is read for its colours", () => {
    const theme = parseGhosttyTheme(MOCHA)!;
    expect(theme.background).toBe("#1e1e2e");
    expect(theme.foreground).toBe("#cdd6f4");
    expect(theme.cursor).toBe("#f5e0dc");
    expect(theme.cursorText).toBe("#1e1e2e");
    expect(theme.selectionBackground).toBe("#f5e0dc");
    expect(theme.selectionForeground).toBe("#1e1e2e");
    expect(theme.ansi).toHaveLength(16);
    expect(theme.ansi[1]).toBe("#f38ba8");
    expect(theme.ansi[15]).toBe("#a6adc8");
  });
  it("is read as Ghostty spells it: spaces optional, quotes and bare hex allowed", () => {
    const text = MOCHA.replace("background = #1e1e2e", 'background="1E1E2E"').replace("palette = 4=#89b4fa", "palette=4 = 89b4fa");
    const theme = parseGhosttyTheme(text)!;
    expect(theme.background).toBe("#1e1e2e");
    expect(theme.ansi[4]).toBe("#89b4fa");
  });
  it("leaves what it does not say unsaid", () => {
    const theme = parseGhosttyTheme(MOCHA.split("\n").filter((l) => !/^(cursor|selection)/.test(l)).join("\n"))!;
    expect(theme.cursor).toBeUndefined();
    expect(theme.cursorText).toBeUndefined();
    expect(theme.selectionBackground).toBeUndefined();
    expect(theme.selectionForeground).toBeUndefined();
  });
  it("gives a slot it leaves out the terminal's usual colour, if it names at least eight", () => {
    const text = MOCHA.split("\n").filter((l) => !/^palette = (9|1[0-5])=/.test(l)).join("\n");
    const theme = parseGhosttyTheme(text)!;
    expect(theme.ansi[8]).toBe("#585b70");
    expect(theme.ansi[9]).toBe("#f14c4c");
    expect(theme.ansi[15]).toBe("#ffffff");
  });
  it("is not a theme with fewer than eight slots, or without its ground and its letter", () => {
    expect(parseGhosttyTheme(MOCHA.split("\n").filter((l) => !/^palette = ([0-9])=/.test(l) || l.startsWith("palette = 0")).join("\n"))).toBeUndefined();
    expect(parseGhosttyTheme(MOCHA.replace(/^background.*$/m, ""))).toBeUndefined();
    expect(parseGhosttyTheme(MOCHA.replace(/^foreground.*$/m, ""))).toBeUndefined();
    expect(parseGhosttyTheme("")).toBeUndefined();
  });
  it("takes a slot only as a plain number", () => {
    const text = MOCHA.replace("palette = 0=#45475a", "palette = =#123456\npalette = 1e1=#654321\npalette = 0x0=#abcdef");
    expect(parseGhosttyTheme(text)!.ansi[0]).toBe("#000000");
  });
  it("ignores a slot out of range, and takes a colour it cannot read as one left out", () => {
    const theme = parseGhosttyTheme(MOCHA + "palette = 16=#ffffff\npalette = x=#ffffff\ncursor-color = cell-foreground\n")!;
    expect(theme.ansi).toHaveLength(16);
    expect(theme.ansi[15]).toBe("#a6adc8");
    // The later line wins, as in Ghostty's config, and says the cursor takes the text's colour.
    expect(theme.cursor).toBeUndefined();
  });
});

describe("the catalogue's packed line", () => {
  it("reads back what it was made of", () => {
    const theme = parseGhosttyTheme(MOCHA)!;
    const packed = packTheme(theme);
    expect(packed.split(" ")).toHaveLength(22);
    expect(unpackTheme(packed)).toEqual(theme);
  });
  it("keeps a colour left out as left out", () => {
    const theme = parseGhosttyTheme(MOCHA.split("\n").filter((l) => !l.startsWith("cursor-color")).join("\n"))!;
    const packed = packTheme(theme);
    expect(packed.split(" ")[2]).toBe("-");
    expect(unpackTheme(packed)?.cursor).toBeUndefined();
  });
  it("is not read from a line that is not one", () => {
    expect(unpackTheme("")).toBeUndefined();
    expect(unpackTheme("1e1e2e cdd6f4")).toBeUndefined();
    expect(unpackTheme(packTheme(parseGhosttyTheme(MOCHA)!).replace("1e1e2e", "zzzzzz"))).toBeUndefined();
    // A ground or a slot cannot be left out.
    expect(unpackTheme(packTheme(parseGhosttyTheme(MOCHA)!).replace("1e1e2e", "-"))).toBeUndefined();
  });
});

describe("the catalogue", () => {
  beforeAll(async () => {
    await loadThemes();
  });

  it("holds the themes Ghostty ships, and then some", () => {
    expect(listThemes().length).toBeGreaterThan(450);
  });
  it("lists each name once, sorted without regard to case", () => {
    const names = listThemes();
    expect(new Set(names).size).toBe(names.length);
    const collator = new Intl.Collator("en", { sensitivity: "base" });
    for (let i = 1; i < names.length; i++) expect(collator.compare(names[i - 1], names[i])).toBeLessThanOrEqual(0);
  });
  it("has the ones people ask for, with their colours", () => {
    const mocha = themeColors("Catppuccin Mocha")!;
    expect(mocha.background).toBe("#1e1e2e");
    expect(mocha.ansi[4]).toBe("#89b4fa");
    expect(themeColors("Dracula")?.foreground).toBe("#f8f8f2");
    expect(themeColors("Catppuccin Latte")?.background).toBe("#eff1f5");
    expect(themeColors("A name that is nobody's")).toBeUndefined();
  });
  it("reads every theme in it", () => {
    for (const name of listThemes()) {
      const theme = themeColors(name);
      expect(theme, name).toBeDefined();
      expect(theme!.ansi, name).toHaveLength(16);
      expect(parseHex(theme!.background), name).toBeDefined();
    }
  });
});

describe("a choice of theme", () => {
  it("is a name, or a pair for the dark and the light, as Ghostty's config has it", () => {
    expect(themeFor("Dracula", true)).toBe("Dracula");
    expect(themeFor("Dracula", false)).toBe("Dracula");
    expect(themeFor("dark:Catppuccin Mocha,light:Catppuccin Latte", true)).toBe("Catppuccin Mocha");
    expect(themeFor("dark:Catppuccin Mocha, light:Catppuccin Latte", false)).toBe("Catppuccin Latte");
    expect(themeFor("light:Catppuccin Latte", true)).toBeUndefined();
    expect(themeFor("", true)).toBeUndefined();
    expect(themeFor("   ", false)).toBeUndefined();
  });
  it("is single unless it is a pair", () => {
    expect(isSingleTheme("Dracula")).toBe(true);
    expect(isSingleTheme("dark:A,light:B")).toBe(false);
    expect(isSingleTheme("")).toBe(false);
  });
});

describe("the palette of a theme", () => {
  beforeAll(async () => {
    await loadThemes();
  });

  it("keeps Keep's own as they were drawn before the sidebar was derived", () => {
    const escuro = resolvePalette("dark");
    expect(escuro.dark).toBe(true);
    expect(escuro.ground).toBe("#282c34");
    expect(escuro.recessed).toBe("#1d2025");
    expect(escuro.overlay).toBe("rgba(31, 34, 41, 0.86)");
    const claro = resolvePalette("light");
    expect(claro.dark).toBe(false);
    expect(claro.ground).toBe("#ffffff");
    expect(claro.recessed).toBe("#f4f4f4");
    expect(claro.overlay).toBe("rgba(240, 240, 240, 0.9)");
  });
  it("makes the ground the theme's background, a step further back for the sidebar", () => {
    const mocha = resolvePalette("light", "Catppuccin Mocha");
    expect(mocha.dark).toBe(true);
    expect(mocha.ground).toBe("#1e1e2e");
    expect(mocha.terminal.background).toBe("#1e1e2e");
    expect(mocha.recessed).toBe("#161621");
    expect(mocha.overlay).toBe("rgba(23, 23, 36, 0.86)");
    const latte = resolvePalette("dark", "Catppuccin Latte");
    expect(latte.dark).toBe(false);
    expect(latte.ground).toBe("#eff1f5");
    expect(latte.recessed).toBe("#e4e6ea");
    expect(latte.overlay).toBe("rgba(225, 227, 230, 0.9)");
  });
  it("sets the terminal in the theme's colours", () => {
    const { terminal } = resolvePalette("dark", "Catppuccin Mocha");
    expect(terminal.foreground).toBe("#cdd6f4");
    expect(terminal.cursor).toBe("#f5e0dc");
    expect(terminal.cursorAccent).toBe("#1e1e2e");
    expect(terminal.selectionBackground).toBe("#f5e0dc");
    expect(terminal.selectionForeground).toBe("#1e1e2e");
    expect(terminal.black).toBe("#45475a");
    expect(terminal.red).toBe("#f38ba8");
    expect(terminal.brightWhite).toBe("#a6adc8");
  });
  it("swaps the cells for a selection the theme does not name, as Ghostty does", () => {
    const palette = paletteOf({
      ...themeColors("Dracula")!,
      selectionBackground: undefined,
      selectionForeground: undefined,
      cursor: undefined,
      cursorText: undefined,
    });
    expect(palette.terminal.selectionBackground).toBe("#f8f8f2");
    expect(palette.terminal.selectionForeground).toBe("#282a36");
    expect(palette.terminal.cursor).toBe("#f8f8f2");
    expect(palette.terminal.cursorAccent).toBe("#282a36");
  });
  it("leaves the letters alone when the theme names only the selection's ground", () => {
    const palette = paletteOf({ ...themeColors("Dracula")!, selectionForeground: undefined });
    expect(palette.terminal.selectionBackground).toBe("#44475a");
    expect(palette.terminal.selectionForeground).toBeUndefined();
  });
  it("follows the appearance only for a pair, and wears Keep's own where the theme is unknown", () => {
    expect(resolvePalette("dark", "dark:Catppuccin Mocha,light:Catppuccin Latte").ground).toBe("#1e1e2e");
    expect(resolvePalette("light", "dark:Catppuccin Mocha,light:Catppuccin Latte").ground).toBe("#eff1f5");
    expect(resolvePalette("dark", "Dracula").ground).toBe("#282a36");
    expect(resolvePalette("light", "Dracula").ground).toBe("#282a36");
    expect(resolvePalette("dark", "No such theme").ground).toBe("#282c34");
    expect(resolvePalette("light", "No such theme").ground).toBe("#ffffff");
  });
  it("is the same object for the same theme, so nothing is redone for nothing", () => {
    expect(resolvePalette("dark", "Dracula")).toBe(resolvePalette("light", "Dracula"));
    expect(resolvePalette("dark")).toBe(resolvePalette("dark", ""));
  });

  it("decides dark or light by the ground, for every theme", () => {
    for (const name of listThemes()) {
      const palette = paletteOf(themeColors(name)!);
      expect(palette.dark, name).toBe(isDark(parseHex(palette.ground)!));
      expect(palette.ground, name).toBe(themeColors(name)!.background);
    }
  });
  it("puts the sidebar a step behind the ground, in every theme", () => {
    for (const name of listThemes()) {
      const palette = paletteOf(themeColors(name)!);
      const ground = parseHex(palette.ground)!;
      const recessed = parseHex(palette.recessed)!;
      for (let channel = 0; channel < 3; channel++) expect(recessed[channel], name).toBeLessThanOrEqual(ground[channel]);
    }
  });
  it("marks a search so the text still reads, in every theme", () => {
    const below: string[] = [];
    for (const name of listThemes()) {
      const { terminal, search } = paletteOf(themeColors(name)!);
      const ink = parseHex(terminal.foreground!)!;
      const ground = parseHex(terminal.background!)!;
      for (const field of [search.matchBackground, search.activeMatchBackground]) {
        expect(field, name).toMatch(/^#[0-9a-f]{6}$/);
      }
      // Where the theme's own letter reads well on its ground the match keeps it readable; where the theme is dim to begin with a highlight can only take a little of it.
      const own = contrast(ink, ground);
      const wanted = own >= 6 ? 4.5 : own * 0.75;
      if (contrast(ink, parseHex(search.matchBackground!)!) < wanted) below.push(name);
    }
    expect(below).toEqual([]);
  });
  it("marks a search in pale yellow on a light ground and dark amber on a dark one", () => {
    const light = resolvePalette("light").search;
    const dark = resolvePalette("dark").search;
    expect(isDark(parseHex(dark.matchBackground!)!)).toBe(true);
    expect(isDark(parseHex(light.matchBackground!)!)).toBe(false);
    expect(dark.matchBackground).not.toBe("#5a4a1a");
  });
});
