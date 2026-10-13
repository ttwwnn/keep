// Which tabs the person has been in, latest first: what the picker lists
// after the tab in front.

import { signal } from "@preact/signals";
import { tabKey } from "./state";

/** The most visits remembered; a tab that closed leaves its key until it is pushed out. */
const LIMIT = 500;

/** `tabKey`s, the latest visit first. In memory only: after a restart the daemon's `lastActive` orders the list. */
export const recentTabs = signal<readonly string[]>([]);

/** `visits` with `key` brought to the front; the same list when it already is there. */
export function touch(visits: readonly string[], key: string, limit = LIMIT): readonly string[] {
  if (visits[0] === key) return visits;
  return [key, ...visits.filter((k) => k !== key)].slice(0, limit);
}

/** The person went to this tab. */
export function noteVisit(workspace: string, tab: number): void {
  recentTabs.value = touch(recentTabs.value, tabKey(workspace, tab));
}
