// Several windows over one daemon: which workspaces each window lists, which
// tab each shows, and what that means for opening one, taking a workspace off
// a window's list, going to one, and the tab shown in two places. A port of
// what the macOS app's Session keeps per window (WindowView, `adopt`,
// `.removeWorkspace`, `placeOrphans`): the facts that really differ from one
// window to the next, and nothing else.
//
// A window lists its workspaces in `order` (the sidebar's order) less the ones
// put away (`hidden`, the same for every window: it is the app's, as the
// macOS put-away.json is). `order` also remembers where a workspace put away
// sat, so that going to it again puts it back there. Every running workspace
// is in some window's list or put away: the first window takes the ones
// nobody lists.
//
// Pure functions; windowSync.ts keeps them in step with the other windows.

import { putAwayWith, putAwayWithout } from "./putAway";

/**
 * The window the app starts with: slot 0, as the macOS app's first window. The others are `w1`, `w2`… and the
 * app gives them (src-tauri: windows.rs), the lowest slot nobody is using, so that the furniture of a window that
 * closed goes to the next.
 */
export const MAIN_LABEL = "main";

// ------------------------------------------------------------------ what a window carries and shows

/** The tab a window has in front. */
export interface Showing {
  workspace: string;
  tab: number;
}

/** What one window tells the others of itself. */
export interface Placement {
  /** The workspaces it lists, in its sidebar's order. */
  carries: string[];
  /** The tab in front. */
  showing: Showing | null;
}

/** A window, as the others know it. */
export interface Peer {
  label: string;
  placement: Placement;
}

export const emptyPlacement = (): Placement => ({ carries: [], showing: null });

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);

/** A placement as another window sent it, whatever it holds. */
export function readPlacement(raw: unknown): Placement {
  if (!isRecord(raw)) return emptyPlacement();
  const carries = Array.isArray(raw.carries) ? raw.carries.filter((n): n is string => typeof n === "string") : [];
  const showing =
    isRecord(raw.showing) && typeof raw.showing.workspace === "string" && Number.isInteger(raw.showing.tab)
      ? { workspace: raw.showing.workspace, tab: raw.showing.tab as number }
      : null;
  return { carries, showing };
}

/** The windows as the app lists them (oldest first), whatever it sent. */
export function readPeers(raw: unknown): Peer[] {
  if (!Array.isArray(raw)) return [];
  const peers: Peer[] = [];
  for (const entry of raw) {
    if (!isRecord(entry) || typeof entry.label !== "string") continue;
    peers.push({ label: entry.label, placement: readPlacement(entry.placement) });
  }
  return peers;
}

/**
 * The workspaces a window lists: its order less the ones put away. (The drop-down terminal's workspace is never
 * among what the window is given: quickWorkspace.ts.)
 */
export function carriedOf(order: readonly string[], hidden: readonly string[]): string[] {
  const away = new Set(hidden);
  return order.filter((name) => !away.has(name));
}

/** Whether a window other than `me` lists the workspace. */
export function carriedElsewhere(peers: readonly Peer[], me: string, workspace: string): boolean {
  return peers.some((p) => p.label !== me && p.placement.carries.includes(workspace));
}

/** What the other windows list, with `mine` in place of what this window last said of itself. */
export function carriedByAll(peers: readonly Peer[], me: string, mine: readonly string[]): Set<string> {
  const all = new Set(mine);
  for (const p of peers) if (p.label !== me) for (const name of p.placement.carries) all.add(name);
  return all;
}

/**
 * The running workspaces no window lists and nobody put away — made by the
 * command line, by a login script, while the app was not running — which the
 * first window takes (`placeOrphans`: "or the first, which is where
 * something with nowhere else to go lands"). In the daemon's order.
 */
export function orphansOf(running: readonly string[], listed: ReadonlySet<string>, hidden: readonly string[]): string[] {
  const away = new Set(hidden);
  return running.filter((name) => !listed.has(name) && !away.has(name));
}

/**
 * How long a workspace goes unlisted, with other windows open, before the
 * oldest window takes it. The window that made one says it lists it a beat
 * after making it (its state is kept 200 ms after a change, then told to the
 * app), and the oldest window, asking the daemon in that beat, saw a
 * workspace nobody listed and took it too: it showed in both. One seen in the
 * beat is not an orphan but a workspace on its way to its window. With one
 * window there is nobody else to list it, and it is taken at once.
 */
export const ORPHAN_GRACE_MS = 1000;

/**
 * The orphans that have been seen unlisted for the grace. `seen` is when each
 * was first seen unlisted, kept between calls and forgotten only when the
 * workspace ends (`running`): two readers that ask in a different moment must
 * not start each other's clock again, and one that comes back unlisted later
 * — its window closed — has waited already. `wait` is how long until the next
 * one is due, or null when none is waiting.
 */
export function settledOrphans(
  orphans: readonly string[],
  seen: Map<string, number>,
  now: number,
  running: readonly string[],
  graceMs = ORPHAN_GRACE_MS,
): { settled: string[]; wait: number | null } {
  for (const name of [...seen.keys()]) if (!running.includes(name)) seen.delete(name);
  const settled: string[] = [];
  let wait: number | null = null;
  for (const name of orphans) {
    if (!seen.has(name)) seen.set(name, now);
    const left = seen.get(name)! + graceMs - now;
    if (left <= 0) settled.push(name);
    else wait = wait === null ? left : Math.min(wait, left);
  }
  return { settled, wait };
}

/** The oldest window: the one that takes what nobody lists and does what the app does once. */
export const leaderOf = (peers: readonly Peer[]): string | null => peers[0]?.label ?? null;

/** Whether another window has this tab in front: the ⧉ the tab wears. */
export function shownElsewhere(peers: readonly Peer[], me: string, workspace: string, tab: number): boolean {
  return peers.some((p) => p.label !== me && p.placement.showing?.workspace === workspace && p.placement.showing.tab === tab);
}

// ------------------------------------------------------------------ the list of one window

export interface Listing {
  /** Every workspace the window knows, in its order, those put away among them. */
  order: string[];
  /** The workspaces put away, the same for every window. */
  hidden: string[];
}

/**
 * Go to a workspace: it is put back if it was put away, and listed here if
 * this window did not list it — at the end, where something new goes, unless
 * the window remembers where it sat. "Entering a workspace also puts it in the
 * window's list: the list is the record of where this window has been."
 */
export function adopt(listing: Listing, workspace: string): Listing {
  return {
    hidden: putAwayWithout(listing.hidden, workspace),
    order: listing.order.includes(workspace) ? listing.order : [...listing.order, workspace],
  };
}

/**
 * Take a workspace off this window's list. If another window lists it, that is
 * all: it leaves this one. If this was the last window listing it, it is put
 * away — out of every sidebar, running, a Go To from coming back — and the
 * window keeps its place in the order, so that going to it again puts it back.
 */
export function remove(listing: Listing, peers: readonly Peer[], me: string, workspace: string): Listing {
  if (carriedElsewhere(peers, me, workspace)) {
    return { hidden: listing.hidden, order: listing.order.filter((name) => name !== workspace) };
  }
  return { hidden: putAwayWith(listing.hidden, workspace), order: listing.order };
}

/**
 * The names that left the put-away list because some other window went to
 * them are listed there now; the place this window remembered for them is
 * stale, and keeping it would list them here as well.
 */
export function forgetTaken(order: readonly string[], hiddenBefore: readonly string[], hiddenNow: readonly string[]): string[] {
  const now = new Set(hiddenNow);
  const taken = new Set(hiddenBefore.filter((name) => !now.has(name)));
  return taken.size === 0 ? [...order] : order.filter((name) => !taken.has(name));
}

// ------------------------------------------------------------------ a tab pulled out of the row

/** How far, in pixels, the pointer must go above or below the row before a tab is being pulled out of it rather than along it. */
export const TEAR_THRESHOLD = 40;

export interface Band {
  top: number;
  bottom: number;
}

/**
 * Whether the pointer is out of the row's band by enough to mean a window.
 * Reordering is done with the wrist and a wrist wanders vertically: the
 * threshold is generous — about one and a half rows —, and only leaving the
 * band, up over the top of the window or down into the terminal, means this.
 */
export function pulledOut(y: number, row: Band, threshold = TEAR_THRESHOLD): boolean {
  return y < row.top - threshold || y > row.bottom + threshold;
}

/** A rectangle on the screen, in physical pixels. */
export interface Frame {
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * Where a window made from a torn tab appears: the size of the window it came
 * from — two windows on one tab have to agree how big it is, and starting
 * them equal makes that agreement a no-op —, put so that the tab is under the
 * pointer where it was let go: the new window shows the same row, so the
 * pointer's place in the old window (`press`, in CSS pixels from its corner)
 * is its place in the new one.
 */
export function frameOfTornTab(cursor: { x: number; y: number }, press: { x: number; y: number }, scale: number, size: { width: number; height: number }): Frame {
  return {
    x: Math.round(cursor.x - press.x * scale),
    y: Math.round(cursor.y - press.y * scale),
    width: size.width,
    height: size.height,
  };
}

/** The same size as the window it was opened from, stepped down and across (the macOS app's ⇧⌘N steps 24 points). */
export function cascaded(from: Frame, scale: number, step = 24): Frame {
  const by = Math.round(step * scale);
  return { x: from.x + by, y: from.y + by, width: from.width, height: from.height };
}

// ------------------------------------------------------------------ one tab, two windows

/**
 * What a terminal tells the daemon its size is. A tab has one size and may have
 * several viewers; the daemon fits the smallest and leaves out one that says it
 * is not looking (0×0). With one window the terminals out of sight go on
 * keeping the size of the room they are kept in, as they always did; with
 * several, a terminal out of sight in this window may be the one another is
 * looking at, and must not shrink it.
 */
export function viewerSize(visible: boolean, severalWindows: boolean, cols: number, rows: number): { cols: number; rows: number } {
  return visible || !severalWindows ? { cols, rows } : { cols: 0, rows: 0 };
}
