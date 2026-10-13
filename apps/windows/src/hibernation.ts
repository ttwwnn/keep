// Which tabs keep their terminals.
//
// A terminal holds a screen, ten thousand lines of scrollback and a
// connection for as long as it exists, whether or not it is on screen, and a
// window that kept one for every tab ever opened would pay for all of them.
// So the tabs used least recently are put to sleep: their terminals are let
// go of. Nothing else about the tab changes — it is still in the row, the
// sidebar and the picker, its state still comes from the daemon, which still
// has the shell — and showing it builds the terminals again, each repainted
// from the daemon.
//
// A port of HibernationPolicy and Hibernation in the macOS app.

import { signal } from "@preact/signals";

/** How many tabs keep their terminals, the one on screen included. */
export const DEFAULT_LIMIT = 6;

export interface Candidate {
  /** On screen: never let go of. */
  visible: boolean;
  /** When it was last put on screen, on a clock of the window's. */
  lastShown: number;
  /** Something the daemon cannot give back lives in its terminals: a selection, a composition under way, the keyboard itself. */
  blocked: boolean;
}

/**
 * The indices of the candidates to put to sleep. The tabs on screen count
 * against the limit and are never chosen; of the rest, the most recently
 * shown fill what room is left, and the others are chosen unless they are
 * blocked. A blocked tab stays over the limit rather than lose what it holds,
 * and is asked about again.
 */
export function sleepers(candidates: readonly Candidate[], limit: number): number[] {
  const visible = candidates.filter((c) => c.visible).length;
  const room = Math.max(0, limit - visible);
  return candidates
    .map((c, i) => [c, i] as const)
    .filter(([c]) => !c.visible)
    .sort(([a], [b]) => b.lastShown - a.lastShown)
    .slice(room)
    .filter(([c]) => !c.blocked)
    .map(([, i]) => i);
}

// ------------------------------------------------------------------ who is awake

/** The tabs that have terminals, by `tabKey`: those shown since the window opened, less the ones put to sleep. */
export const awake = signal<ReadonlySet<string>>(new Set());

const shownAt = new Map<string, number>();
let clock = 0;

/** The tabs' order of being shown, newest last, for the policy to ask. */
export function lastShown(key: string): number {
  return shownAt.get(key) ?? 0;
}

/** A tab was put on screen: it has terminals from now on. */
export function wake(key: string): void {
  shownAt.set(key, ++clock);
  if (!awake.value.has(key)) awake.value = new Set([...awake.value, key]);
}

/** Let go of these tabs' terminals. */
export function sleep(keys: readonly string[]): void {
  if (keys.length === 0) return;
  const drop = new Set(keys);
  awake.value = new Set([...awake.value].filter((key) => !drop.has(key)));
}

/** A tab was re-filed under another key — moved to another workspace, an heir standing in for a root — and is as awake as it was. */
export function refile(from: string, to: string): void {
  const at = shownAt.get(from);
  if (at !== undefined) {
    shownAt.delete(from);
    shownAt.set(to, at);
  }
  if (awake.value.has(from)) awake.value = new Set([...awake.value].map((key) => (key === from ? to : key)));
}

/** A tab that no longer exists keeps nothing. */
export function forget(alive: ReadonlySet<string>): void {
  for (const key of [...shownAt.keys()]) if (!alive.has(key)) shownAt.delete(key);
  const kept = [...awake.value].filter((key) => alive.has(key));
  if (kept.length !== awake.value.size) awake.value = new Set(kept);
}

/** Things under way that the policy waits out: a pane being carried can land on any tab. */
export const holds = signal(0);
