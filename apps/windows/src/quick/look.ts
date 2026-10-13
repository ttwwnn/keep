// What the panel wears. The main window owns the look — the theme, the face,
// the zoom, the cursor, the history — and keeps it in the state file; the
// panel reads it from there and writes nothing, so that two windows never save
// each other's stale copy. It reads again each time it comes in, which is the
// only time anyone can see it.

import { gui } from "../store";
import { normalizeState, type GuiState } from "../state";
import { loadThemes } from "../themes";
import { quickLook } from "./api";

/** The part of the state the panel wears. */
function worn(state: GuiState) {
  const { appearance, theme, font, zoom, terminal } = state;
  return { appearance, theme, font, zoom, terminal };
}

/** Whether two states dress the panel alike: what else changed (the sidebar, the tabs' names) is not its business. */
export function sameLook(a: GuiState, b: GuiState): boolean {
  return JSON.stringify(worn(a)) === JSON.stringify(worn(b));
}

/**
 * Wear the state as the disk has it. Nothing readable now leaves the panel as it is; a state that dresses it
 * the same is not taken, so that nothing downstream (the palette, the terminal's options) is redone for nothing.
 */
export async function wearTheState(): Promise<void> {
  const raw = await quickLook().catch(() => null);
  if (raw === null) return;
  const next = normalizeState(raw);
  if (sameLook(next, gui.value)) return;
  // The theme's colours come from the catalogue, which is read before they are asked for.
  if (next.theme) await loadThemes().catch(() => {});
  gui.value = next;
}
