// The workspaces taken out of the sidebar ("put away", "remove from this
// window"): still running, in no list, a Go To away from being listed again.
// What leaves is a place in the list, not the work — "Fechar o workspace" is
// the one that ends it — and a relaunch leaves them out as they were left.

/** The workspaces put away, with this one added. */
export function putAwayWith(away: readonly string[], workspace: string): string[] {
  return [...away.filter((name) => name !== workspace), workspace];
}

/** The workspaces put away, once this one is gone to: back in the list. */
export function putAwayWithout(away: readonly string[], workspace: string): string[] {
  return away.filter((name) => name !== workspace);
}

/**
 * Those put away that still run. A workspace somebody ended is gone for good
 * — one made later under its name is a new one, and must not be born out of
 * sight —, so it does not stay in the list.
 */
export function stillRunning(away: readonly string[], running: ReadonlySet<string>): string[] {
  return away.filter((name) => running.has(name));
}

/** The workspaces a list shows: all of them but the ones put away. */
export function listed<T extends { name: string }>(workspaces: readonly T[], away: readonly string[]): T[] {
  const hidden = new Set(away);
  return workspaces.filter((w) => !hidden.has(w.name));
}

/**
 * Where to go when the workspace in front is put away: the one after it in
 * the list, else the one before, rather than stay pointed at something the
 * list no longer has.
 */
export function neighbour(names: readonly string[], leaving: string): string | undefined {
  const at = names.indexOf(leaving);
  return at < 0 ? undefined : (names[at + 1] ?? names[at - 1]);
}
