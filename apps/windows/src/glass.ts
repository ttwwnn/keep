// A translucent ground: the terminal's background at a share of its opacity, with the desktop (blurred, if asked)
// showing through. The Mac gets this from Ghostty's `background-opacity` and `background-blur` in the user's
// config (GhosttyApp.terminalBackgroundOpacity, KeepWindow.applyTerminalAppearance); here it is a setting.
//
// Pure functions: what is asked for, what the system can give, and the colours that come of both. The window that
// has to be made transparent, and the effects behind it, are src-tauri/src/glass.rs's; the page that wears the
// result is glassStore.ts.

import type { ITheme } from "@xterm/xterm";
import { parseHex, rgba } from "./color";

/** What sits behind the translucent ground: nothing (the plain desktop), or one of the system's materials. */
export type GlassBlur = "none" | "mica" | "acrylic";

export interface Glass {
  /** The share of the ground that is painted: 1 is the opaque window as it always was, 0.5 the least allowed. */
  opacity: number;
  blur: GlassBlur;
}

/** Ghostty's `background-opacity` runs 0 to 1; below half the text stops being legible over a busy desktop. */
export const OPACITY_MIN = 0.5;

export const defaultGlass = (): Glass => ({ opacity: 1, blur: "none" });

/** An opacity read back from the state file: a step of one per cent between the least and 1; anything else is opaque. */
export function normalizeOpacity(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return 1;
  return Math.min(1, Math.max(OPACITY_MIN, Math.round(value * 100) / 100));
}

/** The look read back from the state file, whatever it holds: a file from before this existed is the opaque window. */
export function normalizeGlass(raw: unknown): Glass {
  const record = typeof raw === "object" && raw !== null && !Array.isArray(raw) ? (raw as Record<string, unknown>) : {};
  const blur = record.blur === "mica" || record.blur === "acrylic" ? record.blur : "none";
  return { opacity: normalizeOpacity(record.opacity), blur };
}

/** The first Windows build with the acrylic the library applies (1809: window-vibrancy asks for 17763). */
export const ACRYLIC_BUILD = 17763;
/** The first build of Windows 11 22H2, which has the system backdrops (Mica). */
export const MICA_BUILD = 22621;
/** The first build of Windows 11: before it, acrylic is the old kind and drags badly. */
export const WINDOWS_11_BUILD = 22000;

/**
 * The materials this system can put behind the window, "none" first. Only Windows has them here: the Mac's
 * translucency is Ghostty's own, in the config of the user, and not a setting of this app.
 */
export function blurChoices(platform: string, build: number): GlassBlur[] {
  if (platform !== "windows") return ["none"];
  const choices: GlassBlur[] = ["none"];
  if (build >= MICA_BUILD) choices.push("mica");
  if (build >= ACRYLIC_BUILD) choices.push("acrylic");
  return choices;
}

/** The blur that is applied: the one asked for if the system has it and the ground lets it through, else none. */
export function effectiveBlur(glass: Glass, platform: string, build: number): GlassBlur {
  if (glass.opacity >= 1) return "none";
  return blurChoices(platform, build).includes(glass.blur) ? glass.blur : "none";
}

/** Acrylic before Windows 11 stutters while the window is dragged or resized (a known fault of the system). */
export function acrylicDrags(platform: string, build: number): boolean {
  return platform === "windows" && build >= ACRYLIC_BUILD && build < WINDOWS_11_BUILD;
}

/**
 * How much of the ground is painted right now. Opaque when the window was not made transparent (the system only
 * lets it through for a window that was made so, and that is decided when it is made), and while something floats
 * over it (the macOS KeepWindow.holdOpaque: a card over a see-through window would be wearing the wallpaper).
 */
export function effectiveAlpha(glass: Glass, windowTransparent: boolean, held: boolean): number {
  return windowTransparent && !held ? glass.opacity : 1;
}

/** Whether what is asked for needs a window that was not made transparent to be made again: the app opened again. */
export function needsRestart(glass: Glass, windowTransparent: boolean): boolean {
  return glass.opacity < 1 && !windowTransparent;
}

/** The overlays that are pickers, over which the window holds itself opaque (the Mac does it for its picker). */
const OPAQUE_OVERLAYS: readonly (string | null)[] = ["picker", "palette", "themes", "fonts"];

export const holdsOpaque = (overlay: string | null): boolean => OPAQUE_OVERLAYS.includes(overlay);

/** A hex ground at an alpha, as the CSS wants it; an opaque one is left alone, and so is one that is no colour. */
export function translucent(ground: string, alpha: number): string {
  const rgb = parseHex(ground);
  return alpha >= 1 || !rgb ? ground : rgba(rgb, alpha);
}

/**
 * The terminal's theme with the ground at the alpha. The same object when there is nothing to do, so that what
 * follows a theme is not redone for nothing. Only the default background: the cells a program paints in its own
 * colours stay solid, as in Ghostty, and the text is never translucent.
 */
export function glassTheme(theme: ITheme, alpha: number): ITheme {
  if (alpha >= 1 || !theme.background || !parseHex(theme.background)) return theme;
  return { ...theme, background: translucent(theme.background, alpha) };
}

/** The variables the translucent surfaces paint with (styles-glass.css), from the palette's grounds. */
export function glassVars(ground: string, recessed: string, alpha: number): Record<string, string> {
  return { "--ground-glass": translucent(ground, alpha), "--recessed-glass": translucent(recessed, alpha) };
}
