// Which windows the app has open, as this page knows them. Signals only, so
// that the store and the parts of the page that depend on the other windows
// can read them without depending on the code that keeps them up to date
// (windowSync.ts).

import { computed, signal } from "@preact/signals";
import { MAIN_LABEL, leaderOf, type Peer } from "./windowModel";

/** This window: `main`, `w1`… Until the page has asked, the first. */
export const windowLabel = signal(MAIN_LABEL);

/** The windows that are open, oldest first, this one among them, with what each last said of itself. */
export const peers = signal<readonly Peer[]>([]);

/**
 * The oldest window. It takes the workspaces nobody lists and does what the
 * app does once — the restore after a restart, the core's rounds that change
 * something. A window that does not know the others yet is alone.
 */
export const isLeader = computed(() => {
  const leader = leaderOf(peers.value);
  return leader === null || leader === windowLabel.value;
});

/** More than one window is open: a terminal out of sight here may be the one another window looks at. */
export const severalWindows = computed(() => peers.value.length > 1);
