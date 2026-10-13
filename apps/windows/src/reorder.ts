// The arithmetic of moving an item along a row or a column while the others
// open a place for it: which place the pointer is over, and how far each item
// has to slide for the gap to be where the item will land.
//
// Everything is measured along one axis from the items' places when the move
// began. Those places do not change while it lasts — the items slide by
// transforms —, so the answer for a given pointer never flickers between two.

/** One item's extent along the axis, from its leading edge. */
export interface Span {
  start: number;
  size: number;
}

/**
 * The item whose place holds `point`. A point before the first item is the
 * first, one past the last the last, and one in a gap between two belongs to
 * the item before it.
 */
export function slotAt(spans: readonly Span[], point: number): number {
  if (spans.length === 0) return -1;
  let found = 0;
  for (let i = 0; i < spans.length; i++) {
    if (point >= spans[i].start) found = i;
    else break;
  }
  return found;
}

/**
 * How far each item (by its first place) slides along the axis once `from`
 * has been moved to place `to`: the ones it passes give way by its own size,
 * and it goes by the sizes of the ones it passed. Sizes include the gap that
 * follows an item, so the places stay put whatever they are.
 */
export function slides(sizes: readonly number[], from: number, to: number): number[] {
  const out = sizes.map(() => 0);
  if (from === to || from < 0 || to < 0 || from >= sizes.length || to >= sizes.length) return out;
  if (from < to) {
    for (let i = from + 1; i <= to; i++) {
      out[i] = -sizes[from];
      out[from] += sizes[i];
    }
  } else {
    for (let i = to; i < from; i++) {
      out[i] = sizes[from];
      out[from] -= sizes[i];
    }
  }
  return out;
}

/** The distance from each item's leading edge to the next one's: its size with the gap after it. */
export function pitches(spans: readonly Span[]): number[] {
  return spans.map((span, i) => {
    const next = spans[i + 1];
    if (next) return next.start - span.start;
    // The last has no neighbour to measure to: it is as far as the one before.
    return i > 0 ? spans[i].start - spans[i - 1].start : span.size;
  });
}
