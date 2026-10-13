// The faces the terminal can be set in: the monospaced families installed
// here, and the font stack a terminal is given (FontCatalog and
// GhosttyApp.terminalFont in the macOS app).

import { signal } from "@preact/signals";
import * as api from "./api";
import { defaultFontFamily } from "./theme";

/** The glyphs a terminal is judged on, in the face itself: a zero, an O, an l, a 1, an I, braces, an arrow. */
export const previewSample = "0O l1I {}[] =>";

/** The face being looked at in a list, before it is chosen; null when none is. */
export const fontPreview = signal<string | null>(null);

/**
 * The font the icons of a Nerd Font come from when the face in use has none
 * (the Powerline arrows, the git branch): embedded with the app
 * (styles-appearance.css), so a terminal does not draw them as boxes on a
 * machine without one installed.
 */
export const SYMBOLS_FAMILY = "Symbols Nerd Font Mono";

/** A family name as `font-family` takes it. */
export function quoteFamily(name: string): string {
  return `"${name.replace(/(["\\])/g, "\\$1")}"`;
}

/** The first face of a font stack, bare: what to call the platform's own. */
export function firstFamily(stack: string): string {
  return stack.split(",")[0].trim().replace(/^["']|["']$/g, "");
}

/** Whether what was kept as a family is already a CSS list, as the old free field of the settings took one. */
function isCssList(text: string): boolean {
  return /[,"']/.test(text);
}

/**
 * The `font-family` a terminal gets for the face chosen (empty: the platform's
 * own): the face, then the platform's faces behind it, then the symbols font
 * for the icons none of them has, then whatever monospace the system has.
 * A face that is not installed any more falls through to the next.
 */
export function terminalFontStack(chosen: string, platform: string): string {
  const own = chosen.trim();
  const stack = own === "" ? defaultFontFamily(platform) : isCssList(own) ? own : `${quoteFamily(own)}, ${defaultFontFamily(platform)}`;
  if (stack.includes(SYMBOLS_FAMILY)) return stack;
  const generic = /,\s*monospace\s*$/i;
  const symbols = quoteFamily(SYMBOLS_FAMILY);
  return generic.test(stack) ? stack.replace(generic, `, ${symbols}, monospace`) : `${stack}, ${symbols}, monospace`;
}

/**
 * Whether the widths of a few glyphs of very different widths in a
 * proportional face — "i", "l", ".", "m", "W" — came out the same: the face is
 * fixed-pitch. Measured in the page, because what counts is how the page draws
 * it (the macOS app asks the system's `isFixedPitch`, which a canvas cannot).
 */
export function isFixedPitch(widths: readonly number[]): boolean {
  if (widths.length === 0) return false;
  const widest = Math.max(...widths);
  return widest > 0 && widest - Math.min(...widths) < widest * 0.01;
}

const PROBE = ["i", "l", ".", "m", "W"];
let canvas: CanvasRenderingContext2D | null = null;

/**
 * The widths the page draws the probe glyphs with in `family`. Behind it
 * comes a proportional face, not a monospace one: a family the page does not
 * have then measures as proportional and drops out, instead of passing as
 * fixed-pitch in the monospace that stood in for it.
 */
function measure(family: string): number[] {
  canvas ??= document.createElement("canvas").getContext("2d");
  if (!canvas) return [];
  canvas.font = `40px ${quoteFamily(family)}, serif`;
  return PROBE.map((glyph) => canvas!.measureText(glyph).width);
}

/** Families people set a terminal in, for when the machine cannot list what it has. */
const WELL_KNOWN = [
  "Cascadia Mono", "Cascadia Code", "Consolas", "Courier New", "Lucida Console", "JetBrains Mono", "Fira Code",
  "Source Code Pro", "Hack", "DejaVu Sans Mono", "Liberation Mono", "IBM Plex Mono", "Roboto Mono", "Menlo", "Monaco",
  "SF Mono", "Andale Mono",
];

/** Fixed-pitch or not, by family, so a list opened again measures only what is new. */
const classified = new Map<string, boolean>();

const frame = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

/**
 * The monospaced families installed here, sorted as the macOS list is. The
 * machine lists its families (src-tauri fonts.rs, out of the fonts' own
 * files); the page keeps the ones it draws fixed-pitch. In batches, with the
 * window let breathe between them: there are hundreds.
 */
export async function listMonospaceFonts(): Promise<string[]> {
  const families = await api.installedFonts().catch(() => WELL_KNOWN);
  const found: string[] = [];
  let measured = 0;
  for (const family of new Set(families)) {
    let fixed = classified.get(family);
    if (fixed === undefined) {
      fixed = isFixedPitch(measure(family));
      classified.set(family, fixed);
      if (++measured % 60 === 0) await frame();
    }
    if (fixed && family !== SYMBOLS_FAMILY) found.push(family);
  }
  return found.sort((a, b) => a.localeCompare(b, "en", { sensitivity: "base" }));
}

/**
 * Ask for the symbols font before a terminal draws, which would otherwise
 * draw the icons without it: a canvas does not wait for a font it has not
 * been asked for. Not for long, and not for ever: the terminal is usable
 * without the icons.
 */
export async function loadSymbolsFont(): Promise<void> {
  try {
    await Promise.race([
      document.fonts.load(`16px ${quoteFamily(SYMBOLS_FAMILY)}`, "\ue0b0"),
      new Promise((resolve) => setTimeout(resolve, 2000)),
    ]);
  } catch {
    // Drawn without the icons.
  }
}
