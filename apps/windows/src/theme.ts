// Keep's two themes, as the terminal and as the chrome around it.
//
// The chrome has no colours of its own (DESIGN.md): its ground is the
// terminal's background, a step further back for the sidebar, and its inks
// are white (or black) at three strengths. Colour is left to state.

import type { ITheme } from "@xterm/xterm";

export type Appearance = "auto" | "light" | "dark";

export interface Palette {
  dark: boolean;
  /** The terminal's background, and the strip's. */
  ground: string;
  /** The sidebar: a step further back. */
  recessed: string;
  terminal: ITheme;
}

/** Keep Escuro: Ghostty's own background, letter and palette. */
const KEEP_ESCURO: Palette = {
  dark: true,
  ground: "#282c34",
  recessed: "#1d2025",
  terminal: {
    background: "#282c34",
    foreground: "#ffffff",
    cursor: "#ffffff",
    cursorAccent: "#282c34",
    selectionBackground: "rgba(255, 255, 255, 0.26)",
    black: "#1d1f21",
    red: "#cc6666",
    green: "#b5bd68",
    yellow: "#f0c674",
    blue: "#81a2be",
    magenta: "#b294bb",
    cyan: "#8abeb7",
    white: "#c5c8c6",
    brightBlack: "#666666",
    brightRed: "#d54e53",
    brightGreen: "#b9ca4a",
    brightYellow: "#e7c547",
    brightBlue: "#7aa6da",
    brightMagenta: "#c397d8",
    brightCyan: "#70c0b1",
    brightWhite: "#eaeaea",
  },
};

/**
 * Keep Claro: a white ground, as Claude Code's light theme assumes, and
 * GitHub Light's palette, made to read on white (its "yellow" is a dark
 * brown on purpose).
 */
const KEEP_CLARO: Palette = {
  dark: false,
  ground: "#ffffff",
  recessed: "#f4f4f4",
  terminal: {
    background: "#ffffff",
    foreground: "#1f2328",
    cursor: "#1f2328",
    cursorAccent: "#ffffff",
    selectionBackground: "#b4d5ff",
    selectionForeground: "#1f2328",
    black: "#24292f",
    red: "#cf222e",
    green: "#116329",
    yellow: "#4d2d00",
    blue: "#0969da",
    magenta: "#8250df",
    cyan: "#1b7c83",
    white: "#6e7781",
    brightBlack: "#57606a",
    brightRed: "#a40e26",
    brightGreen: "#1a7f37",
    brightYellow: "#633c01",
    brightBlue: "#218bff",
    brightMagenta: "#a475f9",
    brightCyan: "#3192aa",
    brightWhite: "#8c959f",
  },
};

const systemDark = () =>
  typeof window !== "undefined" && window.matchMedia?.("(prefers-color-scheme: dark)").matches === true;

export function resolvePalette(appearance: Appearance): Palette {
  const dark = appearance === "dark" || (appearance === "auto" && systemDark());
  return dark ? KEEP_ESCURO : KEEP_CLARO;
}

/** Follow the system while the appearance is automatic. */
export function onSystemAppearanceChange(callback: () => void): () => void {
  const query = window.matchMedia?.("(prefers-color-scheme: dark)");
  if (!query) return () => {};
  query.addEventListener("change", callback);
  return () => query.removeEventListener("change", callback);
}

/** The chrome's colours as CSS variables on the document. */
export function applyPalette(palette: Palette): void {
  const root = document.documentElement;
  const ink = palette.dark ? "255, 255, 255" : "0, 0, 0";
  root.dataset.theme = palette.dark ? "dark" : "light";
  root.style.colorScheme = palette.dark ? "dark" : "light";
  const vars: Record<string, string> = {
    "--ground": palette.ground,
    "--recessed": palette.recessed,
    "--ink": `rgba(${ink}, ${palette.dark ? 0.96 : 0.9})`,
    "--ink-resting": `rgba(${ink}, ${palette.dark ? 0.55 : 0.58})`,
    "--ink-faint": `rgba(${ink}, ${palette.dark ? 0.35 : 0.38})`,
    "--wash": `rgba(${ink}, ${palette.dark ? 0.09 : 0.07})`,
    "--wash-strong": `rgba(${ink}, ${palette.dark ? 0.14 : 0.11})`,
    "--rule": `rgba(${ink}, ${palette.dark ? 0.08 : 0.1})`,
    "--overlay": palette.dark ? "rgba(36, 40, 47, 0.97)" : "rgba(252, 252, 252, 0.98)",
    "--shadow": palette.dark ? "0 18px 50px rgba(0, 0, 0, 0.55)" : "0 18px 50px rgba(0, 0, 0, 0.18)",
    // DESIGN.md's state dots, in OKLCH: attached, busy, idle.
    "--dot-attached": "oklch(0.76 0.14 150)",
    "--dot-busy": "oklch(0.82 0.15 85)",
    "--dot-idle": "oklch(0.70 0.05 250)",
    "--danger": palette.dark ? "#ff6b6b" : "#c42b1c",
  };
  for (const [name, value] of Object.entries(vars)) root.style.setProperty(name, value);
  document.body.style.background = palette.ground;
}

export function defaultFontFamily(platform: string): string {
  if (platform === "windows") return "'Cascadia Mono', 'Cascadia Code', Consolas, monospace";
  if (platform === "macos") return "Menlo, monospace";
  return "'DejaVu Sans Mono', 'Liberation Mono', monospace";
}

export const DEFAULT_FONT_SIZE = 14;
