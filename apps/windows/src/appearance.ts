// The choices of look the settings and the lists of themes and faces make,
// and the window's title. The signals they change are the store's (the state
// is saved from there); what is looked at in a list, before it is chosen, is
// kept apart (themes.ts, fonts.ts) so that backing out of the list is
// putting nothing back.

import { batch, effect } from "@preact/signals";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { fontPreview } from "./fonts";
import { normalizeScrollback, type CursorShape } from "./state";
import { activeTab, activeWorkspace, gui, setFont, tabLabel, tabsByWorkspace, update, workspaceLabel } from "./store";
import { themePreview } from "./themes";
import { windowTitle } from "./windowTitle";

/** Wear a theme of the catalogue (or a `dark:A,light:B` pair of them), whatever was being looked at. */
export function applyTheme(name: string): void {
  batch(() => {
    update((state) => void (state.theme = name.trim()));
    themePreview.value = null;
  });
}

/** Keep's own theme again, dark or light by the appearance (the macOS app's "use the config's own theme"). */
export function clearTheme(): void {
  applyTheme("");
}

/** Wear a theme for as long as it is looked at; null stops looking, and the chosen one is back. */
export function previewTheme(name: string | null): void {
  themePreview.value = name;
}

/** Set the terminals in a face, keeping the size. Empty is the platform's own. */
export function applyFont(family: string): void {
  batch(() => {
    setFont(family, gui.value.font.size);
    fontPreview.value = null;
  });
}

/** Set the terminals in a face for as long as it is looked at; null stops looking. */
export function previewFont(family: string | null): void {
  fontPreview.value = family;
}

/**
 * The text as it was at the start: the platform's face at the base size, with
 * no zoom. The macOS app's "reset the text size" puts the face back as well,
 * and its zoom is the size, so one gesture undoes both.
 */
export function resetText(): void {
  batch(() => {
    update((state) => {
      state.zoom = 1;
      state.font = { family: "", size: 0 };
    });
    fontPreview.value = null;
  });
}

export function setCursorShape(shape: CursorShape): void {
  update((state) => void (state.terminal.cursor = shape));
}

export function setCursorBlink(blink: boolean): void {
  update((state) => void (state.terminal.blink = blink));
}

export function setScrollback(lines: number): void {
  update((state) => void (state.terminal.scrollback = normalizeScrollback(lines)));
}

/** Keep the window's title the tab in front's, from here on. */
export function startAppearance(): void {
  const win = getCurrentWindow();
  let shown: string | null = null;
  effect(() => {
    const workspace = activeWorkspace.value;
    const tab = activeTab.value;
    const index = workspace && tab ? (tabsByWorkspace.value.get(workspace) ?? []).indexOf(tab) : -1;
    const title = windowTitle(workspace ? workspaceLabel(workspace) : null, workspace && tab ? tabLabel(workspace, tab, Math.max(0, index)) : null);
    // The tabs' names are in the state, which changes for many other reasons.
    if (title === shown) return;
    shown = title;
    void win.setTitle(title).catch(() => {});
  });
}
