// A Ghostty theme file, read and packed.
//
// The catalogue is built from the files of Ghostty's own format (see
// tools/gerar-temas.mjs, which imports this module), and the app reads it back
// from the packed form. Nothing here touches the window or imports anything,
// so the generator can run it under Node as it is.

/** A theme's colours, each `#rrggbb`. */
export interface ThemeColors {
  background: string;
  foreground: string;
  /** Left out when the theme does not say; the terminal then draws as it would anyway. */
  cursor?: string;
  /** The text under a block cursor. */
  cursorText?: string;
  selectionBackground?: string;
  selectionForeground?: string;
  /** Slots 0 to 15, the ones a theme names. */
  ansi: readonly string[];
}

/** `#rrggbb` in lower case from `#rrggbb` or `rrggbb`; undefined for anything else. */
function hexOf(text: string): string | undefined {
  const match = /^#?([0-9a-fA-F]{6})$/.exec(text.trim());
  return match ? `#${match[1].toLowerCase()}` : undefined;
}

/** What a terminal uses for a slot a theme leaves out (the macOS app's ThemeCatalog.fallback). */
const XTERM_ANSI = [
  "#000000", "#cd3131", "#0dbc79", "#e5e510",
  "#2472c8", "#bc3fbc", "#11a8cd", "#e5e5e5",
  "#666666", "#f14c4c", "#23d18b", "#f5f543",
  "#3b8eea", "#d670d6", "#29b8db", "#ffffff",
];

/**
 * The colours a Ghostty theme file names, or undefined when it names too few
 * to be one: no background or foreground, or fewer than eight of the sixteen
 * slots (the rule the macOS app reads a theme by). A slot it leaves out takes
 * the terminal's usual colour.
 */
export function parseGhosttyTheme(text: string): ThemeColors | undefined {
  const ansi: (string | undefined)[] = new Array(16).fill(undefined);
  let background: string | undefined;
  let foreground: string | undefined;
  let cursor: string | undefined;
  let cursorText: string | undefined;
  let selectionBackground: string | undefined;
  let selectionForeground: string | undefined;
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (trimmed.startsWith("#")) continue;
    const equals = trimmed.indexOf("=");
    if (equals < 0) continue;
    const key = trimmed.slice(0, equals).trim();
    const value = trimmed.slice(equals + 1).trim().replace(/^["']+|["']+$/g, "");
    switch (key) {
      case "background":
        background = hexOf(value);
        break;
      case "foreground":
        foreground = hexOf(value);
        break;
      case "cursor-color":
        cursor = hexOf(value);
        break;
      case "cursor-text":
        cursorText = hexOf(value);
        break;
      case "selection-background":
        selectionBackground = hexOf(value);
        break;
      case "selection-foreground":
        selectionForeground = hexOf(value);
        break;
      case "palette": {
        // "palette = 4=#89b4fa": the slot, then the colour.
        const split = value.indexOf("=");
        if (split < 0) break;
        const slot = Number(value.slice(0, split).trim());
        const parsed = hexOf(value.slice(split + 1));
        if (Number.isInteger(slot) && slot >= 0 && slot <= 15 && parsed) ansi[slot] = parsed;
        break;
      }
    }
  }
  if (!background || !foreground || ansi.filter(Boolean).length < 8) return undefined;
  return {
    background,
    foreground,
    cursor,
    cursorText,
    selectionBackground,
    selectionForeground,
    ansi: ansi.map((c, slot) => c ?? XTERM_ANSI[slot]),
  };
}

const UNSET = "-";

/**
 * One line of the catalogue: background, foreground, cursor, cursor text,
 * selection background and foreground, then the sixteen slots, as six hex
 * digits each, a space between and `-` for a colour the theme leaves out.
 */
export function packTheme(theme: ThemeColors): string {
  const field = (c: string | undefined) => (c ? c.replace(/^#/, "") : UNSET);
  return [
    theme.background,
    theme.foreground,
    theme.cursor,
    theme.cursorText,
    theme.selectionBackground,
    theme.selectionForeground,
    ...theme.ansi,
  ]
    .map(field)
    .join(" ");
}

/** A packed line read back; undefined when it is not one. */
export function unpackTheme(packed: string): ThemeColors | undefined {
  const fields = packed.split(" ");
  if (fields.length !== 22) return undefined;
  const colours: (string | undefined)[] = [];
  for (const field of fields) {
    if (field === UNSET) {
      colours.push(undefined);
      continue;
    }
    const colour = hexOf(field);
    if (!colour) return undefined;
    colours.push(colour);
  }
  const [background, foreground, cursor, cursorText, selectionBackground, selectionForeground, ...ansi] = colours;
  if (!background || !foreground || ansi.some((c) => c === undefined)) return undefined;
  return { background, foreground, cursor, cursorText, selectionBackground, selectionForeground, ansi: ansi as string[] };
}
