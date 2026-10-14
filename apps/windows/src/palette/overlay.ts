// Putting the card away and bringing it back, which every way of asking for
// it does the same.

import { activeView, dialog, overlay, type Overlay } from "../store";

/**
 * Put the card away and give the keyboard back to the terminal — unless
 * something has taken it by then: the question a command asked, the settings.
 */
export function closeOverlay(): void {
  overlay.value = null;
  requestAnimationFrame(() => {
    if (!dialog.value && !overlay.value) activeView()?.focus();
  });
}

/**
 * The key that summons a card is the key that puts it away: asked for while it
 * is up, it closes; asked for while another is up, it takes its place.
 */
export function toggleOverlay(which: Exclude<Overlay, null>): void {
  if (overlay.value === which) closeOverlay();
  else overlay.value = which;
}
