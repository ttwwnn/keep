// The translucent ground, worn by a page: the class and the variables the surfaces paint with (styles-glass.css),
// and the material the system puts behind the window (src-tauri/src/glass.rs). What is chosen is glass.ts's; the
// state it lives in is the store's.

import { effect } from "@preact/signals";
import * as api from "./api";
import { effectiveBlur, glassVars, type GlassBlur } from "./glass";
import { glassAlpha, gui, info, palette } from "./store";
import type { Palette } from "./theme";

/** Variables put on the document, to be taken off when the ground is whole again. */
const VARS = ["--ground-glass", "--recessed-glass"];

/** The document wears the translucent ground at an alpha, or none of it at a whole one. */
export function wearGlass(colors: Pick<Palette, "ground" | "recessed">, alpha: number): void {
  const root = document.documentElement;
  const on = alpha < 1;
  root.classList.toggle("glass", on);
  const vars = on ? glassVars(colors.ground, colors.recessed, alpha) : {};
  for (const name of VARS) {
    if (name in vars) root.style.setProperty(name, vars[name]);
    else root.style.removeProperty(name);
  }
}

/** What was last sent to the app, so that a change of anything else is not told again. */
let told: { blur: GlassBlur; dark: boolean } | null = null;

/** Wear the translucent ground from here on, and keep the app told of the material behind it. */
export function startGlass(): void {
  effect(() => wearGlass(palette.value, glassAlpha.value));
  effect(() => {
    // Nothing to tell a window that was not made transparent: it has nothing for a material to show through.
    if (info.value?.windowTransparent !== true) return;
    const blur = effectiveBlur(gui.value.glass, info.value.platform, info.value.windowsBuild);
    const dark = palette.value.dark;
    if (told?.blur === blur && told.dark === dark) return;
    told = { blur, dark };
    void api.glassApply(blur, dark).catch(() => {});
  });
}
