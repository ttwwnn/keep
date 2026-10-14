// The shortcut the drop-down terminal is on, as the palette and the settings
// show it. A leaf, so that the list of commands (commands.ts) can read it
// without the window's state coming with it.

import { signal } from "@preact/signals";
import { DEFAULT_SHORTCUT, shortcutKeys } from "./shortcut";

/** The shortcut asked for; the default until the app has said otherwise. */
export const quickShortcut = signal(DEFAULT_SHORTCUT);

/**
 * Whether the keys are drawn as the Mac draws them, where the app is only developed. The palette draws a chord
 * of the window's own with ⌘ for Ctrl (palette/caps.ts), which this one is not: it is a real Ctrl, so the
 * keys come out already as they are to be drawn.
 */
export const quickAsMac = signal(false);

/** The caps of the palette's row, which follow the shortcut when it is changed. */
export const quickChord = (): readonly string[] => shortcutKeys(quickShortcut.value, quickAsMac.value);
