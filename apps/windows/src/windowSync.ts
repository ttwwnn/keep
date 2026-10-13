// Several windows at run time: keeping this window's state and the app's,
// telling the other windows what changed and hearing what they changed, saying
// what this window lists and shows, and opening, restoring and handing tabs
// between windows. The macOS app does it all in one process (`Session`, the
// AppDelegate's windows); here each window is a page of its own and the app
// (src-tauri: windows.rs) is where they meet. What the lists mean is
// windowModel.ts's; this only keeps them in step.

import { effect } from "@preact/signals";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "./api";
import { registerCommand } from "./commands";
import type { Point } from "./drag";
import type { RootTab } from "./layout";
import type { GuiState } from "./state";
import {
  activeTab,
  activeWorkspace,
  carriedHere,
  gui,
  log,
  say,
  select,
  tabsByWorkspace,
  terminals,
} from "./store";
import {
  MAIN_LABEL,
  carriedOf,
  cascaded,
  emptyPlacement,
  forgetTaken,
  frameOfTornTab,
  readPeers,
  type Placement,
} from "./windowModel";
import { peers, severalWindows, windowLabel } from "./windowPeers";
import {
  applyOps,
  diffShared,
  joinState,
  readOps,
  sharedOf,
  windowOf,
  withShared,
  type Shared,
} from "./windowState";

/** After the last change to the state: a burst of them (a divider dragged) is one write. */
const SAVE_MS = 200;
/** After the last change to what this window lists or shows. */
const REPORT_MS = 100;

/** The order of the last change to the app's part of the state this window has heard. */
let seq = 0;
/** The app's part of the state as the app has it, as far as this window knows. */
let lastShared: Shared;
/** This window's own part as it was last kept, as text. */
let savedWindow = "";
let saveTimer: ReturnType<typeof setTimeout> | undefined;
let reportTimer: ReturnType<typeof setTimeout> | undefined;
let reported = "";
let started = false;

/** What the app tells every window when it changes the app's part of the state. */
interface SharedEvent {
  seq: number;
  from: string;
  ops: unknown;
}

/** One call to the app at a time, in the order made: changes told out of order are not the changes made. */
let chain: Promise<unknown> = Promise.resolve();
function inOrder(job: () => Promise<unknown>): Promise<unknown> {
  chain = chain.then(job, job).catch(() => {});
  return chain;
}

// ------------------------------------------------------------------ keeping the state

/** Keep what changed: this window's own part, and the changes to the app's. */
async function keep(): Promise<void> {
  const state = gui.peek();
  const own = windowOf(state);
  const text = JSON.stringify(own);
  if (text !== savedWindow) {
    savedWindow = text;
    await api.windowSave(own).catch(() => {
      savedWindow = "";
    });
  }
  const ops = diffShared(lastShared, sharedOf(state));
  if (ops.length === 0) return;
  lastShared = applyOps(lastShared, ops);
  await api.sharedApply(ops).catch(() => {
    // Not told: told again, a moment on.
    setTimeout(() => void inOrder(() => api.sharedApply(ops).catch(() => {})), 1000);
  });
}

function scheduleKeep(): void {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => void inOrder(keep), SAVE_MS);
}

/** Keep it now, and wait until it is kept. */
export function keepNow(): Promise<unknown> {
  clearTimeout(saveTimer);
  return inOrder(keep);
}

/**
 * The state with the app's part as another window left it. The workspaces this
 * window had put away and another went to are no longer ours to remember —
 * they are listed there —, and tabs of a daemon of another time are not tabs.
 */
function takeShared(state: GuiState, before: Shared, next: Shared): GuiState {
  let merged = withShared(state, next);
  const order = forgetTaken(merged.order, before.hidden, next.hidden);
  if (order.length !== merged.order.length) merged = { ...merged, order };
  if (next.daemon !== before.daemon) merged = { ...merged, active: null };
  return merged;
}

/** Changes another window made (or this one did, told back): made here too. */
function apply(ops: ReturnType<typeof readOps>): void {
  if (ops.length === 0) return;
  const state = gui.peek();
  const before = sharedOf(state);
  lastShared = applyOps(lastShared, ops);
  const next = applyOps(before, ops);
  if (next !== before) gui.value = takeShared(state, before, next);
}

function heard(event: SharedEvent): void {
  if (event.seq <= seq) return;
  // One was missed: the app is asked for everything, and the changes it tells from here on are on top of it.
  if (event.seq > seq + 1) {
    void resync();
    return;
  }
  seq = event.seq;
  apply(readOps(event.ops));
}

async function resync(): Promise<void> {
  const fresh = await api.windowBoot().catch(() => null);
  if (!fresh) return;
  peers.value = readPeers(fresh.windows);
  if (fresh.seq <= seq) return;
  seq = fresh.seq;
  const next = sharedOf(joinState(fresh.shared, null));
  lastShared = next;
  const state = gui.peek();
  gui.value = takeShared(state, sharedOf(state), next);
}

// ------------------------------------------------------------------ saying what this window shows

function scheduleReport(placement: Placement): void {
  clearTimeout(reportTimer);
  reportTimer = setTimeout(() => {
    const text = JSON.stringify(placement);
    if (text === reported) return;
    reported = text;
    void api.windowsReport(placement).catch(() => {
      reported = "";
    });
  }, REPORT_MS);
}

/** Another window handed a tab to this one: it is in front here. */
function showTab(tab: { workspace: string; tab: number }): void {
  if (typeof tab?.workspace !== "string" || !Number.isInteger(tab.tab)) return;
  select(tab.workspace, tab.tab);
}

// ------------------------------------------------------------------ start

/**
 * From here on this window keeps its state and the app's, hears the other
 * windows and is heard. After the state is loaded, before the daemon is asked.
 */
export async function startWindows(boot: api.WindowBoot): Promise<void> {
  if (started) return;
  started = true;
  seq = boot.seq;
  lastShared = sharedOf(joinState(boot.shared, null));
  savedWindow = boot.window === null ? "" : JSON.stringify(windowOf(gui.peek()));
  await Promise.all([
    listen<SharedEvent>("keep://shared", (event) => heard(event.payload)),
    listen<unknown>("keep://windows", (event) => {
      peers.value = readPeers(event.payload);
    }),
    listen<{ workspace: string; tab: number }>("keep://show-tab", (event) => showTab(event.payload)),
  ]);
  // What was told between the answer the app gave at the start and the listening.
  await resync();
  effect(() => {
    void gui.value;
    scheduleKeep();
  });
  effect(() => {
    const workspace = activeWorkspace.value;
    const tab = activeTab.value;
    scheduleReport({ carries: carriedHere.value, showing: workspace && tab ? { workspace, tab: tab.root.id } : null });
  });
  // With other windows open, a terminal out of sight here may be one another window looks at.
  effect(() => terminals.setSeveralWindows(severalWindows.value));
  registerCommand("newWindow", { run: () => void openNewWindow() });
}

// ------------------------------------------------------------------ opening windows

/** The window as it is on the screen, for a new one to be put beside it. */
async function whereThisIs(): Promise<{ at: Point; size: { width: number; height: number }; scale: number; maximized: boolean }> {
  const win = getCurrentWindow();
  const [position, size, scale, maximized, fullscreen] = await Promise.all([
    win.outerPosition(),
    win.innerSize(),
    win.scaleFactor(),
    win.isMaximized(),
    win.isFullscreen(),
  ]);
  return { at: { x: position.x, y: position.y }, size: { width: size.width, height: size.height }, scale, maximized: maximized || fullscreen };
}

/**
 * Open a window, on purpose: the size of this one, stepped down and across,
 * with no workspace in it — every one there is a Go To away. The label it was
 * given.
 */
export async function openNewWindow(): Promise<string | null> {
  try {
    const here = await whereThisIs();
    const frame = cascaded({ ...here.at, ...here.size }, here.scale);
    return await api.windowOpen({
      frame: here.maximized ? undefined : frame,
      maximized: here.maximized,
      placement: emptyPlacement(),
    });
  } catch (error) {
    say(`Não deu para abrir a janela: ${String(error)}`);
    return null;
  }
}

/** The numbers of a frame, if it has them: a maximized window with nothing before it has only that. */
function frameOf(saved: api.WindowFrame | null): api.WindowFrame | undefined {
  if (!saved || ![saved.x, saved.y, saved.width, saved.height].every(Number.isFinite)) return undefined;
  return { x: saved.x, y: saved.y, width: saved.width, height: saved.height };
}

/**
 * Open the windows of the last run, each with the workspaces and the tab it had
 * and where it was on the screen — if a monitor of that place is still here
 * (src-tauri decides). Asked of the first window, once the daemon has its tabs:
 * `remap` carries the tab each was left on to the number it has now.
 */
export async function openSavedWindows(boot: api.WindowBoot, remap: (active: GuiState["active"]) => GuiState["active"]): Promise<void> {
  if (windowLabel.value !== MAIN_LABEL || boot.saved.length === 0) return;
  // The app's part first: the windows read it when they start.
  await keepNow();
  const shared = sharedOf(gui.peek());
  for (const saved of boot.saved) {
    const state = joinState(shared, saved.window);
    const active = remap(state.active);
    const record = { ...windowOf(state), active };
    const frame = frameOf(saved.frame);
    try {
      await api.windowOpen({
        label: saved.label,
        frame,
        maximized: saved.frame?.maximized === true,
        placement: {
          carries: carriedOf(state.order, state.hidden),
          showing: active && active.tab >= 0 ? { workspace: active.workspace, tab: active.tab } : null,
        } satisfies Placement,
        record,
      });
    } catch (error) {
      log(`janela ${saved.label}: não abriu: ${String(error)}`);
    }
  }
}

// ------------------------------------------------------------------ a tab pulled out of the row

/** Leave a tab to the window it went to: this one carries on with the tab beside it, if it was the one showing it. */
function stepOff(workspace: string, root: number): void {
  if (activeWorkspace.value !== workspace || activeTab.value?.root.id !== root) return;
  const tabs = tabsByWorkspace.value.get(workspace) ?? [];
  const at = tabs.findIndex((t) => t.root.id === root);
  const next = tabs[at + 1] ?? tabs[at - 1];
  if (next) select(workspace, next.root.id);
}

/**
 * A tab pulled out of the row and let go: nothing about the session changes —
 * the tab is running, it stays in its workspace and in the row it came from.
 * What moves is which window looks at it: the one it was let go over, or a
 * new one made for it if it was let go over nothing; and this window carries
 * on with the tab beside it. `press` and `release` are where the pointer was
 * pressed and let go, in this window.
 */
export async function tearOffTab(workspace: string, tab: RootTab, press: Point, release: Point): Promise<void> {
  try {
    const win = getCurrentWindow();
    const [inner, scale, size] = await Promise.all([win.innerPosition(), win.scaleFactor(), win.innerSize()]);
    const cursor = { x: Math.round(inner.x + release.x * scale), y: Math.round(inner.y + release.y * scale) };
    const over = await api.windowUnder(cursor.x, cursor.y, windowLabel.value).catch(() => null);
    const root = tab.root.id;
    if (over) {
      await emitTo(over, "keep://show-tab", { workspace, tab: root });
      await api.windowShow(over);
    } else {
      const own = windowOf(gui.peek());
      await api.windowOpen({
        frame: frameOfTornTab(cursor, press, scale, { width: size.width, height: size.height }),
        placement: { carries: [workspace], showing: { workspace, tab: root } } satisfies Placement,
        record: { ...own, order: [workspace], active: { workspace, tab: root } },
      });
    }
    stepOff(workspace, root);
  } catch (error) {
    say(`Não deu para abrir a janela: ${String(error)}`);
  }
}
