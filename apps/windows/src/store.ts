// What the window knows and what it can do.
//
// The daemon owns the facts — workspaces, tabs, what runs in them — and is
// asked again every second; nothing here is a second copy of it. What is
// kept is the window's own: which tab is in front, the sidebar's order, the
// names given, how big the text is, and the state Claude Code's screens
// declare.

import { batch, computed, effect, signal } from "@preact/signals";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import * as api from "./api";
import type { TabInfo, WorkspaceInfo } from "./api";
import {
  declaredMode,
  isAiProgram,
  isAtWork,
  plainTitle,
  programOf,
  readActivity,
  readCodexActivity,
  type ClaudeActivity,
  type ClaudeMode,
} from "./claude";
import { baseName, isPathTitle, rootTabs, type RootTab } from "./layout";
import { defaultState, normalizeState, tabKey, type GuiState, type RestoreWorkspace } from "./state";
import { TerminalManager, viewKey, type TermView } from "./terminals";
import { DEFAULT_FONT_SIZE, applyPalette, defaultFontFamily, onSystemAppearanceChange, resolvePalette } from "./theme";
import { zoomStep, zoomedSize } from "./zoom";

// ------------------------------------------------------------------ state

export const info = signal<api.Startup | null>(null);
export const workspaces = signal<WorkspaceInfo[]>([]);
/** Set while the daemon does not answer; the window keeps asking. */
export const daemonError = signal<string | null>(null);
/** The daemon has answered at least once. */
export const loaded = signal(false);
export const gui = signal<GuiState>(defaultState());

/** Where each Claude Code or Codex pane's turn stands, by pane. */
export const activities = signal<ReadonlyMap<string, ClaudeActivity>>(new Map());
/** The mode each Claude Code pane declares, by pane; `null` is manual. */
export const modes = signal<ReadonlyMap<string, ClaudeMode | null>>(new Map());
/** When each pane began waiting on you, in seconds: what its badge breathes from. */
export const waitingSince = signal<ReadonlyMap<string, number>>(new Map());

export type Overlay = "picker" | "search" | "settings" | null;
export const overlay = signal<Overlay>(null);
export const findOpen = signal(false);
export const editing = signal<{ kind: "tab"; workspace: string; tab: number } | { kind: "workspace"; workspace: string } | null>(
  null,
);
/** A line said quietly at the foot of the sidebar, for a while. */
export const notice = signal<string | null>(null);
/** The pane holding the keyboard, per tab. */
export const focusedPane = signal<Record<string, number>>({});
/** A clock for things that change with time: badges breathing, "há 5 min". */
export const now = signal(Date.now());
export const maximized = signal(false);

export interface DialogButton {
  label: string;
  value: string;
  danger?: boolean;
  primary?: boolean;
}
export interface DialogField {
  name: string;
  label: string;
  value: string;
  placeholder?: string;
  /** A button beside the field that picks a folder into it. */
  folder?: boolean;
}
export interface Dialog {
  title: string;
  message?: string;
  fields?: DialogField[];
  buttons: DialogButton[];
  resolve: (result: { button: string; values: Record<string, string> } | null) => void;
}
export const dialog = signal<Dialog | null>(null);

export function ask(spec: Omit<Dialog, "resolve">): Promise<{ button: string; values: Record<string, string> } | null> {
  return new Promise((resolve) => {
    dialog.value = { ...spec, resolve };
  });
}

export function answer(button: string | null, values: Record<string, string> = {}): void {
  const current = dialog.value;
  if (!current) return;
  dialog.value = null;
  current.resolve(button === null ? null : { button, values });
}

// ------------------------------------------------------------------ derived

export const isMac = computed(() => info.value?.platform === "macos");
export const isWindows = computed(() => info.value?.platform === "windows");

export const palette = computed(() => {
  // Read so the computed follows the system while automatic.
  void systemTheme.value;
  return resolvePalette(gui.value.appearance);
});
const systemTheme = signal(0);

/** The daemon's workspaces in the sidebar's order; ones it has not seen join the end. */
export const orderedWorkspaces = computed(() => {
  const list = workspaces.value;
  const order = gui.value.order;
  const rank = new Map(order.map((name, i) => [name, i]));
  return [...list].sort((a, b) => (rank.get(a.name) ?? Infinity) - (rank.get(b.name) ?? Infinity));
});

export const tabsByWorkspace = computed(() => {
  const out = new Map<string, RootTab[]>();
  for (const w of workspaces.value) out.set(w.name, rootTabs(w.tabs.filter((t) => !t.finished)));
  return out;
});

export const activeWorkspace = computed<string | null>(() => {
  const names = orderedWorkspaces.value.map((w) => w.name);
  const wanted = gui.value.active?.workspace;
  if (wanted && names.includes(wanted)) return wanted;
  return names[0] ?? null;
});

export const activeTab = computed<RootTab | null>(() => {
  const ws = activeWorkspace.value;
  if (!ws) return null;
  const tabs = tabsByWorkspace.value.get(ws) ?? [];
  const wanted = gui.value.active?.workspace === ws ? gui.value.active.tab : gui.value.lastTab[ws];
  return tabs.find((t) => t.root.id === wanted) ?? tabs[0] ?? null;
});

/** The pane of the tab in front that holds the keyboard. */
export const activePane = computed<TabInfo | null>(() => {
  const tab = activeTab.value;
  const ws = activeWorkspace.value;
  if (!tab || !ws) return null;
  const wanted = focusedPane.value[tabKey(ws, tab.root.id)];
  return tab.panes.find((p) => p.id === wanted) ?? tab.root;
});

export const fontFamily = computed(() => gui.value.font.family || defaultFontFamily(info.value?.platform ?? ""));
export const baseFontSize = computed(() => gui.value.font.size || DEFAULT_FONT_SIZE);
export const fontSize = computed(() => zoomedSize(baseFontSize.value, gui.value.zoom));

// ------------------------------------------------------------------ names

/** What a tab is called: the name given it, else what its program says it is. */
export function tabLabel(workspace: string, tab: RootTab, index: number): string {
  const given = gui.value.tabNames[tabKey(workspace, tab.root.id)];
  if (given) return given;
  const pane = tab.root;
  const title = pane.title.trim();
  if (title && !isPathTitle(title)) return plainTitle(title, `aba ${index + 1}`);
  if (pane.command) return pane.command;
  if (title) return baseName(title).replace(/\.exe$/i, "");
  return `aba ${index + 1}`;
}

export function workspaceLabel(name: string): string {
  return gui.value.workspaceNames[name] || name;
}

const URGENCY: Record<ClaudeActivity, number> = { waitingForYou: 4, working: 3, waitingForWorkflow: 2, done: 1 };

/** Where a tab's turn stands: the most pressing of its panes'. */
export function tabActivity(workspace: string, tab: RootTab): ClaudeActivity | undefined {
  let best: ClaudeActivity | undefined;
  for (const pane of tab.panes) {
    const a = activities.value.get(viewKey(workspace, pane.id));
    if (a && (!best || URGENCY[a] > URGENCY[best])) best = a;
  }
  return best;
}

export function tabMode(workspace: string, tab: RootTab): ClaudeMode | null | undefined {
  for (const pane of tab.panes) {
    const m = modes.value.get(viewKey(workspace, pane.id));
    if (m !== undefined) return m;
  }
  return undefined;
}

export function tabBusy(workspace: string, tab: RootTab): boolean {
  const activity = tabActivity(workspace, tab);
  return isAtWork(activity, tab.panes.some((p) => p.busy));
}

/** Since when a tab has waited on you, in seconds, if it does. */
export function tabWaitingSince(workspace: string, tab: RootTab): number | undefined {
  let since: number | undefined;
  for (const pane of tab.panes) {
    const s = waitingSince.value.get(viewKey(workspace, pane.id));
    if (s !== undefined && (since === undefined || s < since)) since = s;
  }
  return since;
}

// ------------------------------------------------------------------ terminals

export let terminals: TerminalManager;

function look() {
  return { fontFamily: fontFamily.value, fontSize: fontSize.value, theme: palette.value.terminal };
}

// ------------------------------------------------------------------ persistence

let saveTimer: ReturnType<typeof setTimeout> | undefined;
let stateLoaded = false;

function update(change: (state: GuiState) => void): void {
  const next = structuredClone(gui.value);
  change(next);
  gui.value = next;
}

function scheduleSave(): void {
  if (!stateLoaded) return;
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => void api.saveState(gui.value).catch(() => {}), 500);
}

// ------------------------------------------------------------------ the daemon

let refreshing = false;

/** Ask the daemon for its workspaces, and make the window agree. */
export async function refresh(): Promise<void> {
  if (refreshing) return;
  refreshing = true;
  try {
    const list = await api.list();
    const firstAnswer = daemonError.value !== null;
    batch(() => {
      workspaces.value = list;
      daemonError.value = null;
      loaded.value = true;
      reconcile(list);
    });
    if (firstAnswer) terminals.reattach();
  } catch (error) {
    daemonError.value = String(error);
    // Started again if it went away: the first launch after it was killed,
    // say. Its tabs are gone with it, which the restore picture is for.
    void api.ensureDaemon().catch(() => {});
  } finally {
    refreshing = false;
  }
}

/** Bring the window's own state in line with what the daemon says exists. */
function reconcile(list: WorkspaceInfo[]): void {
  const names = new Set(list.map((w) => w.name));
  const alive = new Set<string>();
  for (const w of list) for (const t of w.tabs) if (!t.finished) alive.add(viewKey(w.name, t.id));
  terminals.prune(alive);
  // Until the window has decided whether to restore, an empty daemon is not
  // news: what it says must not wipe the picture that restoring needs.
  if (!settled) return;

  const state = gui.value;
  const order = state.order.filter((n) => names.has(n));
  for (const w of list) if (!order.includes(w.name)) order.push(w.name);
  const tabNames = Object.fromEntries(Object.entries(state.tabNames).filter(([k]) => alive.has(k)));
  const workspaceNames = Object.fromEntries(Object.entries(state.workspaceNames).filter(([k]) => names.has(k)));
  const folded = state.folded.filter((n) => names.has(n));
  const lastTab = Object.fromEntries(Object.entries(state.lastTab).filter(([k]) => names.has(k)));
  const restore = picture(list, tabNames);

  // The tab in front went away: the one beside it comes forward.
  let active = state.active;
  const tabsOf = (ws: string) => rootTabs((list.find((w) => w.name === ws)?.tabs ?? []).filter((t) => !t.finished));
  if (active) {
    const tabs = tabsOf(active.workspace);
    if (!tabs.some((t) => t.root.id === active!.tab)) {
      const previous = tabsByWorkspaceSnapshot.get(active.workspace) ?? [];
      const at = previous.findIndex((t) => t === active!.tab);
      const neighbour = tabs[Math.min(Math.max(at, 0), tabs.length - 1)] ?? tabs[tabs.length - 1];
      if (neighbour) active = { workspace: active.workspace, tab: neighbour.root.id };
      else {
        const other = order.find((n) => tabsOf(n).length > 0);
        active = other ? { workspace: other, tab: lastTab[other] ?? tabsOf(other)[0].root.id } : null;
      }
    }
  }
  tabsByWorkspaceSnapshot = new Map(list.map((w) => [w.name, tabsOf(w.name).map((t) => t.root.id)]));

  const changed =
    JSON.stringify([order, tabNames, workspaceNames, folded, lastTab, restore, active]) !==
    JSON.stringify([state.order, state.tabNames, state.workspaceNames, state.folded, state.lastTab, state.restore, state.active]);
  if (changed) {
    gui.value = { ...state, order, tabNames, workspaceNames, folded, lastTab, restore, active };
  }
}

let tabsByWorkspaceSnapshot = new Map<string, number[]>();

/** The window has decided whether to restore; from here on the daemon's list rules. */
let settled = false;

/** The workspaces as they stand, to open again after a reboot. */
function picture(list: WorkspaceInfo[], names: Record<string, string>): RestoreWorkspace[] {
  return list
    .map((w) => ({
      name: w.name,
      tabs: rootTabs(w.tabs.filter((t) => !t.finished)).flatMap((tab) =>
        tab.panes.map((p) => ({
          id: p.id,
          cwd: p.cwd,
          name: names[tabKey(w.name, p.id)],
          splitOf: p.splitOf,
          splitDir: p.splitDir,
        })),
      ),
    }))
    .filter((w) => w.tabs.length > 0);
}

/**
 * Open the workspaces of the last picture again, in the same folders and
 * under the same names. Only on a daemon this window just started: one that
 * was already running and is empty was emptied on purpose.
 */
async function restoreFrom(pictureToRestore: RestoreWorkspace[]): Promise<boolean> {
  if (pictureToRestore.length === 0) return false;
  const { cols, rows } = guessSize();
  const names: Record<string, string> = {};
  let restored = 0;
  for (const w of pictureToRestore) {
    // Parents before their panes: the daemon refuses a pane of nothing.
    const ids = new Map<number, number>();
    const pending = [...w.tabs];
    for (let guard = 0; pending.length > 0 && guard < w.tabs.length + 1; guard++) {
      for (let i = 0; i < pending.length; i++) {
        const t = pending[i];
        const parent = t.splitOf ? ids.get(t.splitOf) : 0;
        if (t.splitOf && parent === undefined) continue;
        try {
          const id = await api.newTab(w.name, t.cwd || null, cols, rows, parent ?? 0, t.splitOf ? t.splitDir : 0);
          ids.set(t.id, id);
          if (t.name) names[tabKey(w.name, id)] = t.name;
          restored++;
        } catch {
          // A folder that no longer exists: the daemon refuses it. Opened at
          // home instead rather than lost.
          try {
            const id = await api.newTab(w.name, null, cols, rows, parent ?? 0, t.splitOf ? t.splitDir : 0);
            ids.set(t.id, id);
            restored++;
          } catch {
            /* nothing to be done for this one */
          }
        }
        pending.splice(i, 1);
        i--;
      }
    }
  }
  update((s) => {
    s.tabNames = { ...s.tabNames, ...names };
  });
  return restored > 0;
}

/** A size for a tab that is not on screen yet: the tab in front's, or a guess from the font. */
function guessSize(): { cols: number; rows: number } {
  const pane = activePane.value;
  const ws = activeWorkspace.value;
  if (pane && ws) {
    const view = terminals.existing(ws, pane.id);
    if (view && view.term.cols > 2) return { cols: view.term.cols, rows: view.term.rows };
  }
  const area = document.querySelector(".terminal-area");
  const width = area?.clientWidth || 1000;
  const height = area?.clientHeight || 600;
  return {
    cols: Math.max(20, Math.floor(width / (fontSize.value * 0.6))),
    rows: Math.max(5, Math.floor(height / (fontSize.value * 1.2))),
  };
}

// ------------------------------------------------------------------ reading Claude Code

/** Read the screens of the panes running Claude Code or Codex. */
function readScreens(): void {
  const nextActivities = new Map<string, ClaudeActivity>();
  const nextModes = new Map<string, ClaudeMode | null>();
  const previousActivities = activities.value;
  const previousModes = modes.value;
  for (const w of workspaces.value) {
    for (const pane of w.tabs) {
      if (pane.finished) continue;
      const program = programOf(pane.command, pane.title);
      if (!isAiProgram(program)) continue;
      const key = viewKey(w.name, pane.id);
      const view = terminals.existing(w.name, pane.id);
      const text = view?.screenText() ?? "";
      if (program === "codex") {
        const a = text ? readCodexActivity(text) : undefined;
        const kept = a ?? previousActivities.get(key);
        if (kept) nextActivities.set(key, kept);
        continue;
      }
      // Undefined is "cannot tell": what was known stands.
      const m = text ? declaredMode(text) : undefined;
      if (m !== undefined) nextModes.set(key, m);
      else if (previousModes.has(key)) nextModes.set(key, previousModes.get(key)!);
      const a = text ? readActivity(text) : undefined;
      const kept = a ?? previousActivities.get(key);
      if (kept) nextActivities.set(key, kept);
    }
  }
  const since = new Map<string, number>();
  const seconds = Date.now() / 1000;
  for (const [key, activity] of nextActivities) {
    if (activity !== "waitingForYou") continue;
    since.set(key, waitingSince.value.get(key) ?? seconds);
  }
  const same = <V,>(a: ReadonlyMap<string, V>, b: ReadonlyMap<string, V>) =>
    a.size === b.size && [...a].every(([k, v]) => b.get(k) === v);
  batch(() => {
    if (!same(nextActivities, previousActivities)) activities.value = nextActivities;
    if (!same(nextModes, previousModes)) modes.value = nextModes;
    if (!same(since, waitingSince.value)) waitingSince.value = since;
  });
}

// ------------------------------------------------------------------ actions

export function select(workspace: string, tab?: number): void {
  const tabs = tabsByWorkspace.value.get(workspace) ?? [];
  const id = tab ?? gui.value.lastTab[workspace] ?? tabs[0]?.root.id;
  update((s) => {
    if (id === undefined) {
      s.active = { workspace, tab: -1 };
      return;
    }
    s.active = { workspace, tab: id };
    s.lastTab[workspace] = id;
  });
  requestAnimationFrame(() => focusActive());
}

export function focusActive(): void {
  const ws = activeWorkspace.value;
  const pane = activePane.value;
  if (!ws || !pane) return;
  terminals.existing(ws, pane.id)?.focus();
}

export function focusPane(workspace: string, root: number, pane: number): void {
  const key = tabKey(workspace, root);
  if (focusedPane.value[key] === pane) return;
  focusedPane.value = { ...focusedPane.value, [key]: pane };
}

/** The tabs of the workspace in front, for moving through them. */
function activeTabs(): RootTab[] {
  const ws = activeWorkspace.value;
  return ws ? tabsByWorkspace.value.get(ws) ?? [] : [];
}

export function stepTab(direction: number): void {
  const tabs = activeTabs();
  const current = activeTab.value;
  const ws = activeWorkspace.value;
  if (!ws || tabs.length === 0) return;
  const at = Math.max(0, tabs.findIndex((t) => t === current));
  const next = tabs[(at + direction + tabs.length) % tabs.length];
  select(ws, next.root.id);
}

export function goToTab(index: number | "last"): void {
  const tabs = activeTabs();
  const ws = activeWorkspace.value;
  if (!ws || tabs.length === 0) return;
  const tab = index === "last" ? tabs[tabs.length - 1] : tabs[index];
  if (tab) select(ws, tab.root.id);
}

export async function newTab(workspace = activeWorkspace.value ?? undefined): Promise<number | null> {
  if (!workspace) {
    await newWorkspace();
    return null;
  }
  const { cols, rows } = guessSize();
  const cwd = activePane.value?.cwd || workspaceCwd(workspace) || info.value?.home || null;
  const id = await api.newTab(workspace, cwd, cols, rows);
  await refresh();
  select(workspace, id);
  return id;
}

function workspaceCwd(workspace: string): string {
  const w = workspaces.value.find((x) => x.name === workspace);
  return w?.tabs.find((t) => t.cwd)?.cwd ?? "";
}

/** Split the pane in front: right or down. */
export async function split(direction: number): Promise<number | null> {
  const ws = activeWorkspace.value;
  const tab = activeTab.value;
  const pane = activePane.value;
  if (!ws || !tab || !pane) return null;
  const view = terminals.existing(ws, pane.id);
  const cols = view ? Math.max(10, Math.floor(view.term.cols / (direction === api.SPLIT_RIGHT ? 2 : 1))) : 80;
  const rows = view ? Math.max(4, Math.floor(view.term.rows / (direction === api.SPLIT_DOWN ? 2 : 1))) : 24;
  const id = await api.newTab(ws, pane.cwd || null, cols, rows, pane.id, direction);
  await refresh();
  focusPane(ws, tab.root.id, id);
  requestAnimationFrame(() => focusActive());
  return id;
}

/** Create a workspace with one tab, or go to it if one of that name exists. */
export async function createWorkspace(name: string, cwd: string | null): Promise<void> {
  const clean = name.trim();
  if (!clean) return;
  if (workspaces.value.some((w) => w.name === clean)) {
    select(clean);
    return;
  }
  const { cols, rows } = guessSize();
  const id = await api.newTab(clean, cwd || info.value?.home || null, cols, rows);
  update((s) => {
    if (!s.order.includes(clean)) s.order.push(clean);
  });
  await refresh();
  select(clean, id);
}

export async function newWorkspace(): Promise<void> {
  const result = await ask({
    title: "Novo workspace",
    fields: [
      { name: "name", label: "Nome", value: "", placeholder: "projeto" },
      { name: "cwd", label: "Pasta", value: "", placeholder: "pasta inicial (opcional)", folder: true },
    ],
    buttons: [
      { label: "Cancelar", value: "cancel" },
      { label: "Criar", value: "create", primary: true },
    ],
  });
  if (!result || result.button !== "create") return;
  const cwd = result.values.cwd?.trim() || "";
  const name = result.values.name?.trim() || (cwd ? baseName(cwd) : "");
  if (!name) return;
  await createWorkspace(name, cwd || null);
}

/** Close a whole tab — its panes with it — after asking. */
export async function closeTab(workspace: string, tab: RootTab, label: string): Promise<boolean> {
  const result = await ask({
    title: `Fechar a aba “${label}”?`,
    message: "O que estiver rodando nela será encerrado.",
    buttons: [
      { label: "Cancelar", value: "cancel" },
      { label: "Fechar a aba", value: "close", danger: true, primary: true },
    ],
  });
  if (result?.button !== "close") return false;
  // The panes first: closing the root alone would hand them a tab of their own.
  for (const pane of [...tab.panes].reverse()) {
    await api.closeTab(workspace, pane.id).catch(() => {});
  }
  await refresh();
  return true;
}

export async function closePane(workspace: string, pane: number): Promise<boolean> {
  const result = await ask({
    title: "Fechar este painel?",
    message: "O que estiver rodando nele será encerrado.",
    buttons: [
      { label: "Cancelar", value: "cancel" },
      { label: "Fechar o painel", value: "close", danger: true, primary: true },
    ],
  });
  if (result?.button !== "close") return false;
  await api.closeTab(workspace, pane).catch(() => {});
  await refresh();
  return true;
}

export async function closeWorkspace(workspace: string): Promise<boolean> {
  const label = workspaceLabel(workspace);
  const count = tabsByWorkspace.value.get(workspace)?.length ?? 0;
  const result = await ask({
    title: `Fechar o workspace “${label}”?`,
    message:
      count === 1
        ? "A aba dele e o que estiver rodando nela serão encerrados."
        : `As ${count} abas dele e o que estiver rodando nelas serão encerrados.`,
    buttons: [
      { label: "Cancelar", value: "cancel" },
      { label: "Fechar o workspace", value: "close", danger: true, primary: true },
    ],
  });
  if (result?.button !== "close") return false;
  await api.killWorkspace(workspace).catch(() => {});
  await refresh();
  return true;
}

export function renameTab(workspace: string, tab: number, name: string): void {
  update((s) => {
    const key = tabKey(workspace, tab);
    if (name.trim()) s.tabNames[key] = name.trim();
    else delete s.tabNames[key];
  });
}

export function renameWorkspace(workspace: string, name: string): void {
  update((s) => {
    if (name.trim() && name.trim() !== workspace) s.workspaceNames[workspace] = name.trim();
    else delete s.workspaceNames[workspace];
  });
}

export function toggleFolded(workspace: string): void {
  update((s) => {
    s.folded = s.folded.includes(workspace) ? s.folded.filter((n) => n !== workspace) : [...s.folded, workspace];
  });
}

export function moveWorkspace(workspace: string, before: string | null): void {
  update((s) => {
    const order = s.order.filter((n) => n !== workspace);
    const at = before ? order.indexOf(before) : -1;
    if (at < 0) order.push(workspace);
    else order.splice(at, 0, workspace);
    s.order = order;
  });
}

export function zoom(direction: number): void {
  const next = zoomStep(gui.value.zoom, direction);
  if (next !== undefined) update((s) => void (s.zoom = next));
}

export function zoomReset(): void {
  update((s) => void (s.zoom = 1));
}

export function setAppearance(appearance: GuiState["appearance"]): void {
  update((s) => void (s.appearance = appearance));
}

export function setFont(family: string, size: number): void {
  update((s) => {
    s.font = { family: family.trim(), size: size >= 6 && size <= 48 ? size : 0 };
  });
}

export function toggleSidebar(): void {
  update((s) => void (s.sidebar.visible = !s.sidebar.visible));
}

export function setSidebarWidth(width: number): void {
  update((s) => void (s.sidebar.width = width));
}

export function say(message: string, seconds = 12): void {
  notice.value = message;
  setTimeout(() => {
    if (notice.value === message) notice.value = null;
  }, seconds * 1000);
}

/** Copy the selection of the pane in front. */
export async function copySelection(): Promise<void> {
  const ws = activeWorkspace.value;
  const pane = activePane.value;
  if (!ws || !pane) return;
  const view = terminals.existing(ws, pane.id);
  const text = view?.term.getSelection() ?? "";
  if (text) await writeText(text).catch(() => navigator.clipboard?.writeText(text));
}

export function activeView(): TermView | undefined {
  const ws = activeWorkspace.value;
  const pane = activePane.value;
  return ws && pane ? terminals.existing(ws, pane.id) : undefined;
}

// ------------------------------------------------------------------ start

export interface StartOptions {
  onKey: (view: TermView, event: KeyboardEvent) => boolean;
}

/** Everything the window needs before it draws: what platform, what state, what look. */
export async function start(options: StartOptions): Promise<void> {
  const startup = await api.startup();
  info.value = startup;
  const raw = await api.loadState().catch(() => null);
  gui.value = normalizeState(raw);
  stateLoaded = true;

  applyPalette(palette.value);
  onSystemAppearanceChange(() => (systemTheme.value += 1));

  terminals = new TerminalManager(
    look(),
    {
      ended: () => void refresh(),
      // A tab that went away between two lists also fails to attach; only a
      // connection that fails says the daemon is gone.
      failed: (_view, reason) => {
        if (/conectar|conex[aã]o|connect|pipe|socket/i.test(reason)) daemonError.value ??= reason;
      },
      key: options.onKey,
      focused: (view) => {
        const tab = (tabsByWorkspace.value.get(view.workspace) ?? []).find((t) => t.panes.some((p) => p.id === view.tab));
        if (tab) focusPane(view.workspace, tab.root.id, view.tab);
      },
    },
    { name: startup.platform, windowsBuild: startup.windowsBuild },
  );

  // Look, saving and the clock follow the state from here on.
  effect(() => applyPalette(palette.value));
  effect(() => terminals.setLook(look()));
  // The tabs' titles grow with the zoom, as the terminals' text does.
  effect(() => document.documentElement.style.setProperty("--zoom", String(gui.value.zoom)));
  effect(() => {
    void gui.value;
    scheduleSave();
  });
  setInterval(() => (now.value = Date.now()), 1000);

  const win = getCurrentWindow();
  void win.isMaximized().then((m) => (maximized.value = m)).catch(() => {});
  void win.onResized(() => void win.isMaximized().then((m) => (maximized.value = m)).catch(() => {}));
}

/** Find the daemon — starting it if needed — and keep asking it. */
export async function connect(): Promise<void> {
  // Was the daemon running before this window asked? A daemon this window
  // starts is a fresh one — after a reboot, typically — and the picture of
  // the last session is opened in it. One that was running and is empty was
  // emptied on purpose.
  const wasRunning = await api.list().then(
    () => true,
    () => false,
  );
  try {
    await api.ensureDaemon();
  } catch (error) {
    daemonError.value = String(error);
  }
  // Taken before the daemon is first heard: its list replaces the picture.
  const lastSession = gui.value.restore;
  // A daemon other than the one the picture was taken from is a fresh one
  // too, whoever started it — `keep ls` in a terminal after a reboot, say.
  const identity = await api.daemonIdentity().catch(() => null);
  const anotherDaemon = identity !== null && gui.value.daemon !== "" && identity !== gui.value.daemon;
  await refresh();
  if ((!wasRunning || anotherDaemon) && loaded.value && workspaces.value.length === 0 && lastSession.length > 0) {
    const restored = await restoreFrom(lastSession);
    if (restored) say("Restaurado após reinício");
  }
  if (identity !== null && identity !== gui.value.daemon) gui.value = { ...gui.value, daemon: identity };
  settled = true;
  await refresh();
  const active = activeWorkspace.value;
  if (active) select(active, activeTab.value?.root.id);

  setInterval(() => void refresh(), 1000);
  setInterval(readScreens, 1000);
}
