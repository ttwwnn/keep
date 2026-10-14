// What several windows remember, and how they keep it in step.
//
// The state a window works with is one `GuiState` (state.ts), as it always
// was. Kept on disk and shared it is two things, as the macOS app keeps them:
//
//  - the window's own — the sidebar's order, what is folded, the vertical
//    tabs, the sidebar's width, the tab in front: the macOS sidebar-state.json
//    and windows.json, "furniture stays where you put it, in the room you put
//    it in";
//  - the app's — the names given, the order of the tabs, the last tab of each
//    workspace, the workspaces put away, the daemon, the zoom and the look:
//    the same for every window, one copy, which a window changes by saying
//    what it changed (`Op`) and the others hear.
//
// Pure functions; windowSync.ts sends and hears the ops, and src-tauri
// (windows.rs) keeps the copy and writes it to state.json.

import { normalizeState, type GuiState } from "./state";

/** How each field the windows share is changed: a value replaced, a map changed by key, a set changed by member. */
const KINDS = {
  tabNames: "map",
  workspaceNames: "map",
  tabOrder: "map",
  lastTab: "map",
  hidden: "set",
  daemon: "value",
  zoom: "value",
  appearance: "value",
  font: "value",
  theme: "value",
  terminal: "value",
} as const;

export type SharedField = keyof typeof KINDS;

/** The fields every window shares. */
export const SHARED_FIELDS = Object.keys(KINDS) as SharedField[];

/** The fields each window keeps for itself. */
export const WINDOW_FIELDS = ["order", "folded", "verticalTabs", "sidebar", "active"] as const;

export type Shared = Pick<GuiState, SharedField>;
export type WindowPart = Pick<GuiState, (typeof WINDOW_FIELDS)[number]>;

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

export function sharedOf(state: GuiState): Shared {
  return {
    tabNames: state.tabNames,
    workspaceNames: state.workspaceNames,
    tabOrder: state.tabOrder,
    lastTab: state.lastTab,
    hidden: state.hidden,
    daemon: state.daemon,
    zoom: state.zoom,
    appearance: state.appearance,
    font: state.font,
    theme: state.theme,
    terminal: state.terminal,
  };
}

export function windowOf(state: GuiState): WindowPart {
  return {
    order: state.order,
    folded: state.folded,
    verticalTabs: state.verticalTabs,
    sidebar: state.sidebar,
    active: state.active,
  };
}

/**
 * What a window opened now starts from: no workspaces — "a new window has
 * none: every workspace is a Go To away, and the ones you go to are the ones
 * that stay" — and the furniture of the window it inherits from (the first
 * one's), "so a new window opens looking like the one it was opened from".
 */
export function blankWindow(inherit?: unknown): WindowPart {
  const from = normalizeState(isRecord(inherit) ? inherit : null);
  return { order: [], folded: from.folded, verticalTabs: from.verticalTabs, sidebar: from.sidebar, active: null };
}

/**
 * The state a window works with, from the app's part and its own (or, for a
 * window with none yet, a blank one). Whatever either holds is made sound, as
 * a state file is when it is read.
 */
export function joinState(shared: unknown, own: unknown, inherit?: unknown): GuiState {
  const window = isRecord(own) ? own : blankWindow(inherit);
  return normalizeState({ ...(isRecord(shared) ? shared : {}), ...window });
}

// ------------------------------------------------------------------ saying what changed

/** One change to what the windows share, as src-tauri applies it. */
export type Op =
  | { op: "put"; field: string; value: unknown }
  | { op: "set"; field: string; key: string; value: unknown }
  | { op: "del"; field: string; key: string }
  | { op: "add"; field: string; item: string }
  | { op: "remove"; field: string; item: string };

/** The changes that make `prev` into `next`. */
export function diffShared(prev: Shared, next: Shared): Op[] {
  const ops: Op[] = [];
  for (const field of SHARED_FIELDS) {
    const before: unknown = prev[field];
    const after: unknown = next[field];
    if (same(before, after)) continue;
    switch (KINDS[field]) {
      case "value":
        ops.push({ op: "put", field, value: after });
        break;
      case "map": {
        const was = before as Record<string, unknown>;
        const now = after as Record<string, unknown>;
        for (const key of Object.keys(now)) if (!(key in was) || !same(was[key], now[key])) ops.push({ op: "set", field, key, value: now[key] });
        for (const key of Object.keys(was)) if (!(key in now)) ops.push({ op: "del", field, key });
        break;
      }
      case "set": {
        const was = before as readonly string[];
        const now = after as readonly string[];
        for (const item of now) if (!was.includes(item)) ops.push({ op: "add", field, item });
        for (const item of was) if (!now.includes(item)) ops.push({ op: "remove", field, item });
        break;
      }
    }
  }
  return ops;
}

function isField(name: string): name is SharedField {
  return Object.hasOwn(KINDS, name);
}

/** The shared state with these changes made; the same object when they change nothing. */
export function applyOps(shared: Shared, ops: readonly Op[]): Shared {
  let next: Record<string, unknown> = shared;
  let copied = false;
  const edit = (field: SharedField, value: unknown) => {
    if (!copied) {
      next = { ...next };
      copied = true;
    }
    next[field] = value;
  };
  for (const op of ops) {
    if (!isField(op.field)) continue;
    const field = op.field;
    const kind = KINDS[field];
    const current = next[field];
    if (op.op === "put" && kind === "value") {
      if (!same(current, op.value)) edit(field, op.value);
    } else if (op.op === "set" && kind === "map") {
      const map = current as Record<string, unknown>;
      if (!(op.key in map) || !same(map[op.key], op.value)) edit(field, { ...map, [op.key]: op.value });
    } else if (op.op === "del" && kind === "map") {
      const map = current as Record<string, unknown>;
      if (op.key in map) {
        const { [op.key]: _gone, ...rest } = map;
        edit(field, rest);
      }
    } else if (op.op === "add" && kind === "set") {
      const set = current as readonly string[];
      if (!set.includes(op.item)) edit(field, [...set, op.item]);
    } else if (op.op === "remove" && kind === "set") {
      const set = current as readonly string[];
      if (set.includes(op.item)) edit(field, set.filter((item) => item !== op.item));
    }
  }
  return next as Shared;
}

/** Changes as src-tauri sent them, whatever arrives: the ones that make no sense are left out. */
export function readOps(raw: unknown): Op[] {
  if (!Array.isArray(raw)) return [];
  const ops: Op[] = [];
  for (const entry of raw) {
    if (!isRecord(entry) || typeof entry.field !== "string" || !isField(entry.field)) continue;
    const field = entry.field;
    switch (entry.op) {
      case "put":
        if (KINDS[field] === "value") ops.push({ op: "put", field, value: entry.value });
        break;
      case "set":
        if (KINDS[field] === "map" && typeof entry.key === "string") ops.push({ op: "set", field, key: entry.key, value: entry.value });
        break;
      case "del":
        if (KINDS[field] === "map" && typeof entry.key === "string") ops.push({ op: "del", field, key: entry.key });
        break;
      case "add":
        if (KINDS[field] === "set" && typeof entry.item === "string") ops.push({ op: "add", field, item: entry.item });
        break;
      case "remove":
        if (KINDS[field] === "set" && typeof entry.item === "string") ops.push({ op: "remove", field, item: entry.item });
        break;
    }
  }
  return ops;
}

/** The state of a window with the app's part replaced by `shared`, made sound. */
export function withShared(state: GuiState, shared: Shared): GuiState {
  return normalizeState({ ...state, ...shared });
}
