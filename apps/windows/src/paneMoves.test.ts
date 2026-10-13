// Moving a pane: the side a point aims at, the landing it paints, and the
// moves the daemon is given — checked against a stand-in for the daemon's own
// rule (Workspace::rearrange) and the tree the window then rebuilds.
import { describe, expect, it } from "vitest";
import { SPLIT_DOWN, SPLIT_RIGHT } from "./api";
import { buildTree, type PaneTree } from "./layout";
import {
  applyMoves,
  dropSide,
  landingRect,
  movesForDetach,
  movesForDrop,
  placeOf,
  vacating,
  type Move,
  type PaneRecord,
} from "./paneMoves";

const R = SPLIT_RIGHT;
const D = SPLIT_DOWN;
const pane = (id: number, splitOf = 0, splitDir = 0): PaneRecord => ({ id, splitOf, splitDir });

/** What the daemon does with a set of moves: all of them or none (Workspace::rearrange). */
function rearrange(panes: PaneRecord[], moves: Move[]): PaneRecord[] {
  const staged = panes.map((p) => ({ ...p }));
  for (const m of moves) {
    if (m.tab === m.splitOf) throw new Error("a pane cannot be a pane of itself");
    if (m.splitOf !== 0 && !staged.some((p) => p.id === m.splitOf)) throw new Error(`no tab ${m.splitOf}`);
    const at = staged.find((p) => p.id === m.tab);
    if (!at) throw new Error(`no tab ${m.tab}`);
    at.splitOf = m.splitOf;
    at.splitDir = m.splitDir;
  }
  for (const p of staged) {
    let current = p.id;
    for (let hops = 0; hops <= staged.length; hops++) {
      current = staged.find((x) => x.id === current)?.splitOf ?? 0;
      if (current === 0) break;
      if (current === p.id) throw new Error("that would make a pane its own pane");
    }
  }
  // Parents before the panes that hang off them: how a client replays the list.
  const ordered: PaneRecord[] = [];
  const rest = [...staged];
  while (rest.length > 0) {
    const next = rest.findIndex((p) => p.splitOf === 0 || ordered.some((o) => o.id === p.splitOf));
    ordered.push(...rest.splice(Math.max(0, next), 1));
  }
  return ordered;
}

/** The tab a root holds, as the window draws it. */
function treeOf(panes: PaneRecord[], root: number): PaneTree {
  const below = new Set([root]);
  for (let grew = true; grew; ) {
    grew = false;
    for (const p of panes) {
      if (!below.has(p.id) && below.has(p.splitOf)) {
        below.add(p.id);
        grew = true;
      }
    }
  }
  return buildTree(
    root,
    panes.filter((p) => p.id !== root && below.has(p.id)).map((p) => ({ tab: p.id, splitOf: p.splitOf, splitDir: p.splitDir })),
  );
}

const leaf = (id: number): PaneTree => ({ kind: "leaf", id });
const split = (vertical: boolean, first: PaneTree, second: PaneTree): PaneTree => ({ kind: "split", vertical, first, second });

describe("the side a point aims at", () => {
  it("is the outer thirds, and the middle is the pane itself", () => {
    expect(dropSide(0.1, 0.5)).toBe("left");
    expect(dropSide(0.9, 0.5)).toBe("right");
    expect(dropSide(0.5, 0.1)).toBe("top");
    expect(dropSide(0.5, 0.9)).toBe("bottom");
    expect(dropSide(0.5, 0.5)).toBe("onto");
  });
  it("gives the corners to the left and the right", () => {
    expect(dropSide(0.1, 0.1)).toBe("left");
    expect(dropSide(0.9, 0.9)).toBe("right");
  });
  it("starts a side at three tenths", () => {
    expect(dropSide(0.299, 0.5)).toBe("left");
    expect(dropSide(0.3, 0.5)).toBe("onto");
    expect(dropSide(0.7, 0.5)).toBe("onto");
    expect(dropSide(0.701, 0.5)).toBe("right");
  });
});

describe("the landing", () => {
  const box = { x: 100, y: 40, width: 400, height: 200 };
  it("is the half the pane would take", () => {
    expect(landingRect("left", box)).toEqual({ x: 100, y: 40, width: 200, height: 200 });
    expect(landingRect("right", box)).toEqual({ x: 300, y: 40, width: 200, height: 200 });
    expect(landingRect("top", box)).toEqual({ x: 100, y: 40, width: 400, height: 100 });
    expect(landingRect("bottom", box)).toEqual({ x: 100, y: 140, width: 400, height: 100 });
  });
  it("is nearly all of it for a trade", () => {
    expect(landingRect("onto", box)).toEqual({ x: 102, y: 42, width: 396, height: 196 });
  });
});

describe("a pane's place", () => {
  const tab = [pane(1), pane(2, 1, R), pane(3, 2, D)];
  it("is what it hangs off and how; a root hangs off nothing", () => {
    expect(placeOf(tab, 3)).toEqual({ parent: 2, dir: D });
    expect(placeOf(tab, 1)).toEqual({ parent: 0, dir: 0 });
    expect(placeOf(tab, 9)).toEqual({ parent: 0, dir: 0 });
  });
  it("is leaving a pane's children in its slot, the oldest standing in", () => {
    expect(vacating(tab, 2)).toEqual([{ tab: 3, splitOf: 1, splitDir: R }]);
    expect(vacating(tab, 3)).toEqual([]);
    const crowd = [pane(1), pane(2, 1, R), pane(3, 2, R), pane(4, 2, D)];
    expect(vacating(crowd, 2)).toEqual([
      { tab: 3, splitOf: 1, splitDir: R },
      { tab: 4, splitOf: 3, splitDir: D },
    ]);
  });
});

describe("dropping a pane on another", () => {
  // 1 | 2, and 3 under 1: (1 / 3) | 2 would be a different arrangement; this is 1 | 2 with 3 below 2.
  const base = [pane(1), pane(2, 1, R), pane(3, 2, D)];

  it("to the right hangs it off the other", () => {
    const moves = movesForDrop(base, 3, 1, "right")!;
    expect(moves).toEqual([{ tab: 3, splitOf: 1, splitDir: R }]);
    const after = rearrange(base, moves);
    // 3 took the right of 1, which 2 had; 2 is its sibling now.
    expect(placeOf(after, 3)).toEqual({ parent: 1, dir: R });
  });

  it("below hangs it off the other, downwards", () => {
    const after = rearrange(base, movesForDrop(base, 3, 1, "bottom")!);
    expect(placeOf(after, 3)).toEqual({ parent: 1, dir: D });
  });

  it("to the left takes the other's place and hangs the other off it", () => {
    const flat = [pane(1), pane(2, 1, R)];
    const moves = movesForDrop(flat, 2, 1, "left")!;
    const after = rearrange(flat, moves);
    // 2 is the root now, 1 to its right.
    expect(placeOf(after, 2)).toEqual({ parent: 0, dir: 0 });
    expect(placeOf(after, 1)).toEqual({ parent: 2, dir: R });
    expect(treeOf(after, 2)).toEqual(split(true, leaf(2), leaf(1)));
  });

  it("above puts it over the other", () => {
    const flat = [pane(1), pane(2, 1, R)];
    const after = rearrange(flat, movesForDrop(flat, 2, 1, "top")!);
    expect(treeOf(after, 2)).toEqual(split(false, leaf(2), leaf(1)));
  });

  it("to the left of a pane deeper in the tree takes that pane's slot", () => {
    // 1 | (2 / 3); 4 beside is a tab of its own, then 4 is dropped left of 3.
    const tabs = [pane(1), pane(2, 1, R), pane(3, 2, D), pane(4, 0, 0)];
    const after = rearrange(tabs, movesForDrop(tabs, 4, 3, "left")!);
    expect(treeOf(after, 1)).toEqual(split(true, leaf(1), split(false, leaf(2), split(true, leaf(4), leaf(3)))));
  });

  it("onto trades two panes", () => {
    // 1 | 2 | 3 as ((1 | 2) | 3)? built as 1, 2 off 1, 3 off 2 (right): 1 | (2 | 3).
    const row = [pane(1), pane(2, 1, R), pane(3, 2, R)];
    const after = rearrange(row, movesForDrop(row, 3, 1, "onto")!);
    // 3 where 1 was, 1 where 3 was.
    expect(treeOf(after, 3)).toEqual(split(true, leaf(3), split(true, leaf(2), leaf(1))));
  });

  it("onto trades a pane with the pane it hangs off, both ways", () => {
    const pair = [pane(1), pane(2, 1, R)];
    const down = rearrange(pair, movesForDrop(pair, 2, 1, "onto")!);
    expect(treeOf(down, 2)).toEqual(split(true, leaf(2), leaf(1)));
    const up = rearrange(pair, movesForDrop(pair, 1, 2, "onto")!);
    expect(treeOf(up, 2)).toEqual(split(true, leaf(2), leaf(1)));
  });

  it("onto trades seats, so what hangs off a seat stays at it", () => {
    // (1 over 3) | 2 as drawn; trading 1 for 2 draws (2 over 3) | 1: the seat that had 3 below it still does.
    const tabs = [pane(1), pane(2, 1, R), pane(3, 1, D)];
    expect(treeOf(tabs, 1)).toEqual(split(true, split(false, leaf(1), leaf(3)), leaf(2)));
    const after = rearrange(tabs, movesForDrop(tabs, 1, 2, "onto")!);
    expect(treeOf(after, 2)).toEqual(split(true, split(false, leaf(2), leaf(3)), leaf(1)));
  });

  it("leaves what hangs off the carried pane where the carried pane was", () => {
    // 1 | (2 | 3) as 1, 2 off 1, 3 off 2: carry 2 below 1, and 3 stays at the right of 1.
    const row = [pane(1), pane(2, 1, R), pane(3, 2, R)];
    const after = rearrange(row, movesForDrop(row, 2, 1, "bottom")!);
    expect(placeOf(after, 3)).toEqual({ parent: 1, dir: R });
    expect(placeOf(after, 2)).toEqual({ parent: 1, dir: D });
    expect(treeOf(after, 1)).toEqual(split(false, split(true, leaf(1), leaf(3)), leaf(2)));
  });

  it("across tabs: a pane joins another tab's, and leaves its own", () => {
    const tabs = [pane(1), pane(2, 1, R), pane(5), pane(6, 5, D)];
    const after = rearrange(tabs, movesForDrop(tabs, 2, 5, "right")!);
    expect(treeOf(after, 5)).toEqual(split(true, split(false, leaf(5), leaf(6)), leaf(2)));
    expect(treeOf(after, 1)).toEqual(leaf(1));
    expect(placeOf(after, 2)).toEqual({ parent: 5, dir: R });
  });

  it("onto across tabs: each takes the other's seat", () => {
    const tabs = [pane(1), pane(2, 1, R), pane(5), pane(6, 5, D)];
    const after = rearrange(tabs, movesForDrop(tabs, 2, 6, "onto")!);
    expect(treeOf(after, 1)).toEqual(split(true, leaf(1), leaf(6)));
    expect(treeOf(after, 5)).toEqual(split(false, leaf(5), leaf(2)));
  });

  it("the root carried out of its tab leaves its oldest pane as the tab", () => {
    const tabs = [pane(1), pane(2, 1, R), pane(3, 1, D), pane(5)];
    const after = rearrange(tabs, movesForDrop(tabs, 1, 5, "right")!);
    expect(placeOf(after, 2)).toEqual({ parent: 0, dir: 0 });
    expect(placeOf(after, 3)).toEqual({ parent: 2, dir: D });
    expect(placeOf(after, 1)).toEqual({ parent: 5, dir: R });
  });

  it("is no move on itself, or on a pane that is not there", () => {
    expect(movesForDrop(base, 2, 2, "onto")).toBeNull();
    expect(movesForDrop(base, 2, 9, "left")).toBeNull();
    expect(movesForDrop(base, 9, 2, "left")).toBeNull();
  });

  it("never makes the daemon refuse: every pane on every side of every other", () => {
    const shapes: PaneRecord[][] = [
      [pane(1), pane(2, 1, R)],
      [pane(1), pane(2, 1, R), pane(3, 2, D)],
      [pane(1), pane(2, 1, R), pane(3, 1, D)],
      [pane(1), pane(2, 1, R), pane(3, 2, R), pane(4, 3, D)],
      [pane(1), pane(2, 1, R), pane(3, 1, D), pane(4, 2, D), pane(7)],
    ];
    const sides = ["left", "right", "top", "bottom", "onto"] as const;
    for (const tabs of shapes) {
      for (const a of tabs) {
        for (const b of tabs) {
          for (const side of sides) {
            const moves = movesForDrop(tabs, a.id, b.id, side);
            if (!moves) continue;
            expect(() => rearrange(tabs, moves), `${a.id} ${side} of ${b.id} in ${JSON.stringify(tabs)}`).not.toThrow();
          }
        }
      }
    }
  });

  it("applyMoves reads the moves in order", () => {
    const after = applyMoves(base, [
      { tab: 3, splitOf: 1, splitDir: D },
      { tab: 3, splitOf: 2, splitDir: R },
    ]);
    expect(placeOf(after, 3)).toEqual({ parent: 2, dir: R });
    expect(placeOf(base, 3)).toEqual({ parent: 2, dir: D });
  });
});

describe("taking a pane out to a tab of its own", () => {
  it("makes it a root", () => {
    const tabs = [pane(1), pane(2, 1, R), pane(3, 2, D)];
    const after = rearrange(tabs, movesForDetach(tabs, 2)!);
    expect(placeOf(after, 2)).toEqual({ parent: 0, dir: 0 });
    // 3 stayed in the tab, in the slot 2 left.
    expect(placeOf(after, 3)).toEqual({ parent: 1, dir: R });
    expect(treeOf(after, 1)).toEqual(split(true, leaf(1), leaf(3)));
  });
  it("takes a root out of its tab, the oldest pane standing in", () => {
    const tabs = [pane(1), pane(2, 1, R), pane(3, 1, D)];
    const after = rearrange(tabs, movesForDetach(tabs, 1)!);
    expect(placeOf(after, 1)).toEqual({ parent: 0, dir: 0 });
    expect(placeOf(after, 2)).toEqual({ parent: 0, dir: 0 });
    expect(placeOf(after, 3)).toEqual({ parent: 2, dir: D });
  });
  it("is no move for a pane that is the whole of its tab", () => {
    expect(movesForDetach([pane(1)], 1)).toBeNull();
    expect(movesForDetach([pane(1)], 9)).toBeNull();
  });
});
