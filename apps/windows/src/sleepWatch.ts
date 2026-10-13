// Puts hibernation.ts to work: the tab in front is woken, the tabs shown least
// recently are put to sleep a moment after the last one was shown, and the
// ones held on to meanwhile — a selection, the keyboard — are asked about
// again every quarter of a minute.

import { computed, effect, untracked } from "@preact/signals";
import { DEFAULT_LIMIT, awake, forget, holds, lastShown, sleep, sleepers, wake, type Candidate } from "./hibernation";
import { tabKey } from "./state";
import { activeTab, activeWorkspace, tabsByWorkspace, terminals } from "./store";

/** After a tab is shown: going through tabs one after the other lets go of nothing until it stops. */
const SETTLE_MS = 1000;
const RETRY_MS = 15000;

let pending: ReturnType<typeof setTimeout> | undefined;

/** Put to sleep what the policy picks. */
export function settle(limit = DEFAULT_LIMIT): string[] {
  // A pane being carried can land on any tab.
  if (holds.value > 0) return [];
  const candidates: Candidate[] = [];
  const keys: string[] = [];
  const front = activeWorkspace.value;
  for (const [workspace, tabs] of tabsByWorkspace.value) {
    for (const tab of tabs) {
      const key = tabKey(workspace, tab.root.id);
      if (!awake.value.has(key)) continue;
      keys.push(key);
      candidates.push({
        visible: workspace === front && tab.root.id === activeTab.value?.root.id,
        lastShown: lastShown(key),
        blocked: tab.panes.some((p) => terminals.existing(workspace, p.id)?.holdsUnsavedState() === true),
      });
    }
  }
  const chosen = sleepers(candidates, limit).map((i) => keys[i]);
  // The panes' hosts let go of their terminals as they see the tab is not awake.
  sleep(chosen);
  return chosen;
}

export function watchSleep(): void {
  // Changes only when the tab in front does, not whenever the daemon is asked.
  const frontKey = computed(() => {
    const workspace = activeWorkspace.value;
    const tab = activeTab.value;
    return workspace && tab ? tabKey(workspace, tab.root.id) : "";
  });
  // Untracked: what these do to who is awake is not a reason for them to run again.
  effect(() => {
    const key = frontKey.value;
    if (!key) return;
    untracked(() => {
      wake(key);
      clearTimeout(pending);
      pending = setTimeout(() => settle(), SETTLE_MS);
    });
  });
  const alive = computed(() =>
    [...tabsByWorkspace.value].flatMap(([workspace, tabs]) => tabs.map((t) => tabKey(workspace, t.root.id))).join("\u0001"),
  );
  effect(() => {
    const keys = alive.value;
    untracked(() => forget(new Set(keys.split("\u0001"))));
  });
  setInterval(() => settle(), RETRY_MS);
}
