// The themes the terminal can wear: Ghostty's, from mbadolato/iTerm2-Color-Schemes
// (tools/gerar-temas.mjs builds the catalogue). Read on demand, so the window
// does not carry eight hundred palettes into its first paint; one that chose
// a theme loads them before it draws (store.start).
//
// The choice itself is kept in the state (`gui.theme`) and worn by
// `store.applyTheme`; what is here is what a name stands for.

import { signal } from "@preact/signals";
import { unpackTheme, type ThemeColors } from "./themes/ghostty";

export type { ThemeColors } from "./themes/ghostty";

interface Catalog {
  /** Every name, in the order a list of names is read in. */
  names: readonly string[];
  packed: ReadonlyMap<string, string>;
}

/** The catalogue once it is read; a computed that reads a theme follows it in. */
const catalog = signal<Catalog | null>(null);
let loading: Promise<void> | null = null;

/** Read the catalogue, once. */
export function loadThemes(): Promise<void> {
  loading ??= import("./themes/catalog.json")
    .then((module) => {
      const entries = module.default.themes as [string, string][];
      catalog.value = { names: entries.map(([name]) => name), packed: new Map(entries) };
    })
    .catch((error) => {
      loading = null;
      throw error;
    });
  return loading;
}

/** Every theme's name, sorted as the macOS list is; empty until the catalogue is read. */
export function listThemes(): readonly string[] {
  return catalog.value?.names ?? [];
}

const unpacked = new Map<string, ThemeColors | undefined>();

/** A theme's colours by name; undefined for a name the catalogue does not have (or before it is read). */
export function themeColors(name: string): ThemeColors | undefined {
  const packed = catalog.value?.packed.get(name);
  if (packed === undefined) return undefined;
  if (!unpacked.has(name)) unpacked.set(name, unpackTheme(packed));
  return unpacked.get(name);
}

/**
 * The theme a choice stands for when the scheme in force is dark or light. A
 * plain name is itself; `dark:Catppuccin Mocha,light:Catppuccin Latte` is one
 * half or the other, as Ghostty's own config reads it (the macOS app takes
 * the same lines from there). Empty is Keep's own.
 */
export function themeFor(choice: string, dark: boolean): string | undefined {
  const text = choice.trim();
  if (!text) return undefined;
  if (!text.includes(":")) return text;
  const prefix = dark ? "dark:" : "light:";
  const half = text
    .split(",")
    .map((part) => part.trim())
    .find((part) => part.startsWith(prefix));
  return half?.slice(prefix.length).trim() || undefined;
}

/** Whether a choice is a single theme, which the scheme in force cannot change. */
export function isSingleTheme(choice: string): boolean {
  return choice.trim() !== "" && !choice.includes(":");
}

/** The theme being looked at in a list, before it is chosen; null when none is. */
export const themePreview = signal<string | null>(null);
