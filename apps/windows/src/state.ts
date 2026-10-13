// What the window remembers between runs (src-tauri: settings.rs keeps the
// file). The daemon owns the facts; this is only what is the window's: the
// sidebar's order, names given, folds, zoom, look. The tabs themselves come
// back after a reboot from the core (`keep restauro`, docs/restauro.md), not
// from here.

import type { Appearance } from "./theme";
import { normalizeZoom } from "./zoom";

/** The cursor's shape, as the terminal draws it. */
export type CursorShape = "bar" | "block" | "underline";

export interface GuiState {
  version: 1;
  /** Workspaces in the sidebar's order. New ones join the end. */
  order: string[];
  /** Workspaces whose tab list is folded shut. */
  folded: string[];
  /** Names given to tabs, by `${workspace}\u0000${tab}`. */
  tabNames: Record<string, string>;
  /** Names given to workspaces, shown in place of the daemon's. */
  workspaceNames: Record<string, string>;
  active: { workspace: string; tab: number } | null;
  /** The tab last shown in each workspace, to come back to it. */
  lastTab: Record<string, number>;
  /** The zoom, as a share of the base size (1 = 100%). */
  zoom: number;
  appearance: Appearance;
  sidebar: { visible: boolean; width: number };
  /** Empty family means the platform's default. */
  font: { family: string; size: number };
  /**
   * A theme of the catalogue (themes.ts), or a `dark:A,light:B` pair of them
   * as Ghostty's config spells it; empty is Keep's own, dark or light by
   * `appearance`.
   */
  theme: string;
  /** What the terminals do with the cursor and with what scrolls off. */
  terminal: { cursor: CursorShape; blink: boolean; scrollback: number };
  /**
   * The daemon the window last saw, as `api.daemonIdentity` says it: tab numbers
   * (the names given, the tab in front) are one daemon's, and a new one counts
   * from one again.
   */
  daemon: string;
}

export const SIDEBAR_MIN = 200;
export const SIDEBAR_MAX = 320;

/** Lines of history a terminal keeps unless told otherwise. */
export const DEFAULT_SCROLLBACK = 10000;
/** A cell of history costs about 12 bytes: past this one tab alone takes hundreds of megabytes of the window's memory. */
export const SCROLLBACK_MAX = 100000;

/** A number of lines held to what a terminal can keep; the default for anything else. */
export function normalizeScrollback(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return DEFAULT_SCROLLBACK;
  return Math.min(SCROLLBACK_MAX, Math.max(0, Math.round(value)));
}

export function defaultState(): GuiState {
  return {
    version: 1,
    order: [],
    folded: [],
    tabNames: {},
    workspaceNames: {},
    active: null,
    lastTab: {},
    zoom: 1,
    appearance: "auto",
    sidebar: { visible: true, width: 250 },
    font: { family: "", size: 0 },
    theme: "",
    terminal: { cursor: "bar", blink: true, scrollback: DEFAULT_SCROLLBACK },
    daemon: "",
  };
}

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const strings = (v: unknown): string[] => (Array.isArray(v) ? v.filter((s): s is string => typeof s === "string") : []);
const stringMap = (v: unknown): Record<string, string> => {
  const out: Record<string, string> = {};
  if (isRecord(v)) for (const [k, s] of Object.entries(v)) if (typeof s === "string" && s.trim()) out[k] = s;
  return out;
};

/**
 * A state file read back, whatever it holds: fields it lacks or holds wrongly
 * take their defaults, so a file written by an older version — or damaged —
 * costs at most what it no longer says, never the window.
 */
export function normalizeState(raw: unknown): GuiState {
  const base = defaultState();
  if (!isRecord(raw)) return base;
  const sidebar = isRecord(raw.sidebar) ? raw.sidebar : {};
  const font = isRecord(raw.font) ? raw.font : {};
  const terminal = isRecord(raw.terminal) ? raw.terminal : {};
  const active = isRecord(raw.active) &&
    typeof raw.active.workspace === "string" &&
    typeof raw.active.tab === "number"
    ? { workspace: raw.active.workspace, tab: raw.active.tab }
    : null;
  const lastTab: Record<string, number> = {};
  if (isRecord(raw.lastTab)) {
    for (const [k, v] of Object.entries(raw.lastTab)) if (typeof v === "number") lastTab[k] = v;
  }
  const appearance = raw.appearance === "light" || raw.appearance === "dark" ? raw.appearance : "auto";
  const width = typeof sidebar.width === "number" ? sidebar.width : base.sidebar.width;
  return {
    version: 1,
    order: strings(raw.order),
    folded: strings(raw.folded),
    tabNames: stringMap(raw.tabNames),
    workspaceNames: stringMap(raw.workspaceNames),
    active,
    lastTab,
    zoom: normalizeZoom(raw.zoom),
    appearance,
    sidebar: {
      visible: typeof sidebar.visible === "boolean" ? sidebar.visible : true,
      width: Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, width)),
    },
    font: {
      family: typeof font.family === "string" ? font.family : "",
      size: typeof font.size === "number" && font.size >= 6 && font.size <= 48 ? font.size : 0,
    },
    theme: typeof raw.theme === "string" ? raw.theme.trim() : "",
    terminal: {
      cursor: terminal.cursor === "block" || terminal.cursor === "underline" ? terminal.cursor : "bar",
      blink: typeof terminal.blink === "boolean" ? terminal.blink : true,
      scrollback: normalizeScrollback(terminal.scrollback),
    },
    daemon: typeof raw.daemon === "string" ? raw.daemon : "",
  };
}

/** The key a tab's name is kept under. */
export const tabKey = (workspace: string, tab: number) => `${workspace}\u0000${tab}`;
