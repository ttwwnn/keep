// The names given to tabs, as the core keeps them (src-tauri/src/names.rs,
// docs/restauro.md).
//
// The window keeps the names it gives in its own state; the core — `keep
// restauro`, which writes every tab down for the next restart with the name it
// has, and `keep nova --nome`, which names the tab it opens — reads and writes
// them in the macOS app's file: `names.json`, `{"daemonStart": <seconds>,
// "tabs": {"<workspace>␟<tab>": "<name>"}, "workspaces": {…}}`, true for the one
// daemon that started at that moment, since a daemon's tab numbers start again
// from one when it does. This is the window's end of that file: what it writes
// from its names, and which names in it are somebody else's to take.
//
// Pure functions; iaChores.ts reads and writes the file through the app.

import { tabKey } from "./state";

/** The file, with the fields the window cares about. */
export interface NamesFile {
  /** When the daemon these tab numbers belong to started, in seconds; null when not said. */
  daemonStart: number | null;
  /** By `"workspace␟tab"`. */
  tabs: Record<string, string>;
  /** By the workspace's own name. */
  workspaces: Record<string, string>;
}

/** What separates the workspace from the tab number in the file's keys (the window's own keys use NUL). */
const SEPARATOR = "\u001f";

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);

const strings = (v: unknown): Record<string, string> => {
  const out: Record<string, string> = {};
  if (isRecord(v)) for (const [k, s] of Object.entries(v)) if (typeof s === "string" && s.trim()) out[k] = s;
  return out;
};

/**
 * When the daemon an identity names started, in seconds: the identity is
 * `"<pid>-<ms>"` with the start the daemon says (api.daemonIdentity). A
 * daemon that could not say gave its process's creation time instead, a
 * different clock and 100 ns steps — too large to be milliseconds since 1970.
 */
export function daemonStartOf(identity: string): number | null {
  const match = /^\d+-(\d+)$/.exec(identity);
  if (!match) return null;
  const ms = Number(match[1]);
  return Number.isSafeInteger(ms) && ms > 0 && ms < 1e14 ? ms / 1000 : null;
}

/** The same daemon: the core compares to the hundredth of a second too. */
export const sameStart = (a: number | null, b: number | null) => a !== null && b !== null && Math.abs(a - b) < 0.01;

/** Whether a stored identity and the one now are one daemon, for the window's state that holds only for it. */
export function sameDaemon(stored: string, now: string): boolean {
  if (stored === now) return true;
  const [pid, started] = stored.split("-");
  const [nowPid] = now.split("-");
  // Before the daemon said when it started, the window kept its process's creation time (a number of 17 digits, not
  // milliseconds): the same process is the same daemon, whichever way it is told.
  return stored !== "" && pid === nowPid && /^\d{16,}$/.test(started ?? "");
}

/** The file as it was read, tolerant of what it lacks; null for what is not one. */
export function readNamesFile(text: string | null): NamesFile | null {
  if (text === null) return null;
  let json: unknown;
  try {
    json = JSON.parse(text.replace(/^﻿/, ""));
  } catch {
    return null;
  }
  if (!isRecord(json)) return null;
  const start = json.daemonStart;
  return {
    daemonStart: typeof start === "number" && Number.isFinite(start) ? start : null,
    tabs: strings(json.tabs),
    workspaces: strings(json.workspaces),
  };
}

/** The file the window's names make for the daemon that started at `daemonStart`. */
export function namesFileOf(
  state: { tabNames: Record<string, string>; workspaceNames: Record<string, string> },
  daemonStart: number,
): NamesFile {
  const tabs: Record<string, string> = {};
  for (const [key, name] of Object.entries(state.tabNames)) {
    const at = key.lastIndexOf("\u0000");
    if (at > 0 && name.trim()) tabs[`${key.slice(0, at)}${SEPARATOR}${key.slice(at + 1)}`] = name;
  }
  return { daemonStart, tabs, workspaces: { ...state.workspaceNames } };
}

const same = (a: Record<string, string>, b: Record<string, string>) => {
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every((k) => a[k] === b[k]);
};

/** Whether two files say the same. */
export const sameNames = (a: NamesFile | null, b: NamesFile | null) =>
  a === b || (a !== null && b !== null && sameStart(a.daemonStart, b.daemonStart) && same(a.tabs, b.tabs) && same(a.workspaces, b.workspaces));

/** A name somebody else put on a tab. */
export interface TabName {
  workspace: string;
  tab: number;
  name: string;
}

/**
 * The names in `file` that are not what the window last knew of it (`known`:
 * what it wrote or read there, nothing the first time): the core named those
 * tabs since, or the window never heard of the name. A name the window took
 * off a tab is not in the file's news — the file only repeats it — and does
 * not come back.
 */
export function foreignNames(file: NamesFile, known: NamesFile | null): TabName[] {
  const news: TabName[] = [];
  for (const [key, name] of Object.entries(file.tabs)) {
    if (known?.tabs[key] === name) continue;
    const at = key.lastIndexOf(SEPARATOR);
    const tab = Number(key.slice(at + 1));
    if (at <= 0 || !Number.isInteger(tab) || tab <= 0) continue;
    news.push({ workspace: key.slice(0, at), tab, name });
  }
  return news;
}

/**
 * Names the window last knew in the file that are gone from it: another program took them off (the core, when a
 * conversation put to sleep wakes up and says its own title again).
 */
export function droppedNames(file: NamesFile, known: NamesFile | null): TabName[] {
  if (!known) return [];
  const gone = Object.fromEntries(Object.entries(known.tabs).filter(([key]) => !(key in file.tabs)));
  return foreignNames({ ...known, tabs: gone }, null);
}

/** What the window knew of the file, kept only if it was of this daemon: another's is no news to compare with. */
export const knownFor = (known: NamesFile | null, daemonStart: number) =>
  known !== null && sameStart(known.daemonStart, daemonStart) ? known : null;

export interface NamesRound {
  /** The window's names. */
  state: { tabNames: Record<string, string>; workspaceNames: Record<string, string> };
  /** When the daemon the window is on started, in seconds. */
  daemonStart: number;
  /** The file's text, when it changed since the window last looked; null when it did not, or there is none. */
  text: string | null;
  /** What the window knew of the file. */
  known: NamesFile | null;
}

export interface NamesPlan {
  /** Names the core put on tabs, to become the window's. */
  adopt: TabName[];
  /** Names the core took off tabs, to go from the window too: only where the window still has them as they were. */
  drop: TabName[];
  /** The file to write, when what the window has is not what is in it. */
  write: NamesFile | null;
  /** What the window knows of the file after this. */
  known: NamesFile | null;
}

/**
 * One round of names in both directions: the news in the file (the tabs a
 * restart brought back with their names, `keep nova --nome`) become the
 * window's, and then — with them in — the window's names are the file's, for
 * the next picture of the tabs to carry.
 */
export function planNames({ state, daemonStart, text, known }: NamesRound): NamesPlan {
  let knew = knownFor(known, daemonStart);
  const adopt: TabName[] = [];
  const drop: TabName[] = [];
  const file = readNamesFile(text);
  if (file && sameStart(file.daemonStart, daemonStart)) {
    adopt.push(...foreignNames(file, knew).filter((n) => state.tabNames[tabKey(n.workspace, n.tab)] !== n.name));
    drop.push(...droppedNames(file, knew).filter((n) => state.tabNames[tabKey(n.workspace, n.tab)] === n.name));
    knew = file;
  }
  const tabNames = { ...state.tabNames };
  for (const n of adopt) tabNames[tabKey(n.workspace, n.tab)] = n.name;
  for (const n of drop) delete tabNames[tabKey(n.workspace, n.tab)];
  const mine = namesFileOf({ tabNames, workspaceNames: state.workspaceNames }, daemonStart);
  return { adopt, drop, write: sameNames(mine, knew) ? null : mine, known: knew };
}
