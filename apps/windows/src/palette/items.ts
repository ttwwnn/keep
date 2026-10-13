// What the picker lists, as plain values. The tabs to go to, latest visit
// first; the folders to start something in; the commands; the lines of
// history. Nothing here reads the window — source.ts collects the facts and
// this decides what they make of the list — so all of it can be tested
// without one. A port of the macOS picker's model (Session.pickerItems and
// PickerModel in Snapshots.swift, and the filtering in PickerView.swift).

import type { SearchHit } from "../api";
import { COMMANDS, isEnabled, type CommandId } from "../commands";
import { baseName } from "../layout";
import { tabKey } from "../state";
import { fuzzyScore, homeRelative, matchRanges } from "../text";
import { matchSpan } from "./hits";

export type ItemKind =
  /** A tab that exists, in some workspace. */
  | { type: "running"; workspace: string; tab: number }
  /** A folder to open a new workspace in. */
  | { type: "destination"; path: string }
  /** A line of history: the tab that shows it, the pane that holds it (a pane is not a tab), and where the match sits in the line. */
  | { type: "hit"; hit: SearchHit; root: number; span: [number, number] | null }
  /** Something to do. */
  | { type: "command"; command: CommandId }
  /** The workspace that does not exist yet, named by what was typed. */
  | { type: "create"; name: string };

export interface PickerItem {
  id: string;
  kind: ItemKind;
  /** The daemon's name of the workspace the row belongs to; empty for a folder or a command. */
  workspace: string;
  /** The left column, and the whole of the disambiguation between tabs: the workspace's name alone, `name/tail` when the tab sits somewhere inside it, `name 2` when position is the only thing telling siblings apart. A command's is its group. */
  context: string;
  /** The row's own text: the tab's title, a folder's name, a matched line, a command's name. */
  title: string;
  /** A folder's whole path, how many panes a tab has, or where a hit is. */
  detail: string;
  /** What holds the terminal — `claude`, `pwsh`. A title is what a program decided to call itself, which may be nothing, and may be the same sentence in six tabs. */
  command: string;
  /** Where the row is on disk: a tab's folder, or the folder a destination opens. Empty for a line of history. */
  path: string;
  busy: boolean;
  /** When the tab last did anything, in unix milliseconds; 0 when unknown. */
  lastActive: number;
  /** A command with nothing bound to it: shown dim, and never the selection. */
  disabled?: boolean;
}

/** The three columns a query is tried on, in reading order. */
export type Column = "context" | "title" | "detail";
const COLUMNS: readonly Column[] = ["context", "title", "detail"];

/** What put a row in the list, when the query did: the column it matched on, and where. */
export interface Lit {
  column: Column;
  ranges: [number, number][];
}

/** What the list draws: a heading over a run of rows, or a row. */
export type Row = { type: "heading"; text: string } | { type: "item"; item: PickerItem };

// ------------------------------------------------------------------ the tabs

/** What a tab says of itself, flattened to what the list needs. */
export interface TabFacts {
  workspace: string;
  /** The tab's root pane. */
  tab: number;
  /** Where it sits among its workspace's tabs, from 0. */
  position: number;
  title: string;
  /** Its panes, the root included. */
  panes: number;
  program: string;
  cwd: string;
  busy: boolean;
  lastActive: number;
}

export interface WorkspaceFacts {
  name: string;
  /** What the sidebar calls it: the name, unless somebody renamed it. */
  label: string;
  /** Where the workspace is: the folder its first tab is in. */
  place: string;
  tabs: readonly TabFacts[];
}

export interface GoToInput {
  workspaces: readonly WorkspaceFacts[];
  /** `tabKey`s, the latest visit first. */
  recent: readonly string[];
  /** The tab in front, which is not a place to go. */
  here: { workspace: string; tab: number } | null;
  /** Folders to start something in, best first, as the machine writes them. */
  directories: readonly string[];
  home: string;
}

/** A path with one kind of separator and none at the end; whether Windows wrote it decides how it is compared. */
function plain(path: string): string {
  const slashed = path.replace(/\\/g, "/");
  return slashed.length > 1 ? slashed.replace(/\/+$/, "") : slashed;
}

const windowsPath = (path: string) => /^[a-z]:/i.test(path) || path.startsWith("\\\\");

/**
 * The part of `cwd` below the workspace's own folder, or null when it is not
 * below it, is the folder itself, or is unknown. The shortest true thing that
 * separates siblings — `777leads/api` rather than `C:\www\777leads\api`, and
 * short enough to sit in a column. A tab that wandered outside its workspace
 * keeps its last component rather than a path nobody can read in a column.
 */
export function relativePlace(cwd: string, place: string, name: string): string | null {
  if (!cwd) return null;
  const path = plain(cwd);
  const root = plain(place);
  const same = windowsPath(cwd) || windowsPath(place);
  const fold = (text: string) => (same ? text.toLowerCase() : text);
  // A tail is only worth showing if it is a name: at a drive's root the last component is "" or "/".
  const named = (tail: string) => (tail === "" || tail === "/" ? null : tail);
  const last = (text: string) => text.slice(text.lastIndexOf("/") + 1);
  if (!place) {
    // No folder to measure against: the tab's own last component is all there is, and says nothing when it merely repeats the name.
    const tail = last(path);
    return fold(tail) === fold(name) ? null : named(tail);
  }
  if (fold(path) === fold(root)) return null;
  // The slash matters: a root of `proj` must not claim `project-two`.
  if (!fold(path).startsWith(`${fold(root)}/`)) return named(last(path));
  return named(path.slice(root.length + 1));
}

/** How long ago, in as few characters as carry it: a column that has to align, beside identifiers rather than inside a sentence. */
export function recency(lastActive: number, now: number): string {
  if (!lastActive) return "";
  const seconds = (now - lastActive) / 1000;
  if (seconds < 60) return "agora";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)}h`;
  if (seconds < 604_800) return `${Math.floor(seconds / 86_400)}d`;
  return `${Math.floor(seconds / 604_800)}sem`;
}

/**
 * Everything running, most recently visited first, then the places to start
 * something new — with two things settled for the window that asks.
 *
 * The tab in front is left out: the list is places to go, and the place you
 * are is not one. And the first row is the last tab you were in somewhere
 * else, so that the picker and Enter are the way back to whatever you were
 * doing before this — the gesture Alt+Tab makes between programs. Without it
 * the row under the cursor would be the next tab of the workspace already in
 * front of you, and the most common move of all would take aiming.
 */
export function goToItems(input: GoToInput): PickerItem[] {
  const { workspaces, recent, here, directories, home } = input;
  const all = new Map<string, { facts: TabFacts; workspace: WorkspaceFacts }>();
  for (const workspace of workspaces) for (const facts of workspace.tabs) all.set(tabKey(workspace.name, facts.tab), { facts, workspace });

  const seen = new Set<string>(here ? [tabKey(here.workspace, here.tab)] : []);
  const picked: { facts: TabFacts; workspace: WorkspaceFacts }[] = [];
  const take = (key: string) => {
    const entry = all.get(key);
    if (!entry || seen.has(key)) return;
    seen.add(key);
    picked.push(entry);
  };
  // Where you have been, then whatever you have not visited yet — most recently active first. The visits only know the tabs this window went to, so without the tail the rest would arrive in the order the daemon lists them, which at a cold start is the whole list.
  for (const key of recent) take(key);
  [...all.keys()]
    .filter((key) => !seen.has(key))
    .sort((a, b) => all.get(b)!.facts.lastActive - all.get(a)!.facts.lastActive)
    .forEach(take);

  // A position is only worth showing when there is a sibling to be told apart from, so what counts is the rows the list has.
  const rows = new Map<string, number>();
  for (const { workspace } of picked) rows.set(workspace.name, (rows.get(workspace.name) ?? 0) + 1);

  const running = picked.map(({ facts, workspace }): PickerItem => {
    const tail = relativePlace(facts.cwd, workspace.place, workspace.name);
    const context = tail
      ? `${workspace.label}/${tail}`
      : rows.get(workspace.name)! > 1
        ? `${workspace.label} ${facts.position + 1}`
        : workspace.label;
    return {
      id: `run:${tabKey(workspace.name, facts.tab)}`,
      kind: { type: "running", workspace: workspace.name, tab: facts.tab },
      workspace: workspace.name,
      context,
      title: facts.title,
      detail: facts.panes > 1 ? `${facts.panes} painéis` : "",
      command: facts.program,
      path: facts.cwd,
      busy: facts.busy,
      lastActive: facts.lastActive,
    };
  });

  // Two tabs of one workspace in the same folder come out with the same context, which is a column that has stopped doing its job: number those, and only those. Counted per workspace, because two workspaces are told apart by the name the context starts with.
  const sameContext = (item: PickerItem) => `${item.workspace}\u0000${item.context}`;
  const counts = new Map<string, number>();
  for (const item of running) counts.set(sameContext(item), (counts.get(sameContext(item)) ?? 0) + 1);
  const numbered = running.map((item) => {
    if ((counts.get(sameContext(item)) ?? 0) < 2 || item.kind.type !== "running") return item;
    const tab = item.kind.tab;
    const workspace = workspaces.find((w) => w.name === item.workspace);
    const position = workspace?.tabs.find((t) => t.tab === tab)?.position;
    // The tab's own position, so the number means what it means everywhere else (Ctrl+1 to Ctrl+9).
    return position === undefined ? item : { ...item, context: `${item.context} ${position + 1}` };
  });

  // The most recent one from another workspace, brought to the front: only moved, not filtered.
  const elsewhere = numbered.findIndex((item) => item.workspace !== here?.workspace);
  if (elsewhere > 0) numbered.unshift(...numbered.splice(elsewhere, 1));

  // A folder whose workspace exists is reachable above.
  const existing = new Set(workspaces.map((w) => w.name.toLowerCase()));
  const listed = new Set<string>();
  const folders: PickerItem[] = [];
  for (const path of directories) {
    const name = baseName(path);
    if (listed.has(path) || existing.has(name.toLowerCase())) continue;
    listed.add(path);
    folders.push({
      id: `dir:${path}`,
      kind: { type: "destination", path },
      workspace: "",
      context: "",
      title: name,
      // The whole path, still, even though the row draws it as one line: it is also what the query is matched against, and a folder found by `www/acaua` yesterday should not have become findable only by `acaua`.
      detail: homeRelative(path, home),
      command: "",
      path,
      busy: false,
      lastActive: 0,
    });
  }
  return [...numbered, ...folders];
}

/** The row that offers to make the workspace a query names, when none has that name. */
export function createItem(query: string, workspaces: readonly WorkspaceFacts[]): PickerItem | null {
  const name = query.trim();
  if (!name || workspaces.some((w) => w.name === name)) return null;
  return {
    id: `create:${name}`,
    kind: { type: "create", name },
    workspace: "",
    context: "",
    title: `Criar o workspace “${name}”`,
    detail: "",
    command: "",
    path: "",
    busy: false,
    lastActive: 0,
  };
}

// ------------------------------------------------------------------ the commands

/** Every command, in the order the palette reads them, with the ones nothing is bound to marked. */
export function commandItems(): PickerItem[] {
  return COMMANDS.map((spec) => ({
    id: `cmd:${spec.id}`,
    kind: { type: "command", command: spec.id },
    workspace: "",
    context: spec.group,
    title: spec.title,
    detail: "",
    command: "",
    path: "",
    busy: false,
    lastActive: 0,
    disabled: !isEnabled(spec.id),
  }));
}

// ------------------------------------------------------------------ the history

/** A hit as a row: the line, and where it is. `place` names the tab it is in, as the person knows it. */
export function hitItem(hit: SearchHit, root: number, place: string): PickerItem {
  return {
    id: `hit:${tabKey(hit.workspace, hit.tab)}\u0000${hit.line}`,
    kind: { type: "hit", hit, root, span: matchSpan(hit.text, hit.matchStart, hit.matchLen) },
    workspace: hit.workspace,
    context: "",
    title: hit.text === "" ? " " : hit.text,
    detail: `${place}:${hit.line + 1}`,
    command: "",
    path: "",
    busy: false,
    lastActive: 0,
  };
}

// ------------------------------------------------------------------ filtering

/**
 * The rows that match, best first, and which column put each one there.
 * Three columns are tried in reading order and the first that answers is
 * remembered: the context goes first on purpose, because it is where a
 * workspace's name lives, so typing one gathers that project instead of
 * scattering its tabs behind whichever titles happened to score better.
 * Rows of equal score keep the list's own order — where you have been, latest
 * first.
 */
export function filterItems(items: readonly PickerItem[], query: string): { kept: PickerItem[]; lit: Map<string, Lit> } {
  // A folder is found by `www/acaua` or by `www\acaua`: both slashes are read as one, in the query and in the text, which keeps its length and so its ranges.
  const needle = query.trim().replace(/\\/g, "/");
  if (!needle) return { kept: [...items], lit: new Map() };
  const scored: { item: PickerItem; score: number; position: number; lit: Lit }[] = [];
  items.forEach((item, position) => {
    for (const column of COLUMNS) {
      const text = item[column].replace(/\\/g, "/");
      const score = text ? fuzzyScore(needle, text) : undefined;
      if (score === undefined) continue;
      scored.push({ item, score, position, lit: { column, ranges: matchRanges(needle, text) } });
      return;
    }
  });
  scored.sort((a, b) => a.score - b.score || a.position - b.position);
  return { kept: scored.map((s) => s.item), lit: new Map(scored.map((s) => [s.item.id, s.lit])) };
}

// ------------------------------------------------------------------ rows

/**
 * The two headings, around the two groups, and neither when its group is
 * empty — which is what makes a heading disappear as its section is typed
 * away rather than stand over nothing. The kinds are gathered rather than
 * assumed to arrive together: the score sorts the whole list at once, so one
 * letter typed can put a folder between two terminals, and a heading over rows
 * of the other kind is worse than none. Order within each group is the score's.
 */
export function sectioned(items: readonly PickerItem[]): Row[] {
  const terminals: PickerItem[] = [];
  const folders: PickerItem[] = [];
  const loose: PickerItem[] = [];
  const last: PickerItem[] = [];
  for (const item of items) {
    if (item.kind.type === "running") terminals.push(item);
    else if (item.kind.type === "destination") folders.push(item);
    else if (item.kind.type === "create") last.push(item);
    else loose.push(item);
  }
  const rows: Row[] = loose.map((item) => ({ type: "item", item }));
  if (terminals.length > 0) rows.push({ type: "heading", text: "terminais" }, ...terminals.map((item): Row => ({ type: "item", item })));
  if (folders.length > 0) rows.push({ type: "heading", text: "abrir em" }, ...folders.map((item): Row => ({ type: "item", item })));
  rows.push(...last.map((item): Row => ({ type: "item", item })));
  return rows;
}

/** Runs of one group, each under its name. The order is the list's own; this only notices where one group stops and the next begins. */
export function grouped(items: readonly PickerItem[]): Row[] {
  const rows: Row[] = [];
  let current: string | undefined;
  for (const item of items) {
    if (item.context !== current) {
      current = item.context;
      if (item.context !== "") rows.push({ type: "heading", text: item.context });
    }
    rows.push({ type: "item", item });
  }
  return rows;
}

/** Plain rows, no headings: a search's, where the daemon has already decided what matches. */
export function plainRows(items: readonly PickerItem[]): Row[] {
  return items.map((item) => ({ type: "item", item }));
}

/** A row the selection may rest on: not a heading, not a command that cannot run. */
export function selectable(row: Row): row is { type: "item"; item: PickerItem } {
  return row.type === "item" && !row.item.disabled;
}

/** The first row that can hold the selection, or -1 in an empty list. */
export function firstSelectable(rows: readonly Row[]): number {
  return rows.findIndex(selectable);
}

/**
 * The next row the selection may rest on, stepping over headings. Clamps
 * rather than wraps, and returns where it started when there is nowhere to go,
 * so holding ↓ at the bottom of the list does nothing instead of jumping back
 * to the top.
 */
export function nextSelectable(rows: readonly Row[], from: number, step: 1 | -1): number {
  for (let next = from + step; next >= 0 && next < rows.length; next += step) {
    if (selectable(rows[next])) return next;
  }
  return from >= 0 && from < rows.length && selectable(rows[from]) ? from : firstSelectable(rows);
}
