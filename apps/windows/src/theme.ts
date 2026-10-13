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
    // The sidebar's inks, as the macOS app draws them (Palette in
    // WorkspaceSidebar.swift), which the footers and the menus share.
    "--ink": `rgba(${ink}, ${palette.dark ? 0.96 : 0.92})`,
    "--ink-resting": `rgba(${ink}, ${palette.dark ? 0.6 : 0.75})`,
    "--ink-faint": `rgba(${ink}, ${palette.dark ? 0.38 : 0.55})`,
    // The row at the top (TabStripView.swift): the tab's text, its dim and
    // hover inks, its fill, hover fill and edge, the glass a chosen tab
    // wears, and the ground the round controls stand on.
    "--chrome-text": `rgba(${ink}, ${palette.dark ? 0.92 : 0.85})`,
    "--chrome-dim": `rgba(${ink}, 0.45)`,
    "--chrome-hover": `rgba(${ink}, ${palette.dark ? 0.72 : 0.66})`,
    "--tab-fill": `rgba(${ink}, ${palette.dark ? 0.22 : 0.14})`,
    "--tab-hover": `rgba(${ink}, ${palette.dark ? 0.05 : 0.035})`,
    "--tab-edge": `rgba(${ink}, ${palette.dark ? 0.16 : 0.1})`,
    "--control-fill": `rgba(${ink}, ${palette.dark ? 0.09 : 0.06})`,
    "--control-hover": `rgba(${ink}, ${palette.dark ? 0.22 : 0.12})`,
    "--chrome-off": `rgba(${ink}, 0.09)`,
    "--ink-rgb": ink,
    "--edge-attention": palette.dark ? "#ffffff" : "rgb(40, 30, 20)",
    "--attention-fill": palette.dark ? "rgb(255, 120, 20)" : "rgb(255, 106, 0)",
    "--wash": `rgba(${ink}, ${palette.dark ? 0.09 : 0.07})`,
    "--wash-strong": `rgba(${ink}, ${palette.dark ? 0.14 : 0.11})`,
    "--rule": `rgba(${ink}, 0.08)`,
    "--overlay": palette.dark ? "rgba(36, 40, 47, 0.97)" : "rgba(252, 252, 252, 0.98)",
    "--shadow": "0 18px 48px rgba(0, 0, 0, 0.62)",
    // DESIGN.md's state dots, in OKLCH: attached, busy, idle.
    // A step darker on a light ground, as the macOS app lowers them.
    "--dot-attached": palette.dark ? "oklch(0.76 0.14 150)" : "oklch(0.54 0.14 150)",
    "--dot-busy": palette.dark ? "oklch(0.82 0.15 85)" : "oklch(0.60 0.15 85)",
    "--dot-idle": palette.dark ? "oklch(0.70 0.05 250)" : "oklch(0.48 0.05 250)",
    // The mark of a tab at work: Claude Code's orange, Codex's blue-green.
    "--mark-claude": palette.dark ? "oklch(0.72 0.15 48)" : "oklch(0.50 0.15 48)",
    "--mark-codex": palette.dark ? "oklch(0.78 0.11 200)" : "oklch(0.56 0.11 200)",
    "--danger": palette.dark ? "#ff6b6b" : "#c42b1c",
    // The usage footer's: amber from 75%, red from 90% (the Clínica panel's
    // thresholds), and the attached green for the account a tab is on — a
    // step darker on a light ground, as the macOS footer's are.
    "--attention": palette.dark ? "oklch(0.82 0.15 85)" : "oklch(0.60 0.15 85)",
    "--critical": palette.dark ? "oklch(0.70 0.19 27)" : "oklch(0.48 0.19 27)",
    "--live": palette.dark ? "oklch(0.76 0.14 150)" : "oklch(0.54 0.14 150)",
  };
  for (const [name, value] of Object.entries(vars)) root.style.setProperty(name, value);
  document.body.style.background = palette.ground;
}

export function defaultFontFamily(platform: string): string {
  if (platform === "windows") return "'Cascadia Mono', 'Cascadia Code', Consolas, monospace";
  if (platform === "macos") return "Menlo, monospace";
  return "'DejaVu Sans Mono', 'Liberation Mono', monospace";
}

/**
 * The terminal's text at 100%: 13 points, Ghostty's own default, a quarter
 * up — what the macOS app calls 100% (GhosttyApp.textBoost).
 */
export const DEFAULT_FONT_SIZE = 16.25;
