// Moving an item along a row: which place the pointer is over, and how far the
// others slide to leave the gap where it will land.
import { describe, expect, it } from "vitest";
import { pitches, slides, slotAt, type Span } from "./reorder";

const row = (...sizes: number[]): Span[] => {
  let start = 0;
  return sizes.map((size) => {
    const span = { start, size };
    start += size;
    return span;
  });
};

describe("the place a point is over", () => {
  const spans = row(100, 100, 100);
  it("is the item whose place holds it", () => {
    expect(slotAt(spans, 0)).toBe(0);
    expect(slotAt(spans, 99)).toBe(0);
    expect(slotAt(spans, 100)).toBe(1);
    expect(slotAt(spans, 250)).toBe(2);
  });
  it("is the first before the first and the last past the last", () => {
    expect(slotAt(spans, -40)).toBe(0);
    expect(slotAt(spans, 900)).toBe(2);
  });
  it("is the item before a gap for a point in the gap", () => {
    const gappy = [
      { start: 0, size: 90 },
      { start: 100, size: 90 },
    ];
    expect(slotAt(gappy, 95)).toBe(0);
  });
  it("is nothing for no items", () => {
    expect(slotAt([], 10)).toBe(-1);
  });
});

describe("how far the others slide", () => {
  it("opens the gap ahead for an item moving up the list", () => {
    // [A B C D], D to the front: A, B, C each give way by D's size, D goes back by theirs.
    expect(slides([10, 20, 30, 40], 3, 0)).toEqual([40, 40, 40, -60]);
  });
  it("closes it behind for an item moving down the list", () => {
    expect(slides([10, 20, 30, 40], 0, 2)).toEqual([50, -10, -10, 0]);
  });
  it("moves nothing when it stays", () => {
    expect(slides([10, 20, 30], 1, 1)).toEqual([0, 0, 0]);
  });
  it("moves nothing for places that are not there", () => {
    expect(slides([10, 20, 30], 5, 1)).toEqual([0, 0, 0]);
    expect(slides([10, 20, 30], 1, -1)).toEqual([0, 0, 0]);
  });
  it("leaves the items tiling the same stretch in their new order", () => {
    const sizes = [24, 36, 24, 48, 24];
    const starts = sizes.map((_, i) => sizes.slice(0, i).reduce((a, b) => a + b, 0));
    for (const [from, to] of [
      [0, 4],
      [4, 0],
      [1, 3],
      [3, 1],
    ]) {
      const moved = slides(sizes, from, to);
      const order = sizes.map((_, i) => i).sort((a, b) => starts[a] + moved[a] - (starts[b] + moved[b]));
      const expected = sizes.map((_, i) => i).filter((i) => i !== from);
      expected.splice(to, 0, from);
      expect(order).toEqual(expected);
      let at = 0;
      for (const i of order) {
        expect(starts[i] + moved[i]).toBe(at);
        at += sizes[i];
      }
    }
  });
});

describe("the distance between places", () => {
  it("is from one leading edge to the next, the gap after an item included", () => {
    const spans = [
      { start: 0, size: 90 },
      { start: 100, size: 90 },
      { start: 200, size: 90 },
    ];
    expect(pitches(spans)).toEqual([100, 100, 100]);
  });
  it("lets the last go as far as the one before it, and a lone item as far as it is wide", () => {
    expect(pitches(row(30, 50))).toEqual([30, 30]);
    expect(pitches(row(70))).toEqual([70]);
  });
});
