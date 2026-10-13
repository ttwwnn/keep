// The panes of a tab as the window divides them: where each divider stands,
// and moving about by keyboard — which pane is beside another, which divider
// a pane's arrow moves.
//
// The daemon keeps who hangs off whom, not how big each half is: the share a
// divider gives is the window's own, and stays in memory (the macOS app keeps
// it only to put a tab back as it was after its surfaces are let go of).

import { signal } from "@preact/signals";
import type { PaneTree } from "./layout";

export type Direction = "left" | "right" | "up" | "down";

type Split = Extract<PaneTree, { kind: "split" }>;

/** Where the dividers stand, as the near side's share of its split, by `splitKey`. */
export const shares = signal<Readonly<Record<string, number>>>({});

export const HALF = 0.5;

/** The share a divider gives, until somebody moves it, one half. */
export function shareOf(key: string): number {
  return shares.value[key] ?? HALF;
}

export function setShare(key: string, share: number): void {
  if (shares.value[key] === share) return;
  shares.value = { ...shares.value, [key]: share };
}

/** Forget the dividers of tabs that are gone: only those whose key starts with a prefix still wanted are kept. */
export function keepShares(wanted: (key: string) => boolean): void {
  const kept = Object.entries(shares.value).filter(([key]) => wanted(key));
  if (kept.length !== Object.keys(shares.value).length) shares.value = Object.fromEntries(kept);
}

/** The dividers of one tab now known by another's key: an heir carrying on a closed root's tab keeps what the panes still there had. */
export function moveShares(fromPrefix: string, toPrefix: string): void {
  const moved = Object.entries(shares.value).filter(([key]) => key.startsWith(`${fromPrefix}\u0000`));
  if (moved.length === 0) return;
  const kept = Object.fromEntries(Object.entries(shares.value).filter(([key]) => !key.startsWith(`${fromPrefix}\u0000`)));
  for (const [key, share] of moved) kept[`${toPrefix}${key.slice(fromPrefix.length)}`] = share;
  shares.value = kept;
}

/** The leftmost, topmost pane of a tree. */
export function firstLeaf(tree: PaneTree): number {
  return tree.kind === "leaf" ? tree.id : firstLeaf(tree.first);
}

/**
 * What a divider is known by. The two panes at the head of its sides are
 * enough: each split of a tab has its own pair, and a divider keeps its share
 * when a pane is split off somewhere inside one of its sides.
 */
export function splitKey(prefix: string, split: Split): string {
  return `${prefix}\u0000${split.vertical ? "v" : "h"}${firstLeaf(split.first)}-${firstLeaf(split.second)}`;
}

/**
 * A share held to what each side needs to stay usable. `near` and `far` are
 * the least, as shares of the whole; if they do not both fit, the middle.
 */
export function clampShare(share: number, near: number, far: number): number {
  if (near + far >= 1) return HALF;
  return Math.min(Math.max(share, near), 1 - far);
}

/** The share a pointer at `at` (measured from the split's start) asks for, given the split's length and its divider's thickness. */
export function shareAt(at: number, length: number, divider = 1): number {
  const room = length - divider;
  return room > 0 ? (at - divider / 2) / room : HALF;
}

// ------------------------------------------------------------------ about the panes

/** The chain of splits a pane sits in, outermost first. */
function splitsAbove(tree: PaneTree, pane: number): Split[] | null {
  if (tree.kind === "leaf") return tree.id === pane ? [] : null;
  const inFirst = splitsAbove(tree.first, pane);
  if (inFirst) return [tree, ...inFirst];
  const inSecond = splitsAbove(tree.second, pane);
  return inSecond ? [tree, ...inSecond] : null;
}

/**
 * The divider a pane's arrow moves: the nearest split outward from the pane
 * facing the way the arrow runs. A pane is inside several splits at once —
 * that is what nesting them means — and only some have a divider that runs
 * the way this asks.
 */
export function dividerFor(tree: PaneTree, pane: number, direction: Direction): Split | null {
  const wantsVertical = direction === "left" || direction === "right";
  const chain = splitsAbove(tree, pane);
  if (!chain) return null;
  return [...chain].reverse().find((split) => split.vertical === wantsVertical) ?? null;
}

/**
 * The share after a step of the arrow: the divider goes the way the arrow
 * points, whichever side of it the pane is on (as the macOS app's does).
 */
export function stepShare(share: number, direction: Direction, step: number, length: number): number {
  if (length <= 0) return share;
  const way = direction === "right" || direction === "down" ? 1 : -1;
  return share + (way * step) / length;
}

export interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/**
 * The pane beside another in a direction, answered from where the panes are
 * on screen rather than from where they sit in the tree. The two are not the
 * same question: a pane is the left child of some split, but what is to the
 * left of it may be two levels up the tree and one across, and the answer
 * changes as splits nest.
 *
 * The nearest pane that overlaps it along the other axis; of equals, the one
 * whose middle is nearest. A divider is a point wide, so touching counts as
 * beside.
 */
export function paneBeside(boxes: ReadonlyMap<number, Box>, origin: number, direction: Direction): number | null {
  const from = boxes.get(origin);
  if (!from) return null;
  const middleX = (b: Box) => (b.left + b.right) / 2;
  const middleY = (b: Box) => (b.top + b.bottom) / 2;
  let best: { pane: number; gap: number; offset: number } | null = null;
  for (const [pane, box] of boxes) {
    if (pane === origin) continue;
    let gap: number;
    let alongside: boolean;
    switch (direction) {
      case "left":
        gap = from.left - box.right;
        alongside = box.top < from.bottom && box.bottom > from.top;
        break;
      case "right":
        gap = box.left - from.right;
        alongside = box.top < from.bottom && box.bottom > from.top;
        break;
      case "up":
        gap = from.top - box.bottom;
        alongside = box.left < from.right && box.right > from.left;
        break;
      case "down":
        gap = box.top - from.bottom;
        alongside = box.left < from.right && box.right > from.left;
        break;
    }
    if (!alongside || gap < -1) continue;
    const offset = Math.abs(middleX(box) - middleX(from)) + Math.abs(middleY(box) - middleY(from));
    if (!best || gap < best.gap || (gap === best.gap && offset < best.offset)) best = { pane, gap, offset };
  }
  return best?.pane ?? null;
}
