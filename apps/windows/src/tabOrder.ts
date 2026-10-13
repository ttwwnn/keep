// The order a workspace's tabs are listed in.
//
// The daemon lists tabs in the order they were made and knows no other. The
// order somebody gave them is the window's own (the macOS app keeps it in
// tab-order.json), as root ids, and ids are only meaningful within one
// daemon's life: a daemon started afresh counts from one again.

import type { TabInfo } from "./api";

/**
 * Items in the remembered order. Any it never heard of keep the daemon's
 * order and follow the ones it did — a tab made a moment ago appears where it
 * was made, at the end, rather than at the front of a row somebody arranged.
 */
export function arrangeTabs<T>(items: readonly T[], remembered: readonly number[] | undefined, idOf: (item: T) => number): T[] {
  if (!remembered || remembered.length === 0) return [...items];
  const known = new Set(remembered);
  const byId = new Map(items.map((item) => [idOf(item), item]));
  const placed = remembered.flatMap((id) => {
    const item = byId.get(id);
    return item === undefined ? [] : [item];
  });
  return [...placed, ...items.filter((item) => !known.has(idOf(item)))];
}

/** `ids` with `moved` taken out and put at index `to` of the result (clamped). */
export function moveId(ids: readonly number[], moved: number, to: number): number[] {
  const rest = ids.filter((id) => id !== moved);
  if (rest.length === ids.length) return [...ids];
  rest.splice(Math.min(Math.max(0, to), rest.length), 0, moved);
  return rest;
}

/** `ids` with `inserted` put before `before`, or at the end when that is not among them. */
export function insertBefore(ids: readonly number[], inserted: number, before: number | null): number[] {
  const rest = ids.filter((id) => id !== inserted);
  const at = before === null ? -1 : rest.indexOf(before);
  if (at < 0) rest.push(inserted);
  else rest.splice(at, 0, inserted);
  return rest;
}

/** `from` replaced by `to` where it stands: an heir taking the place of a root that went. */
export function substitute(ids: readonly number[], from: number, to: number): number[] {
  if (!ids.includes(from)) return [...ids];
  const swapped = ids.map((id) => (id === from ? to : id));
  // Never twice in one list, whichever of the two was there first.
  return swapped.filter((id, i) => swapped.indexOf(id) === i);
}

/**
 * The remembered orders cut down to what is alive: workspaces the daemon no
 * longer has leave, and so do ids it no longer lists as roots. An order left
 * with nothing is dropped.
 */
export function pruneOrders(
  orders: Readonly<Record<string, number[]>>,
  roots: ReadonlyMap<string, ReadonlySet<number>>,
): Record<string, number[]> {
  const out: Record<string, number[]> = {};
  for (const [workspace, ids] of Object.entries(orders)) {
    const alive = roots.get(workspace);
    if (!alive) continue;
    const kept = ids.filter((id) => alive.has(id));
    if (kept.length > 0) out[workspace] = kept;
  }
  return out;
}

const isRoot = (tab: TabInfo, ids: ReadonlySet<number>) => tab.splitOf === 0 || !ids.has(tab.splitOf);

/**
 * The roots that stopped being one while a pane of theirs stayed on, and the
 * pane that stood in: the daemon promotes the oldest surviving pane of a
 * closed root to a tab of its own (its `inherit`), and a root dragged into
 * another tab leaves its panes the same way. The tab it was is that pane now,
 * with the name it carried and the place it held.
 *
 * Looked up in the lists of two moments of one workspace, tabs still running.
 */
export function heirsOf(before: readonly TabInfo[], after: readonly TabInfo[]): Map<number, number> {
  const was = new Set(before.map((t) => t.id));
  const now = new Set(after.map((t) => t.id));
  const nowRoot = new Set(after.filter((t) => isRoot(t, now)).map((t) => t.id));
  const heirs = new Map<number, number>();
  for (const root of before.filter((t) => isRoot(t, was))) {
    if (nowRoot.has(root.id)) continue;
    // The panes under it, those under them too: a chain that ended with it.
    const below = new Set([root.id]);
    for (let grew = true; grew; ) {
      grew = false;
      for (const t of before) {
        if (!below.has(t.id) && below.has(t.splitOf)) {
          below.add(t.id);
          grew = true;
        }
      }
    }
    below.delete(root.id);
    const heir = before.find((t) => below.has(t.id) && nowRoot.has(t.id));
    if (heir) heirs.set(root.id, heir.id);
  }
  return heirs;
}

/**
 * The ids of a tab and of every pane under it, in the order the daemon lists
 * them — finished ones too, since moving a tab takes whatever the daemon
 * holds under it. The daemon issues the new ids in that same order.
 */
export function memberIds(tabs: readonly TabInfo[], root: number): number[] {
  const below = new Set([root]);
  for (let grew = true; grew; ) {
    grew = false;
    for (const t of tabs) {
      if (!below.has(t.id) && below.has(t.splitOf)) {
        below.add(t.id);
        grew = true;
      }
    }
  }
  return tabs.filter((t) => below.has(t.id)).map((t) => t.id);
}
