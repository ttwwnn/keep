// Moving a pane: which side of another it was dropped on, what the landing
// looks like, and the moves the daemon is asked for.
//
// The daemon keeps the arrangement: each pane records the pane it was split
// from and the direction (1 right, 2 below), and the window rebuilds the tree
// by replaying them. A pane's place is therefore its parent and a direction.
// To land on the right of or below another, the newcomer simply hangs off it
// — a pane is always the second half of the split it makes. To land on the
// left or above, the two change roles instead: the newcomer takes the other's
// place and the other becomes its pane. Both are the same thing said from
// opposite ends, which is why the daemon takes them together and keeps all of
// them or none (PaneMove, Workspace::rearrange).
//
// A port of Session.movePane and Session.detachPane in the macOS app.

import { SPLIT_DOWN, SPLIT_RIGHT } from "./api";

export type DropSide = "left" | "right" | "top" | "bottom" | "onto";

/** What the daemon records of a pane: where it hangs and how. A tab's root hangs off nothing. */
export interface PaneRecord {
  id: number;
  splitOf: number;
  splitDir: number;
}

/** One pane's new place, as the daemon takes it. */
export interface Move {
  tab: number;
  splitOf: number;
  splitDir: number;
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** How near a pane's edge a point must be to aim at that side, as a share of the pane. */
const EDGE = 0.3;

/**
 * Which side of a pane a point over it aims at, the point given as shares of
 * the pane's width and height from its top left: the outer thirds are the
 * sides, the left and right taking the corners, and the middle is the pane
 * itself, to trade places with it.
 */
export function dropSide(x: number, y: number): DropSide {
  if (x < EDGE) return "left";
  if (x > 1 - EDGE) return "right";
  if (y > 1 - EDGE) return "bottom";
  if (y < EDGE) return "top";
  return "onto";
}

/** The half of a pane the carried one would take, or nearly all of it for a trade. */
export function landingRect(side: DropSide, pane: Rect): Rect {
  const { x, y, width, height } = pane;
  switch (side) {
    case "onto":
      return { x: x + 2, y: y + 2, width: width - 4, height: height - 4 };
    case "left":
      return { x, y, width: width / 2, height };
    case "right":
      return { x: x + width / 2, y, width: width / 2, height };
    case "top":
      return { x, y, width, height: height / 2 };
    case "bottom":
      return { x, y: y + height / 2, width, height: height / 2 };
  }
}

/** Where a pane sits: what it hangs off and how. */
export function placeOf(panes: readonly PaneRecord[], id: number): { parent: number; dir: number } {
  const pane = panes.find((p) => p.id === id);
  return pane ? { parent: pane.splitOf, dir: pane.splitDir } : { parent: 0, dir: 0 };
}

/** The panes as they stand once the moves are made, in the order given. */
export function applyMoves(panes: readonly PaneRecord[], moves: readonly Move[]): PaneRecord[] {
  const out = panes.map((p) => ({ ...p }));
  for (const move of moves) {
    const pane = out.find((p) => p.id === move.tab);
    if (pane) {
      pane.splitOf = move.splitOf;
      pane.splitDir = move.splitDir;
    }
  }
  return out;
}

/**
 * The moves that let a pane leave without taking anything with it. Panes
 * record what they were split from, so the ones hanging off the one being
 * carried would travel with it — the whole subtree, when what was picked up
 * was a single pane. They stay: the oldest takes the departing pane's place
 * and the others hang off it, which is what closing that pane would have done.
 */
export function vacating(panes: readonly PaneRecord[], pane: number): Move[] {
  const children = panes.filter((p) => p.splitOf === pane);
  const heir = children[0];
  if (!heir) return [];
  const mine = placeOf(panes, pane);
  return [
    { tab: heir.id, splitOf: mine.parent, splitDir: mine.dir },
    ...children.slice(1).map((other) => ({ tab: other.id, splitOf: heir.id, splitDir: other.splitDir })),
  ];
}

/**
 * Two panes trading places: the layout stays as it is and each takes the
 * other's seat, so the panes hanging off a seat stay at it. Said in the
 * daemon's terms every pane is renamed — the two exchanged — and the ones
 * whose record changed are the moves.
 */
function trading(panes: readonly PaneRecord[], a: number, b: number): Move[] {
  const seat = (id: number) => (id === a ? b : id === b ? a : id);
  const next = new Map(
    panes.map((p) => [seat(p.id), { splitOf: p.splitOf === 0 ? 0 : seat(p.splitOf), splitDir: p.splitDir }] as const),
  );
  return panes.flatMap((p) => {
    const now = next.get(p.id);
    return now && (now.splitOf !== p.splitOf || now.splitDir !== p.splitDir) ? [{ tab: p.id, ...now }] : [];
  });
}

/**
 * The moves for a pane dropped on a side of another, or null when that is no
 * move. `panes` are all the panes of the workspace, in the order the daemon
 * lists them: the two may be in different tabs.
 */
export function movesForDrop(panes: readonly PaneRecord[], pane: number, target: number, side: DropSide): Move[] | null {
  if (pane === target || !panes.some((p) => p.id === pane) || !panes.some((p) => p.id === target)) return null;
  if (side === "onto") return trading(panes, pane, target);
  const leaving = vacating(panes, pane);
  // Where the target sits once the pane has gone from where it was.
  const theirs = placeOf(applyMoves(panes, leaving), target);
  switch (side) {
    case "right":
      return [...leaving, { tab: pane, splitOf: target, splitDir: SPLIT_RIGHT }];
    case "bottom":
      return [...leaving, { tab: pane, splitOf: target, splitDir: SPLIT_DOWN }];
    case "left":
      return [
        ...leaving,
        { tab: pane, splitOf: theirs.parent, splitDir: theirs.dir },
        { tab: target, splitOf: pane, splitDir: SPLIT_RIGHT },
      ];
    case "top":
      return [
        ...leaving,
        { tab: pane, splitOf: theirs.parent, splitDir: theirs.dir },
        { tab: target, splitOf: pane, splitDir: SPLIT_DOWN },
      ];
  }
}

/** The moves that make a pane a tab of its own, or null when it already is the whole of one. */
export function movesForDetach(panes: readonly PaneRecord[], pane: number): Move[] | null {
  const me = panes.find((p) => p.id === pane);
  if (!me) return null;
  // A lone root has nothing to be taken out of, and nothing hangs off it.
  if (me.splitOf === 0 && !panes.some((p) => p.splitOf === pane)) return null;
  return [...vacating(panes, pane), { tab: pane, splitOf: 0, splitDir: 0 }];
}
