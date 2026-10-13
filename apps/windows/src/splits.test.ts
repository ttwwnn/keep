// The dividers between panes and moving about them by keyboard: what a divider
// is known by, how far it may go, which divider an arrow moves and which pane
// is beside another.
import { beforeEach, describe, expect, it } from "vitest";
import { buildTree, type PaneTree } from "./layout";
import {
  clampShare,
  dividerFor,
  firstLeaf,
  keepShares,
  paneBeside,
  setShare,
  shareAt,
  shareOf,
  shares,
  splitKey,
  stepShare,
  type Box,
} from "./splits";

const R = 1;
const D = 2;
/** 1 | (2 over 3): a pane on the left, two stacked on the right. */
const tree = buildTree(1, [
  { tab: 2, splitOf: 1, splitDir: R },
  { tab: 3, splitOf: 2, splitDir: D },
]);
const rootSplit = tree as Extract<PaneTree, { kind: "split" }>;

describe("a divider's share", () => {
  beforeEach(() => {
    shares.value = {};
  });
  it("is one half until somebody moves it", () => {
    expect(shareOf("x")).toBe(0.5);
    setShare("x", 0.7);
    expect(shareOf("x")).toBe(0.7);
  });
  it("is kept by the pair of panes at the head of its sides", () => {
    expect(firstLeaf(tree)).toBe(1);
    expect(splitKey("w", rootSplit)).toBe("w\u0000v1-2");
    expect(splitKey("w", rootSplit.second as Extract<PaneTree, { kind: "split" }>)).toBe("w\u0000h2-3");
  });
  it("survives a pane being split off inside one of its sides", () => {
    const after = buildTree(1, [
      { tab: 2, splitOf: 1, splitDir: R },
      { tab: 3, splitOf: 2, splitDir: D },
      { tab: 4, splitOf: 3, splitDir: R },
    ]) as Extract<PaneTree, { kind: "split" }>;
    expect(splitKey("w", after)).toBe(splitKey("w", rootSplit));
  });
  it("forgets the dividers nobody wants", () => {
    setShare("a\u0000v1-2", 0.3);
    setShare("b\u0000v1-2", 0.6);
    keepShares((key) => key.startsWith("a"));
    expect(Object.keys(shares.value)).toEqual(["a\u0000v1-2"]);
  });
});

describe("how far a divider may go", () => {
  it("holds a share between what each side needs", () => {
    expect(clampShare(0.02, 0.1, 0.1)).toBe(0.1);
    expect(clampShare(0.99, 0.1, 0.1)).toBe(0.9);
    expect(clampShare(0.4, 0.1, 0.1)).toBe(0.4);
  });
  it("is the middle when the two sides cannot both have their least", () => {
    expect(clampShare(0.9, 0.6, 0.6)).toBe(0.5);
  });
  it("reads a pointer as a share of the room left by the divider", () => {
    expect(shareAt(100, 201, 1)).toBeCloseTo(0.4975, 4);
    expect(shareAt(100.5, 201, 1)).toBe(0.5);
    expect(shareAt(5, 1, 1)).toBe(0.5);
  });
});

describe("the divider an arrow moves", () => {
  it("is the nearest split outward from the pane that faces the way the arrow runs", () => {
    // 3 sits in the stacked split, inside the side-by-side one.
    expect(dividerFor(tree, 3, "up")).toBe(rootSplit.second);
    expect(dividerFor(tree, 3, "down")).toBe(rootSplit.second);
    expect(dividerFor(tree, 3, "left")).toBe(rootSplit);
    expect(dividerFor(tree, 2, "right")).toBe(rootSplit);
    expect(dividerFor(tree, 1, "right")).toBe(rootSplit);
  });
  it("is none when nothing divides that way", () => {
    expect(dividerFor(tree, 1, "up")).toBeNull();
    expect(dividerFor(tree, 9, "left")).toBeNull();
    expect(dividerFor({ kind: "leaf", id: 1 }, 1, "left")).toBeNull();
  });
  it("moves the way the arrow points, whichever side the pane is on", () => {
    expect(stepShare(0.5, "right", 20, 400)).toBeCloseTo(0.55);
    expect(stepShare(0.5, "left", 20, 400)).toBeCloseTo(0.45);
    expect(stepShare(0.5, "down", 20, 200)).toBeCloseTo(0.6);
    expect(stepShare(0.5, "up", 20, 200)).toBeCloseTo(0.4);
    expect(stepShare(0.5, "up", 20, 0)).toBe(0.5);
  });
});

describe("the pane beside another", () => {
  // |  1  |  2  |
  // |     |-----|
  // |     |  3  |
  const boxes = new Map<number, Box>([
    [1, { left: 0, top: 0, right: 100, bottom: 100 }],
    [2, { left: 101, top: 0, right: 200, bottom: 50 }],
    [3, { left: 101, top: 51, right: 200, bottom: 100 }],
  ]);
  it("is the nearest that overlaps along the other axis", () => {
    expect(paneBeside(boxes, 2, "left")).toBe(1);
    expect(paneBeside(boxes, 3, "left")).toBe(1);
    expect(paneBeside(boxes, 2, "down")).toBe(3);
    expect(paneBeside(boxes, 3, "up")).toBe(2);
  });
  it("of two that qualify takes the one whose middle is nearest", () => {
    // From 1, both 2 and 3 are to the right, at the same distance: 2's middle is the nearer to 1's.
    expect(paneBeside(boxes, 1, "right")).toBe(2);
  });
  it("is none where there is nothing", () => {
    expect(paneBeside(boxes, 1, "left")).toBeNull();
    expect(paneBeside(boxes, 1, "up")).toBeNull();
    expect(paneBeside(boxes, 2, "right")).toBeNull();
    expect(paneBeside(boxes, 9, "right")).toBeNull();
  });
  it("does not cross to a pane that only touches at a corner", () => {
    const corner = new Map<number, Box>([
      [1, { left: 0, top: 0, right: 100, bottom: 100 }],
      [2, { left: 100, top: 100, right: 200, bottom: 200 }],
    ]);
    expect(paneBeside(corner, 1, "right")).toBeNull();
    expect(paneBeside(corner, 1, "down")).toBeNull();
  });
});
