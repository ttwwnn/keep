// The core's chores that nobody asks for, on their clocks — the macOS app's
// `AIChores` for the restart and for the conversations a drop stopped:
//
//  - the picture of the tabs (`keep restauro manter`, every 30 s and at once on
//    opening): the conversations still to come back after a restart, one at a
//    time, and a new picture for the next restart;
//  - the conversations a dropped login, network or usage limit stopped (`keep
//    continuar`, every 15 s): they are told to continue when the cause is gone;
//  - the conversations left alone for twelve hours out of sight (`keep restauro
//    adormecer`, every 5 min): they leave memory, each tab kept with its shell,
//    and come back as the tab is shown (`keep restauro acordar`);
//  - the names given to tabs, which the core keeps in a file of its own
//    (names.ts), so that a restart gives them back and `keep nova --nome`
//    works.
//
// The tabs back after a restart (`restoreTabs`) is the first phase of the
// same, which the window waits for before it draws anything of a daemon that
// has just started (store.ts `connect`).
//
// The core decides and does everything (docs/restauro.md); this only asks, one
// at a time each, while the daemon answers, and says in the log what is worth
// saying.

import { computed, effect, untracked } from "@preact/signals";
import * as api from "./api";
import { daemonStartOf, planNames, type NamesFile } from "./names";
import {
  dormancyLine,
  isQuietResume,
  isQuietRestore,
  readResume,
  readRestore,
  restoreLine,
  resumeLine,
  sleepArgs,
  type Restored,
  type Shown,
} from "./restauro";
import { activeTab, activeWorkspace, daemonError, gui, loaded, log, renameTab } from "./store";
import { isLeader, peers, windowLabel } from "./windowPeers";

const daemonUp = () => loaded.value && daemonError.value === null;

/** Whether the core is here to be asked (a window without it has no AI layer, and nothing to restore with). */
async function coreHere(): Promise<boolean> {
  return (await api.iaInfo().catch(() => null))?.available === true;
}

// ------------------------------------------------------------------ the tabs back after a restart

/**
 * The tabs the last daemon had, made again in this one (`keep restauro
 * restaurar --fase=abas`: each in its folder, the panes beside the tab they
 * were split from, the names given, nothing run again yet); which came back,
 * under what numbers, or null when none did. The core knows by itself whether
 * there is anything to restore and does it once per daemon, so asking an empty
 * daemon is always safe.
 */
export async function restoreTabs(): Promise<Restored | null> {
  if (!(await coreHere())) return null;
  let restored: Restored | null = null;
  try {
    const round = readRestore(await api.iaAsk(["restauro", "restaurar", "--fase=abas"]));
    if (!isQuietRestore(round)) log(restoreLine(round));
    if (round.tabs > 0) restored = { made: round.made };
  } catch (error) {
    log(`restauro: ${String(error)}`);
  }
  // The names the core gave them, before the window shows them.
  if (restored) await syncNames().catch(() => {});
  return restored;
}

let keeping = false;

/**
 * The conversations still to come back after a restart, and a new picture of
 * the tabs for the next one (`keep restauro manter`). Slow when conversations
 * come back: one at a time, each waiting for the one before to come up.
 */
async function keepPicture(always: boolean): Promise<void> {
  if (keeping || !isLeader.value || (!always && !daemonUp())) return;
  keeping = true;
  try {
    const round = readRestore(await api.iaAsk(["restauro", "manter"]));
    if (!isQuietRestore(round)) log(restoreLine(round));
  } catch (error) {
    log(`restauro manter: ${String(error)}`);
  } finally {
    keeping = false;
  }
}

// ------------------------------------------------------------------ the conversations a drop stopped

let resuming = false;

/**
 * The conversations that stopped on a dropped login, network or usage limit,
 * told to continue once it is back (`keep continuar`). A round that types
 * waits up to 30 s for the message to reach the transcript; its own log and
 * state are beside the pictures.
 */
async function resumeStopped(): Promise<void> {
  if (resuming || !isLeader.value || !daemonUp()) return;
  resuming = true;
  try {
    const round = readResume(await api.iaAsk(["continuar"]));
    if (!isQuietResume(round)) log(resumeLine(round));
  } catch (error) {
    log(`continuar: ${String(error)}`);
  } finally {
    resuming = false;
  }
}

// ------------------------------------------------------------------ the conversations left alone

/** The tab this window shows. */
function shownHere(): Shown | null {
  const workspace = activeWorkspace.value;
  const tab = activeTab.value?.root.id;
  return workspace && tab !== undefined ? { workspace, tab } : null;
}

let sleeping = false;

/**
 * The conversations left alone for twelve hours out of sight leave memory, each tab kept with its shell (`keep
 * restauro adormecer`): never the tab this window or another one shows.
 */
async function sleepIdle(): Promise<void> {
  if (sleeping || !isLeader.value || !daemonUp()) return;
  sleeping = true;
  try {
    const others = peers.value.filter((p) => p.label !== windowLabel.value).flatMap((p) => (p.placement.showing ? [p.placement.showing] : []));
    const here = shownHere();
    const line = dormancyLine(await api.iaAsk(sleepArgs(here ? [here, ...others] : others)));
    if (line) log(line);
  } catch (error) {
    log(`adormecer: ${String(error)}`);
  } finally {
    sleeping = false;
  }
}

/** The tab this window shows, as text: the same tab is the same string, and a poll that rebuilds the tabs wakes nothing. */
const shownKey = computed(() => {
  const shown = shownHere();
  return shown ? `${shown.workspace}/${shown.tab}` : null;
});

/** The conversation of a tab put to sleep comes back in it as the tab is shown (`keep restauro acordar`). */
function wakeShown(): void {
  effect(() => {
    if (shownKey.value === null || !daemonUp()) return;
    const shown = untracked(shownHere);
    if (!shown) return;
    void api
      .iaAsk(["restauro", "acordar", "--workspace", shown.workspace, "--aba", String(shown.tab)])
      .then((answer) => {
        const line = dormancyLine(answer);
        if (line) log(line);
      })
      .catch((error) => log(`acordar: ${String(error)}`));
  });
}

// ------------------------------------------------------------------ the names

/** The version of the names file as the window last read or wrote it, and what it said then. */
let seen: string | null = null;
let known: NamesFile | null = null;
let syncing = false;

/**
 * Names in both directions, true for this daemon only (names.ts): the ones the
 * core put on tabs since the window last looked (`keep nova --nome`, the tabs a
 * restart brought back) become the window's, and the window's become the
 * file's, for the picture of the tabs to carry them. Where the daemon cannot
 * say when it started, there is nothing to stamp the names with, and nothing
 * happens.
 */
export async function syncNames(): Promise<void> {
  const start = daemonStartOf(gui.value.daemon);
  if (start === null || syncing || !isLeader.value) return;
  syncing = true;
  try {
    const read = await api.namesRead(seen);
    if (read.text !== null) seen = read.stamp;
    const plan = planNames({ state: gui.value, daemonStart: start, text: read.text, known });
    for (const named of plan.adopt) renameTab(named.workspace, named.tab, named.name);
    for (const named of plan.drop) renameTab(named.workspace, named.tab, "");
    known = plan.known;
    if (plan.write) {
      seen = await api.namesWrite(JSON.stringify(plan.write));
      known = plan.write;
    }
  } finally {
    syncing = false;
  }
}

// ------------------------------------------------------------------ start

let started = false;

/**
 * Start the clocks; the first round of the picture is now, so that a restart's conversations come back at once.
 * With several windows open the rounds that change something are the oldest window's alone (windowPeers.ts).
 */
export function startChores(): void {
  if (started) return;
  started = true;
  void keepPicture(true);
  setInterval(() => void keepPicture(false), 30_000);
  setInterval(() => void resumeStopped(), 15_000);
  setInterval(() => void sleepIdle(), 300_000);
  wakeShown();
  setInterval(() => void syncNames().catch(() => {}), 2_000);
}
