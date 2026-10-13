// Dragging the divider between two panes. The share it gives is the window's
// own (splits.ts); the terminals refit by themselves as their boxes change.

import { trackDrag } from "../drag";
import { leastShare } from "../paneActions";
import { HALF, clampShare, setShare, shareAt } from "../splits";

/**
 * Follow a press on a divider: it goes where the pointer goes, and never so
 * far that a side is left with less than a few columns or rows. A double-click
 * puts it back in the middle.
 */
export function startDividerDrag(event: PointerEvent, key: string, vertical: boolean): void {
  const split = (event.currentTarget as HTMLElement).parentElement;
  if (!split) return;
  const cursor = vertical ? "resizing-cols" : "resizing-rows";
  trackDrag(event, {
    threshold: 1,
    start() {
      document.body.classList.add(cursor);
    },
    move(at) {
      const box = split.getBoundingClientRect();
      const length = vertical ? box.width : box.height;
      const least = leastShare(vertical, length);
      setShare(key, clampShare(shareAt(vertical ? at.x - box.left : at.y - box.top, length), least, least));
    },
    end() {
      document.body.classList.remove(cursor);
    },
  });
}

export const centreDivider = (key: string) => setShare(key, HALF);
