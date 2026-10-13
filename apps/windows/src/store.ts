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
import {
  claudeStatus,
  codexStatus,
  ENTER_DELAY_MS,
  parseCodexModels,
  parseStatusFile,
  sameStatus,
  type AiStatus,
  type Choice,
  type CodexModel,
} from "./aiStatus";
import { defaultsOf, planStatusLine, restoreDefaults } from "./claudeSettings";
import { fontPreview, loadSymbolsFont, terminalFontStack } from "./fonts";
import { nameToKeep } from "./ia";
import { sameDaemon } from "./names";
import { noTabNumbers, remapTabNumbers, type Restored, type TabNumbers } from "./restauro";
import { chooseClaudeEffort, chooseClaudeModel, chooseCodex, type PickerIO } from "./pickers";
import { baseName, isPathTitle, rootTabs, type RootTab } from "./layout";
import { noteVisit } from "./recent";
import { defaultState, tabKey, type GuiState } from "./state";
import { refile } from "./hibernation";
import { keepShares, moveShares } from "./splits";
import { listed, neighbour, stillRunning } from "./putAway";
import { arrangeTabs, heirsOf, insertBefore, memberIds, pruneOrders, substitute } from "./tabOrder";
import { TerminalManager, viewKey, type TermView } from "./terminals";
import { DEFAULT_FONT_SIZE, applyPalette, onSystemAppearanceChange, resolvePalette } from "./theme";
import { loadThemes, themePreview } from "./themes";
import {
  listingPids,
  readListing,
  targetArgument,
  trashProblems,
  worktreeNote,
  type WorktreeAnswer,
  type WorktreeTarget,
} from "./worktrees";
import { adopt, carriedByAll, carriedOf, isQuickWorkspace, orphansOf, readPeers, remove } from "./windowModel";
import { isLeader, peers, windowLabel } from "./windowPeers";
import { joinState } from "./windowState";
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
/** What the bar under each Claude Code or Codex pane shows, by pane; a pane with none has no bar. */
export const statuses = signal<ReadonlyMap<string, AiStatus>>(new Map());
/** The models Codex offers, from its cache: what the bar's Codex menus list. */
export const codexModels = signal<readonly CodexModel[]>([]);
/** The panes whose program's picker the bar is walking just now: their menus offer nothing until it is done. */
export const picking = signal<ReadonlySet<string>>(new Set());
/** When each pane began waiting on you, in seconds: what its badge breathes from. */
export const waitingSince = signal<ReadonlyMap<string, number>>(new Map());

/**
 * What floats over the terminal: Ir para, the command palette, the search of
 * the pane in front (find) and of everything (search) — one card, four
 * questions — the settings, and the lists of themes and faces they open.
 */
export type Overlay = "picker" | "palette" | "find" | "search" | "settings" | "themes" | "fonts" | null;
export const overlay = signal<Overlay>(null);
export const findOpen = signal(false);
/** Where a tab's name is typed: over its title in the row, or in the sidebar. */
export type NameSurface = "strip" | "sidebar";
/**
 * The name being typed. A tab's is typed in one place only — two fields for
 * one name take the keyboard from each other —: the surface asked for, else
 * the row above the terminal, else the sidebar (see `defaultNameSurface`).
 */
export const editing = signal<
  { kind: "tab"; workspace: string; tab: number; surface?: NameSurface } | { kind: "workspace"; workspace: string } | null
>(null);
/** A line said quietly at the foot of the sidebar, for a while. */
export const notice = signal<string | null>(null);
/** The pane holding the keyboard, per tab. */
export const focusedPane = signal<Record<string, number>>({});
/** A clock for things that change with time: badges breathing, "há 5 min". */
export const now = signal(Date.now());
export const maximized = signal(false);
/** The core's word on the AI layer: there to ask, and from which home (see iaStore.ts). */
export const iaInfo = signal<api.IaInfo | null>(null);
/** Worktrees on their way to the Recycle Bin. */
export const trashing = signal(0);

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

/** What the window and its terminals wear: the theme chosen — or being looked at in a list — else Keep's own. */
export const palette = computed(() => {
  // Read so the computed follows the system while automatic.
  void systemTheme.value;
  return resolvePalette(gui.value.appearance, themePreview.value ?? gui.value.theme);
});
const systemTheme = signal(0);

/**
 * Whether the terminals' ground is dark as chosen — not as a theme being
 * looked at in a list shows it: a program that asked is told when the ground
 * changes, and the person leafing through themes has not changed it yet.
 */
const groundIsDark = computed(() => {
  void systemTheme.value;
  return resolvePalette(gui.value.appearance, gui.value.theme).dark;
});

/** The daemon's workspaces in the sidebar's order; ones it has not seen join the end. */
export const orderedWorkspaces = computed(() => {
  const list = workspaces.value;
  const order = gui.value.order;
  const rank = new Map(order.map((name, i) => [name, i]));
  return [...list].sort((a, b) => (rank.get(a.name) ?? Infinity) - (rank.get(b.name) ?? Infinity));
});

/** The running workspaces no window lists and nobody put away: made by the command line, or while the app was not running. */
const orphans = computed(() =>
  orphansOf(
    workspaces.value.map((w) => w.name),
    carriedByAll(peers.value, windowLabel.value, carriedOf(gui.value.order, gui.value.hidden)),
    gui.value.hidden,
  ),
);

/**
 * The workspaces the sidebar lists: those of this window's order that are not
 * put away, still running and a Go To from being listed again; and, in the
 * oldest window, the ones nobody lists, which it takes (windowModel.ts). Alone,
 * that is every workspace there is but the ones put away.
 */
export const shownWorkspaces = computed(() => {
  const mine = new Set(gui.value.order);
  const taken = isLeader.value ? new Set(orphans.value) : null;
  const here = orderedWorkspaces.value.filter((w) => !isQuickWorkspace(w.name) && (mine.has(w.name) || taken?.has(w.name)));
  return listed(here, gui.value.hidden);
});

/** What this window says it lists: its order less what is put away, and — the oldest window — what it takes. */
export const carriedHere = computed(() => [
  ...carriedOf(gui.value.order, gui.value.hidden),
  ...(isLeader.value ? orphans.value : []),
]);

/** A workspace's tabs, each with its panes, in the order the window gave them (tabOrder.ts). */
function orderedTabs(tabs: WorkspaceInfo["tabs"], remembered: number[] | undefined): RootTab[] {
  return arrangeTabs(rootTabs(tabs.filter((t) => !t.finished)), remembered, (t) => t.root.id);
}

export const tabsByWorkspace = computed(() => {
  const out = new Map<string, RootTab[]>();
  const orders = gui.value.tabOrder;
  for (const w of workspaces.value) out.set(w.name, orderedTabs(w.tabs, orders[w.name]));
  return out;
});

export const activeWorkspace = computed<string | null>(() => {
  const names = shownWorkspaces.value.map((w) => w.name);
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

export const fontFamily = computed(() =>
  terminalFontStack(fontPreview.value ?? gui.value.font.family, info.value?.platform ?? ""),
);
const cursorShape = computed(() => gui.value.terminal.cursor);
const cursorBlink = computed(() => gui.value.terminal.blink);
const scrollback = computed(() => gui.value.terminal.scrollback);
export const baseFontSize = computed(() => gui.value.font.size || DEFAULT_FONT_SIZE);
export const fontSize = computed(() => zoomedSize(baseFontSize.value, gui.value.zoom));

// ------------------------------------------------------------------ names

/**
 * Names tabs go by while their AI is switched to another account: the program
 * that comes up may announce another title, and the tab goes on being called
 * what it was (the macOS app's `switching`). By `tabKey`.
 */
export const heldNames = signal<ReadonlyMap<string, string>>(new Map());

/** What a tab is called: the name given it, else what its program says it is. */
export function tabLabel(workspace: string, tab: RootTab, index: number): string {
  const key = tabKey(workspace, tab.root.id);
  const given = gui.value.tabNames[key] || heldNames.value.get(key);
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
  return {
    fontFamily: fontFamily.value,
    fontSize: fontSize.value,
    theme: palette.value.terminal,
    cursor: cursorShape.value,
    blink: cursorBlink.value,
    scrollback: scrollback.value,
    dark: groundIsDark.value,
  };
}

// ------------------------------------------------------------------ persistence

/** Change the state; it is kept, and told to the other windows, by windowSync.ts. */
export function update(change: (state: GuiState) => void): void {
  const next = structuredClone(gui.value);
  change(next);
  gui.value = next;
}

// ------------------------------------------------------------------ the daemon

let running: Promise<void> | null = null;
let again = false;
/** Tab moves under way: the window's list waits for them, which re-file what the window keeps. */
let moves = 0;

/**
 * Ask the daemon for its workspaces, and make the window agree. Asked while
 * one answer is on its way, it asks once more when that arrives — the one in
 * flight may be older than what the caller just did — and the promise is of
 * that last one.
 */
export function refresh(): Promise<void> {
  if (running) {
    again = true;
    return running;
  }
  running = (async () => {
    do {
      again = false;
      await refreshOnce();
    } while (again);
  })().finally(() => (running = null));
  return running;
}

async function refreshOnce(): Promise<void> {
  if (moves > 0) return;
  try {
    let list = await api.list();
    // A move began while this was on its way: the answer may be from either side of it.
    if (moves > 0) return;
    const firstAnswer = daemonError.value !== null;
    // After an outage the list may be another daemon's: its ids are not the
    // ones the last list had, and a tab is not an heir to one of those. It may
    // be one that has just started, with nothing in it, too: the tabs of the
    // last one are asked for before the window takes the empty list for the
    // truth.
    if (firstAnswer && settled) {
      const was = await adoptIdentity();
      const restored = await restoreIfEmpty(list);
      if (restored) {
        list = await api.list();
        say("Restaurado após reinício");
      }
      carryOver(was, restored);
    }
    applyList(list, firstAnswer);
    if (firstAnswer) terminals.reattach();
  } catch (error) {
    daemonError.value = String(error);
    // Started again if it went away: the first launch after it was killed,
    // say. Its tabs are gone with it, which the core's restore is for.
    void api.ensureDaemon().catch(() => {});
  }
}

function applyList(list: WorkspaceInfo[], fresh = false): void {
  const previous = fresh ? [] : workspaces.value;
  batch(() => {
    workspaces.value = list;
    daemonError.value = null;
    loaded.value = true;
    reconcile(list, previous);
  });
}

/** Bring the window's own state in line with what the daemon says exists. */
function reconcile(list: WorkspaceInfo[], previous: WorkspaceInfo[]): void {
  const names = new Set(list.map((w) => w.name));
  const alive = new Set<string>();
  for (const w of list) for (const t of w.tabs) if (!t.finished) alive.add(viewKey(w.name, t.id));
  terminals.prune(alive);
  // Until the window has decided whether to restore, an empty daemon is not
  // news: what it says must not take the order and the names that restoring
  // is about to give meaning back to.
  if (!settled) return;

  const state = gui.value;
  // A root that closed, or left its panes behind: the pane that stood in for
  // it is the tab now, and carries on with its name, its place and the
  // keyboard.
  const heirs = new Map<string, Map<number, number>>();
  for (const w of list) {
    const was = previous.find((p) => p.name === w.name);
    const found = was && heirsOf(was.tabs.filter((t) => !t.finished), w.tabs.filter((t) => !t.finished));
    if (found && found.size > 0) heirs.set(w.name, found);
  }
  const heirOf = (workspace: string, id: number) => heirs.get(workspace)?.get(id);

  const order = state.order.filter((n) => names.has(n));
  // The oldest window takes the workspaces nobody lists, at the end (windowModel.ts).
  if (isLeader.value) {
    const listedByAll = carriedByAll(peers.value, windowLabel.value, carriedOf(order, state.hidden));
    order.push(...orphansOf(list.map((w) => w.name), listedByAll, state.hidden));
  }
  const named = { ...state.tabNames };
  for (const [workspace, found] of heirs) {
    for (const [root, heir] of found) {
      const from = tabKey(workspace, root);
      refile(from, tabKey(workspace, heir));
      moveShares(from, tabKey(workspace, heir));
      if (named[from] === undefined) continue;
      named[tabKey(workspace, heir)] ??= named[from];
      delete named[from];
    }
  }
  const tabNames = Object.fromEntries(Object.entries(named).filter(([k]) => alive.has(k)));
  const workspaceNames = Object.fromEntries(Object.entries(state.workspaceNames).filter(([k]) => names.has(k)));
  const folded = state.folded.filter((n) => names.has(n));
  const hidden = stillRunning(state.hidden, names);
  const lastTab = Object.fromEntries(
    Object.entries(state.lastTab)
      .filter(([k]) => names.has(k))
      .map(([k, id]) => [k, heirOf(k, id) ?? id]),
  );
  const roots = new Map(list.map((w) => [w.name, new Set(rootTabs(w.tabs.filter((t) => !t.finished)).map((t) => t.root.id))]));
  const handed = Object.fromEntries(
    Object.entries(state.tabOrder).map(([workspace, ids]) => {
      let next = ids;
      for (const [root, heir] of heirs.get(workspace) ?? []) next = substitute(next, root, heir);
      return [workspace, next];
    }),
  );
  const tabOrder = pruneOrders(handed, roots);
  // The dividers of tabs that are gone go with them.
  keepShares((key) => {
    const [workspace, root] = key.split("\u0000");
    return roots.get(workspace)?.has(Number(root)) === true;
  });

  // The tab in front went away: the one beside it comes forward.
  let active = state.active;
  if (active) {
    const heir = heirOf(active.workspace, active.tab);
    if (heir !== undefined) active = { workspace: active.workspace, tab: heir };
  }
  const tabsOf = (ws: string) => orderedTabs(list.find((w) => w.name === ws)?.tabs ?? [], tabOrder[ws]);
  if (active) {
    const tabs = tabsOf(active.workspace);
    if (!tabs.some((t) => t.root.id === active!.tab)) {
      const before = tabsByWorkspaceSnapshot.get(active.workspace) ?? [];
      const at = before.findIndex((t) => t === active!.tab);
      const neighbour = tabs[Math.min(Math.max(at, 0), tabs.length - 1)] ?? tabs[tabs.length - 1];
      if (neighbour) active = { workspace: active.workspace, tab: neighbour.root.id };
      else {
        const other = order.find((n) => !hidden.includes(n) && tabsOf(n).length > 0);
        active = other ? { workspace: other, tab: lastTab[other] ?? tabsOf(other)[0].root.id } : null;
      }
    }
  }
  tabsByWorkspaceSnapshot = new Map(list.map((w) => [w.name, tabsOf(w.name).map((t) => t.root.id)]));

  // The pane that held the keyboard keeps it in the tab its heir carries on.
  if (heirs.size > 0) {
    const kept = { ...focusedPane.value };
    for (const [workspace, found] of heirs) {
      for (const [root, heir] of found) {
        const from = tabKey(workspace, root);
        if (kept[from] === undefined) continue;
        kept[tabKey(workspace, heir)] ??= kept[from];
        delete kept[from];
      }
    }
    focusedPane.value = kept;
  }

  const changed =
    JSON.stringify([order, tabNames, workspaceNames, folded, hidden, lastTab, tabOrder, active]) !==
    JSON.stringify([
      state.order,
      state.tabNames,
      state.workspaceNames,
      state.folded,
      state.hidden,
      state.lastTab,
      state.tabOrder,
      state.active,
    ]);
  if (changed) {
    gui.value = { ...state, order, tabNames, workspaceNames, folded, hidden, lastTab, tabOrder, active };
  }
}

let tabsByWorkspaceSnapshot = new Map<string, number[]>();

/**
 * The window has decided whether to restore; from here on the daemon's list
 * rules. Until then an empty daemon is not news: what it says must not take
 * the order of the sidebar and the names the restore is about to bring back.
 */
let settled = false;

/** The restore the page was given (`connect`), and the daemon it last asked about: a daemon is asked once. */
let restoreTabs: (() => Promise<Restored | null>) | undefined;
let restoreAsked: string | undefined;

/**
 * Which daemon this is. Tab numbers are one daemon's — a new one counts from
 * one again —, so what the window keeps by them belongs to no tab of another.
 * The names given are dropped at once (the restore brings back the ones the
 * tabs it makes had); the rest — the order of the tabs, the tab in front — is
 * handed back, for `carryOver` once it is known which tabs came back and under
 * what numbers. Null when it is the daemon the window knew (one told another
 * way, before it said when it started, is the same), or when it knew none.
 */
async function adoptIdentity(): Promise<TabNumbers | null> {
  const identity = await api.daemonIdentity().catch(() => null);
  if (identity === null || identity === gui.value.daemon) return null;
  const kept = gui.value.daemon;
  if (sameDaemon(kept, identity)) {
    gui.value = { ...gui.value, daemon: identity };
    return null;
  }
  const { tabOrder, lastTab, active } = gui.value;
  gui.value = { ...gui.value, daemon: identity, tabNames: {} };
  return kept === "" ? null : { tabOrder, lastTab, active };
}

/** The tabs of the daemon of before come to the one that is, or go with it: see `adoptIdentity`. */
function carryOver(was: TabNumbers | null, restored: Restored | null): void {
  if (!was) return;
  gui.value = { ...gui.value, ...(restored ? remapTabNumbers(was, restored.made) : noTabNumbers()) };
}

/**
 * A daemon with no tab that has not been asked yet whether the last one left
 * some to bring back: the core is asked (`keep restauro`, docs/restauro.md —
 * the one place tabs are made again), once per daemon. Which tabs came back,
 * or null when none did.
 */
async function restoreIfEmpty(list: WorkspaceInfo[]): Promise<Restored | null> {
  if (!restoreTabs || list.some((w) => w.tabs.some((t) => !t.finished))) return null;
  const daemon = gui.value.daemon || "?";
  if (restoreAsked === daemon) return null;
  restoreAsked = daemon;
  const saying = "Restaurando as abas…";
  say(saying, 60);
  try {
    return await restoreTabs();
  } catch {
    return null;
  } finally {
    if (notice.value === saying) notice.value = null;
  }
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

let readingScreens = false;

/**
 * Read the screens of the panes running Claude Code or Codex. A pane with no
 * terminal — its tab is asleep (hibernation.ts) — is read off the daemon's
 * own copy of its screen, so a conversation waiting on you is still seen
 * waiting.
 */
async function readScreens(): Promise<void> {
  if (readingScreens) return;
  readingScreens = true;
  try {
    const unseen = new Map<string, string>();
    await Promise.all(
      workspaces.value.flatMap((w) =>
        w.tabs
          .filter((t) => !t.finished && isAiProgram(programOf(t.command, t.title)) && !terminals.existing(w.name, t.id))
          .map((t) =>
            api.preview(w.name, t.id).then(
              (text) => void unseen.set(viewKey(w.name, t.id), text),
              () => {},
            ),
          ),
      ),
    );
    readScreensFrom(unseen);
  } finally {
    readingScreens = false;
  }
}

function readScreensFrom(unseen: ReadonlyMap<string, string>): void {
  const nextActivities = new Map<string, ClaudeActivity>();
  const nextModes = new Map<string, ClaudeMode | null>();
  const forBar: BarInput[] = [];
  const previousActivities = activities.value;
  const previousModes = modes.value;
  for (const w of workspaces.value) {
    for (const pane of w.tabs) {
      if (pane.finished) continue;
      const program = programOf(pane.command, pane.title);
      if (!isAiProgram(program)) continue;
      const key = viewKey(w.name, pane.id);
      const view = terminals.existing(w.name, pane.id);
      const text = view ? view.screenText() : (unseen.get(key) ?? "");
      if (program === "codex") {
        const a = text ? readCodexActivity(text) : undefined;
        const kept = a ?? previousActivities.get(key);
        if (kept) nextActivities.set(key, kept);
        forBar.push({ key, program: "codex", shellPid: pane.shellPid, screen: text, activity: kept });
        continue;
      }
      // Undefined is "cannot tell": what was known stands.
      const m = text ? declaredMode(text) : undefined;
      if (m !== undefined) nextModes.set(key, m);
      else if (previousModes.has(key)) nextModes.set(key, previousModes.get(key)!);
      const a = text ? readActivity(text) : undefined;
      const kept = a ?? previousActivities.get(key);
      if (kept) nextActivities.set(key, kept);
      forBar.push({ key, program: "claude", shellPid: pane.shellPid, screen: text, activity: kept });
    }
  }
  void readBars(forBar);
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

/** What the bar under a pane is built from: its screen, its turn, and the shell whose status file speaks for it. */
interface BarInput {
  key: string;
  program: "claude" | "codex";
  shellPid: number;
  screen: string;
  activity: ClaudeActivity | undefined;
}

let readingBars = false;

/**
 * Build the bar of each pane running Claude Code or Codex. Claude Code's model
 * and effort are in the file its status line saved for the pane's shell (one
 * question to the app for all of them); Codex's are on its screen.
 */
async function readBars(inputs: BarInput[]): Promise<void> {
  if (readingBars) return;
  readingBars = true;
  try {
    const shells = inputs.filter((i) => i.program === "claude" && i.shellPid > 0).map((i) => i.shellPid);
    let files: Record<string, string> = {};
    if (shells.length > 0) {
      try {
        files = await api.claudeStatuses(shells);
      } catch {
        return; // cannot read now: the bars stand as they are
      }
    }
    if (inputs.some((i) => i.program === "codex")) await readCodexModels();
    const previous = statuses.value;
    const next = new Map<string, AiStatus>();
    for (const input of inputs) {
      let status: AiStatus | null;
      if (input.program === "claude") {
        const text = files[String(input.shellPid)];
        status = claudeStatus(text ? parseStatusFile(text) : null, input.screen, input.activity);
      } else {
        const before = previous.get(input.key);
        // By its name on screen, which the cache knows as well as the slug, and which reads the same without it.
        const kept = before?.program === "codex" ? { model: before.modelName, effort: before.effort } : null;
        status = codexStatus(input.screen, input.activity, kept, codexModels.value);
      }
      if (status) next.set(input.key, status);
    }
    const same = next.size === previous.size && [...next].every(([k, v]) => sameStatus(previous.get(k), v));
    if (!same) statuses.value = next;
  } finally {
    readingBars = false;
  }
}

/** Which version of Codex's model cache `codexModels` came from, and when it was last asked. */
let codexCache: { stamp: string | null; asked: number } = { stamp: null, asked: 0 };

/** Codex's model cache read again when it changed, asked at most every few seconds. */
async function readCodexModels(): Promise<void> {
  if (Date.now() - codexCache.asked < 3000) return;
  codexCache = { ...codexCache, asked: Date.now() };
  try {
    const answer = await api.codexModels(codexCache.stamp);
    if (answer.stamp === null) {
      if (codexModels.value.length > 0) codexModels.value = [];
      codexCache = { ...codexCache, stamp: null };
    } else if (answer.text !== null) {
      codexModels.value = parseCodexModels(answer.text);
      codexCache = { ...codexCache, stamp: answer.stamp };
    }
  } catch {
    // Asked again in a few seconds.
  }
}

/** A line in the app's log, with the time it was said. */
export function log(line: string): void {
  void api.appLog(`${new Date().toISOString()} ${line}`).catch(() => {});
}

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/** A pane's keyboard and screen, for walking its program's picker. */
function pickerIO(view: TermView, program: string): PickerIO {
  return {
    key: (keys) => view.type(keys),
    screen: async () => view.screenText(),
    sleep,
    log: (line) => log(`${program} ${view.workspace}/${view.tab}: ${line}`),
  };
}

/**
 * Do what a pick in the bar says, in the pane it was picked under: walk
 * Claude Code's or Codex's own picker by keys, or type a command and Enter.
 * One at a time per pane: a pick made while another is being walked is
 * dropped.
 */
export async function runChoice(workspace: string, pane: number, choice: Choice): Promise<void> {
  const view = terminals.existing(workspace, pane);
  const key = viewKey(workspace, pane);
  if (!view || picking.value.has(key)) return;
  picking.value = new Set([...picking.value, key]);
  const action = choice.action;
  try {
    switch (action.kind) {
      case "claudeModel":
        await chooseClaudeModel(pickerIO(view, "claude"), action.label);
        break;
      case "claudeEffort":
        await chooseClaudeEffort(pickerIO(view, "claude"), action.level);
        break;
      case "codex":
        await chooseCodex(pickerIO(view, "codex"), action.model, action.effort);
        break;
      case "type":
        await typeCommand(view, action.command, action.keepDefaults);
        break;
    }
  } catch (error) {
    // The tab went away or let go; nothing typed after that matters.
    log(`bar ${workspace}/${pane}: ${String(error)}`);
  } finally {
    const rest = new Set(picking.value);
    rest.delete(key);
    picking.value = rest;
    view.focus();
  }
}

/**
 * Type a command into a pane's prompt and send it. With `keepDefaults`, the
 * keys of Claude Code's settings that it saves as the default for new
 * sessions (`/model <id>` does) are put back as they were, two and five
 * seconds on — the file read again each time, so nothing written meanwhile
 * is lost, and written only over what was read.
 */
async function typeCommand(view: TermView, command: string, keepDefaults: boolean): Promise<void> {
  const before = keepDefaults ? await api.claudeSettings().then((s) => defaultsOf(s.text)).catch(() => null) : null;
  await view.type(command);
  await sleep(ENTER_DELAY_MS);
  await view.type("\r");
  if (before === null) return;
  for (const delay of [2000, 5000]) setTimeout(() => void putDefaultsBack(before), delay);
}

async function putDefaultsBack(before: string): Promise<void> {
  try {
    const settings = await api.claudeSettings();
    const text = restoreDefaults(settings.text, before);
    if (text === null) return;
    const written = await api.writeClaudeSettings(settings.text, text);
    log(written ? `settings.json: defaults put back (${before})` : "settings.json changed meanwhile: defaults left for the next look");
  } catch (error) {
    log(`settings.json: defaults not put back: ${String(error)}`);
  }
}

/**
 * Claude Code's status line set to the script installed with the app, so
 * the bar learns its model and effort: in a settings file that has none —
 * the key added, nothing else changed — and in a new one where Claude Code's
 * folder is there without it. One with another status line is left alone;
 * what was done goes to the log either way. Windows only: elsewhere the app
 * is for development, and its settings are somebody's own.
 */
export async function setUpStatusLine(): Promise<string> {
  if (info.value?.platform !== "windows") return "fora do Windows";
  let said: string;
  try {
    const settings = await api.claudeSettings();
    if (settings.script === null) {
      said = "o keep-statusline.ps1 não está junto do app";
    } else if (settings.text === null && !settings.folder) {
      said = `sem a pasta do Claude Code (${settings.path}): ele ainda não rodou aqui`;
    } else {
      const plan = planStatusLine(settings.text, settings.script);
      switch (plan.kind) {
        case "write":
          said = (await api.writeClaudeSettings(settings.text, plan.text))
            ? `gravada em ${settings.path}: ${plan.command}`
            : `${settings.path} mudou enquanto era lido: fica para a próxima abertura`;
          break;
        case "ours":
          said = "já é a do Keep";
          break;
        case "other":
          said = `${settings.path} já tem outra (${plan.command}): não foi tocada`;
          break;
        case "unreadable":
          said = `${settings.path} não foi tocado: ${plan.why}`;
          break;
      }
    }
  } catch (error) {
    said = `não deu para ler o settings.json: ${String(error)}`;
  }
  log(`statusLine: ${said}`);
  return said;
}

/** What setting the status line up did, once it is done: the scripted check waits on it. */
export let statusLineSetup: Promise<string> = Promise.resolve("");

// ------------------------------------------------------------------ actions

export function select(workspace: string, tab?: number): void {
  const tabs = tabsByWorkspace.value.get(workspace) ?? [];
  const id = tab ?? gui.value.lastTab[workspace] ?? tabs[0]?.root.id;
  update((s) => {
    // Going to a workspace lists it here, and puts it back if it was put away (windowModel.ts).
    ({ order: s.order, hidden: s.hidden } = adopt(s, workspace));
    if (id === undefined) {
      s.active = { workspace, tab: -1 };
      return;
    }
    s.active = { workspace, tab: id };
    s.lastTab[workspace] = id;
  });
  if (id !== undefined) noteVisit(workspace, id);
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

/** The next workspace in the sidebar's order, or the one before: its last tab in front. */
export function stepWorkspace(direction: number): void {
  const names = shownWorkspaces.value.map((w) => w.name);
  if (names.length === 0) return;
  const at = Math.max(0, names.indexOf(activeWorkspace.value ?? ""));
  select(names[(at + direction + names.length) % names.length]);
}

export function goToTab(index: number | "last"): void {
  const tabs = activeTabs();
  const ws = activeWorkspace.value;
  if (!ws || tabs.length === 0) return;
  const tab = index === "last" ? tabs[tabs.length - 1] : tabs[index];
  if (tab) select(ws, tab.root.id);
}

export async function newTab(workspace = activeWorkspace.value ?? undefined, folder?: string): Promise<number | null> {
  if (!workspace) {
    await newWorkspace();
    return null;
  }
  const { cols, rows } = guessSize();
  const cwd = folder || activePane.value?.cwd || workspaceCwd(workspace) || info.value?.home || null;
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

// ------------------------------------------------------------------ closing, and the worktrees that go with it

/**
 * The worktrees that go with these tabs (worktrees.ts). Asked before the
 * question, because after the tab closes the conversation in it is gone and
 * so is the link between the tab and what it made; the question waits for
 * it, up to the core's three seconds.
 */
async function listWorktrees(targets: WorktreeTarget[]): Promise<WorktreeAnswer> {
  if (!iaInfo.value?.available || targets.length === 0) return { kind: "off" };
  try {
    const answer = await api.iaAsk(["worktrees", "listar", "--prazo", "3.0", ...targets.map(targetArgument)]);
    return readListing(answer);
  } catch (error) {
    return readListing(null, String(error));
  }
}

/** Each tab with its panes: every daemon tab closing it ends. */
const targetOf = (workspace: string, tab: RootTab): WorktreeTarget => ({
  workspace,
  tabs: [...new Set([tab.root.id, ...tab.panes.map((p) => p.id)])],
});

/**
 * After a yes, and only once the tabs did close: send the listed worktrees to
 * the Recycle Bin, in the backend — it finishes even if the window does not —
 * and say what did not go.
 */
function trashAfter(answer: WorktreeAnswer): void {
  if (answer.kind !== "listing") return;
  const going = answer.listing.lixeira ?? [];
  if (going.length === 0) return;
  trashing.value += 1;
  void api
    .iaTrash(
      going.map((item) => ({ caminho: item.caminho, nasceu_ns: item.nasceu_ns ?? null })),
      listingPids(answer.listing),
    )
    .catch((error) => [String(error)])
    .then((problems) => {
      if (problems.length === 0) return;
      void ask({
        title: "Worktrees",
        message: trashProblems(problems),
        buttons: [{ label: "OK", value: "ok", primary: true }],
      });
    })
    .finally(() => (trashing.value -= 1));
}

/** A question about ending something, with what goes to the Recycle Bin with it said. */
async function askToEnd(
  targets: WorktreeTarget[],
  subject: string,
  question: { title: string; message: string; action: string },
): Promise<WorktreeAnswer | null> {
  if (asking) return null;
  asking = true;
  try {
    const worktrees = await listWorktrees(targets);
    const result = await ask({
      title: question.title,
      message: question.message + worktreeNote(worktrees, subject, info.value?.home ?? ""),
      buttons: [
        { label: "Cancelar", value: "cancel" },
        { label: question.action, value: "close", danger: true, primary: true },
      ],
    });
    return result?.button === "close" ? worktrees : null;
  } finally {
    asking = false;
  }
}

/** A question about ending something is up, or on its way: a second one waits its turn. */
let asking = false;

/** Close a whole tab — its panes with it — after asking. */
export async function closeTab(workspace: string, tab: RootTab, label: string): Promise<boolean> {
  const worktrees = await askToEnd([targetOf(workspace, tab)], "desta aba", {
    title: `Fechar a aba “${label}”?`,
    message: "O que estiver rodando nela será encerrado.",
    action: "Fechar a aba",
  });
  if (!worktrees) return false;
  // The panes first: closing the root alone would hand them a tab of their own.
  let closed = true;
  for (const pane of [...tab.panes].reverse()) {
    await api.closeTab(workspace, pane.id).catch(() => (closed = false));
  }
  await refresh();
  if (closed) trashAfter(worktrees);
  return true;
}

export async function closePane(workspace: string, pane: number): Promise<boolean> {
  const worktrees = await askToEnd([{ workspace, tabs: [pane] }], "deste painel", {
    title: "Fechar este painel?",
    message: "O que estiver rodando nele será encerrado.",
    action: "Fechar o painel",
  });
  if (!worktrees) return false;
  const closed = await api.closeTab(workspace, pane).then(
    () => true,
    () => false,
  );
  await refresh();
  if (closed) trashAfter(worktrees);
  return true;
}

export async function closeWorkspace(workspace: string): Promise<boolean> {
  const label = workspaceLabel(workspace);
  const tabs = tabsByWorkspace.value.get(workspace) ?? [];
  const count = tabs.length;
  const worktrees = await askToEnd(
    tabs.map((tab) => targetOf(workspace, tab)),
    "deste workspace",
    {
      title: `Fechar o workspace “${label}”?`,
      message:
        count === 1
          ? "A aba dele e o que estiver rodando nela serão encerrados."
          : `As ${count} abas dele e o que estiver rodando nelas serão encerrados.`,
      action: "Fechar o workspace",
    },
  );
  if (!worktrees) return false;
  const closed = await api.killWorkspace(workspace).then(
    () => true,
    () => false,
  );
  await refresh();
  if (closed) trashAfter(worktrees);
  return true;
}

export function renameTab(workspace: string, tab: number, name: string): void {
  update((s) => {
    const key = tabKey(workspace, tab);
    if (name.trim()) s.tabNames[key] = name.trim();
    else delete s.tabNames[key];
  });
}

/** Keep a tab called `title` while its AI is switched (`null` lets it go). */
export function holdTabName(workspace: string, tab: number, title: string | null): void {
  const key = tabKey(workspace, tab);
  if ((heldNames.value.get(key) ?? null) === title) return;
  const next = new Map(heldNames.value);
  if (title) next.set(key, title);
  else next.delete(key);
  heldNames.value = next;
}

/** After a switch the name the tab had becomes its own (see `nameToKeep`). */
export function preserveTabName(workspace: string, tab: number, title: string): void {
  const kept = nameToKeep(title, gui.value.tabNames[tabKey(workspace, tab)]);
  if (kept) renameTab(workspace, tab, kept);
}

export function renameWorkspace(workspace: string, name: string): void {
  update((s) => {
    if (name.trim() && name.trim() !== workspace) s.workspaceNames[workspace] = name.trim();
    else delete s.workspaceNames[workspace];
  });
}

// ------------------------------------------------------------------ naming

/** Where a name can be typed when nobody asked for a place: the row above the terminal, else the sidebar, else nowhere. */
function defaultNameSurface(): NameSurface | null {
  if (!gui.value.verticalTabs) return "strip";
  return gui.value.sidebar.visible ? "sidebar" : null;
}

/** Whether a tab's name is being typed on this surface now. */
export function editingTabOn(surface: NameSurface, workspace: string, tab: number): boolean {
  const e = editing.value;
  return e?.kind === "tab" && e.workspace === workspace && e.tab === tab && (e.surface ?? defaultNameSurface()) === surface;
}

/**
 * Open a tab's name for typing, where the gesture was made or else where the
 * window shows tabs. With neither the row nor the sidebar in sight — vertical
 * tabs and the sidebar hidden — a question asks for it instead.
 */
export function startRename(workspace: string, tab: number, surface?: NameSurface): void {
  const where = surface ?? defaultNameSurface();
  if (where === null) {
    void renameByQuestion(workspace, tab);
    return;
  }
  // A name cannot be typed in a row folded out of sight.
  if (where === "sidebar" && gui.value.folded.includes(workspace)) toggleFolded(workspace);
  editing.value = { kind: "tab", workspace, tab, surface: where };
}

async function renameByQuestion(workspace: string, tab: number): Promise<void> {
  const tabs = tabsByWorkspace.value.get(workspace) ?? [];
  const at = tabs.findIndex((t) => t.root.id === tab);
  if (at < 0) return;
  const seed = gui.value.tabNames[tabKey(workspace, tab)] ?? tabLabel(workspace, tabs[at], at);
  const result = await ask({
    title: "Renomear a aba",
    fields: [{ name: "name", label: "Nome", value: seed }],
    buttons: [
      { label: "Cancelar", value: "cancel" },
      { label: "Renomear", value: "rename", primary: true },
    ],
  });
  // A name that comes back as it went out is no new name (see `finishTabName`).
  if (result?.button === "rename" && result.values.name.trim() !== seed.trim()) renameTab(workspace, tab, result.values.name);
  requestAnimationFrame(() => focusActive());
}

/**
 * A name typed over a tab's title is done: `value` is the name to keep, or
 * null when there is nothing to keep — Esc, or the field left as it was
 * opened, since a title kept by accident would stop following its program.
 * The keyboard goes back to the work unless it already went somewhere else
 * (`keyboard` says), as Finder leaves it where the click put it.
 */
export function finishTabName(workspace: string, tab: number, value: string | null, keyboard: "back" | "elsewhere"): void {
  editing.value = null;
  if (value !== null) renameTab(workspace, tab, value);
  giveKeyboardBack(keyboard);
}

/** The same for a workspace's name. */
export function finishWorkspaceName(workspace: string, value: string | null, keyboard: "back" | "elsewhere"): void {
  editing.value = null;
  if (value !== null) renameWorkspace(workspace, value);
  giveKeyboardBack(keyboard);
}

/** A frame later, and not if another name was opened meanwhile: it has the keyboard now. */
function giveKeyboardBack(keyboard: "back" | "elsewhere"): void {
  if (keyboard === "back") requestAnimationFrame(() => editing.value === null && focusActive());
}

// ------------------------------------------------------------------ where tabs are

export function toggleVerticalTabs(): void {
  update((s) => void (s.verticalTabs = !s.verticalTabs));
}

/**
 * Take a workspace out of this window's sidebar. It keeps running, and so do
 * its tabs: what leaves is its place in the list, which Go To gives back, and
 * which a relaunch leaves out as it was left. If another window lists it, it
 * is only this window's list it leaves; put away is what the last window
 * listing it does (windowModel.ts).
 */
export function putAway(workspace: string): void {
  const names = shownWorkspaces.value.map((w) => w.name);
  if (!names.includes(workspace)) return;
  // Standing in the one going out: step to a neighbour.
  const next = activeWorkspace.value === workspace ? neighbour(names, workspace) : undefined;
  update((s) => void ({ order: s.order, hidden: s.hidden } = remove(s, peers.value, windowLabel.value, workspace)));
  if (next !== undefined) select(next);
}

/** Put the tabs of a workspace in this order (root ids). */
export function reorderTabs(workspace: string, ids: number[]): void {
  const now = (tabsByWorkspace.value.get(workspace) ?? []).map((t) => t.root.id);
  if (ids.length === now.length && ids.every((id, i) => id === now[i])) return;
  update((s) => void (s.tabOrder[workspace] = ids));
}

/** Keys of a map renamed as tabs were re-filed: what the window knew of a pane goes with it. */
function refiled<V>(map: ReadonlyMap<string, V>, keys: ReadonlyMap<string, string>): Map<string, V> {
  const out = new Map<string, V>();
  for (const [key, value] of map) out.set(keys.get(key) ?? key, value);
  return out;
}

/**
 * Move a tab, its panes with it, to another workspace, to land before the tab
 * `before` of that workspace or at the end. The shells carry on untouched —
 * only the filing changes, and the daemon issues new ids — so the terminals
 * are re-keyed rather than rebuilt, and what the window knew of the tab goes
 * with it: its name, its place, which pane held the keyboard, what Claude Code
 * was seen doing in it. If it was the tab in front, the window follows it.
 */
export async function moveTabTo(workspace: string, tab: RootTab, to: string, before: number | null): Promise<boolean> {
  if (workspace === to) return false;
  const root = tab.root.id;
  const inFront = activeWorkspace.value === workspace && activeTab.value?.root.id === root;
  const oldIds = memberIds(workspaces.value.find((w) => w.name === workspace)?.tabs ?? [], root);
  moves += 1;
  try {
    let newRoot: number;
    try {
      newRoot = await api.moveTab(workspace, root, to);
    } catch (error) {
      void ask({
        title: "Não deu para mover a aba",
        message: String(error),
        buttons: [{ label: "OK", value: "ok", primary: true }],
      });
      return false;
    }
    let list: WorkspaceInfo[];
    try {
      list = await api.list();
    } catch {
      // The tab is in its new place; the window learns it on its next look, without the terminal's scrollback.
      return true;
    }
    const newIds = memberIds(list.find((w) => w.name === to)?.tabs ?? [], newRoot);
    // The new ids are issued in the order the old ones were listed in.
    const pairs = oldIds.length === newIds.length ? oldIds.map((id, i) => [id, newIds[i]] as const) : [];
    terminals.rekey(pairs.map(([from, now]) => ({ from: [workspace, from], to: [to, now] })));
    const keys = new Map(pairs.map(([from, now]) => [viewKey(workspace, from), viewKey(to, now)]));
    refile(tabKey(workspace, root), tabKey(to, newRoot));
    const panes = new Map(pairs);
    batch(() => {
      activities.value = refiled(activities.value, keys);
      modes.value = refiled(modes.value, keys);
      statuses.value = refiled(statuses.value, keys);
      waitingSince.value = refiled(waitingSince.value, keys);
      const held = focusedPane.value[tabKey(workspace, root)];
      if (held !== undefined) {
        const { [tabKey(workspace, root)]: _gone, ...rest } = focusedPane.value;
        focusedPane.value = { ...rest, [tabKey(to, newRoot)]: panes.get(held) ?? newRoot };
      }
      update((s) => {
        const was = tabKey(workspace, root);
        if (s.tabNames[was] !== undefined) {
          s.tabNames[tabKey(to, newRoot)] = s.tabNames[was];
          delete s.tabNames[was];
        }
        const there = orderedTabs(list.find((w) => w.name === to)?.tabs ?? [], s.tabOrder[to]).map((t) => t.root.id);
        s.tabOrder[to] = insertBefore(
          there.filter((id) => id !== newRoot),
          newRoot,
          before,
        );
        if (s.lastTab[workspace] === root) delete s.lastTab[workspace];
        if (inFront) {
          s.active = { workspace: to, tab: newRoot };
          s.lastTab[to] = newRoot;
          ({ order: s.order, hidden: s.hidden } = adopt(s, to));
        }
      });
      applyList(list);
    });
    if (inFront) requestAnimationFrame(() => focusActive());
    return true;
  } finally {
    moves -= 1;
  }
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

/**
 * The terminal of a pane, once it has one: a pane of a tab that is asleep has
 * none until the tab is shown (hibernation.ts), and a terminal built for it
 * is empty until the daemon repaints. For what has to act on the screen of a
 * tab it has just gone to — a search result, a line to scroll to.
 */
export async function viewWhenAwake(workspace: string, pane: number, timeoutMs = 3000): Promise<TermView | undefined> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const view = terminals.existing(workspace, pane);
    // Attached: the daemon's repaint is on its way, or in.
    if (view?.isAttached) return view;
    if (Date.now() > deadline) return view;
    await sleep(50);
  }
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

/**
 * Everything the window needs before it draws: what platform, what state,
 * what look. What the app told it of itself and of the other windows.
 */
export async function start(options: StartOptions): Promise<api.WindowBoot> {
  const startup = await api.startup();
  info.value = startup;
  // The app's state and this window's own, and which windows there are (src-tauri: windows.rs).
  const boot = await api.windowBoot();
  windowLabel.value = boot.label;
  peers.value = readPeers(boot.windows);
  gui.value = joinState(boot.shared, boot.window, boot.inherit);
  // A state that could not be read is told, once, with what was done about it.
  void api
    .stateWarning()
    .then((message) => {
      if (message) void ask({ title: "O estado do Keep", message, buttons: [{ label: "OK", value: "ok", primary: true }] });
    })
    .catch(() => {});

  // Before anything is drawn: the catalogue, if a theme was chosen, and the
  // symbols font the terminals fall back to for icons.
  await Promise.all([gui.value.theme ? loadThemes().catch(() => {}) : undefined, loadSymbolsFont()]);
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

  // Look and the clock follow the state from here on; keeping it is windowSync.ts's.
  effect(() => applyPalette(palette.value));
  effect(() => terminals.setLook(look()));
  // The tabs' titles grow with the zoom, as the terminals' text does.
  effect(() => document.documentElement.style.setProperty("--zoom", String(gui.value.zoom)));
  setInterval(() => (now.value = Date.now()), 1000);

  const win = getCurrentWindow();
  void win.isMaximized().then((m) => (maximized.value = m)).catch(() => {});
  void win.onResized(() => void win.isMaximized().then((m) => (maximized.value = m)).catch(() => {}));

  // Not waited on: the window draws while it is being done.
  statusLineSetup = setUpStatusLine();
  return boot;
}

export interface ConnectOptions {
  /**
   * Bring back the tabs the last daemon had, for a daemon that has none (the
   * core does it: `keep restauro`, docs/restauro.md); which came back, under
   * what numbers, or null when none did. Asked before the window first hears
   * from the daemon, so that nothing
   * is drawn in the place of what is coming back, and again if the daemon turns
   * up empty after being out of reach.
   */
  restore?: () => Promise<Restored | null>;
  /**
   * The windows of the last run, to be opened (by the oldest window only): once
   * the daemon has its tabs and the window knows what became of the old ones,
   * and before it decides what the daemon's list means. `remap` carries the
   * tab a window was left on to the number it has now, null when it is gone.
   */
  windows?: (remap: (active: GuiState["active"]) => GuiState["active"]) => Promise<void>;
}

/**
 * Start the daemon if it is not up, and wait for a slow one — a first launch
 * after a reboot, with a program scanning its executable — up to a minute:
 * what the window decides about the tabs it holds is decided on its list.
 * That list, or null for a daemon that never answered.
 */
async function reachDaemon(): Promise<WorkspaceInfo[] | null> {
  const until = Date.now() + 60_000;
  for (let attempt = 0; ; attempt++) {
    // Started again now and then, in case it died while this was waiting.
    if (attempt % 15 === 0) {
      try {
        await api.ensureDaemon();
      } catch (error) {
        daemonError.value = String(error);
      }
    }
    try {
      return await api.list();
    } catch (error) {
      daemonError.value = String(error);
    }
    if (Date.now() > until) return null;
    await sleep(1000);
  }
}

/** Find the daemon — starting it if needed — and keep asking it. */
export async function connect(options: ConnectOptions = {}): Promise<void> {
  restoreTabs = options.restore;
  // The tabs of a daemon this window starts are the last one's, brought back
  // by the core before the window has heard anything of the new one: an empty
  // list would be drawn as "no workspace", and the sidebar's order, kept for
  // the workspaces that are about to return, taken as stale. One that was
  // already running and is empty was emptied on purpose, and the core knows
  // that too: it restores once per daemon.
  const first = await reachDaemon();
  const was = first !== null ? await adoptIdentity() : null;
  const restored = first !== null ? await restoreIfEmpty(first) : null;
  carryOver(was, restored);
  await refresh();
  if (restored) say("Restaurado após reinício");
  await options.windows?.((active) => (was ? remapTabNumbers({ ...noTabNumbers(), active }, restored?.made ?? []).active : active));
  settled = true;
  await refresh();
  // Something must be on screen: a daemon with nothing in it and nothing to
  // bring back starts with a workspace named after the person.
  const user = info.value?.user || baseName(info.value?.home ?? "");
  if (loaded.value && workspaces.value.length === 0 && user) await createWorkspace(user, null).catch(() => {});
  const active = activeWorkspace.value;
  if (active) select(active, activeTab.value?.root.id);

  setInterval(() => void refresh(), 1000);
  setInterval(() => void readScreens(), 1000);
}
