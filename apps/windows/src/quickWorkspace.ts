// The daemon's workspace the drop-down terminal (quick/boot.tsx, quick.rs)
// lives in: a workspace like any other to the daemon, and none to the window.
// The Mac keeps it out of every window's list and out of the restore and the
// orphans' sweep (Session.quickWorkspace, retrato.rs RAPIDO); here it is kept
// out where the window learns what the daemon holds, so that nothing after
// that — the sidebar, Go To, the palette, the search, what the state remembers
// — ever sees it.

export const QUICK_WORKSPACE = "quick";

export const isQuickWorkspace = (name: string): boolean => name === QUICK_WORKSPACE;

/** The workspaces as the window shows them: the daemon's, without the drop-down terminal's. */
export function withoutQuick<T extends { name: string }>(list: readonly T[]): T[] {
  return list.filter((w) => !isQuickWorkspace(w.name));
}

/** The same for what a search found, which looks in every workspace the daemon has. */
export function withoutQuickHits<T extends { workspace: string }>(hits: readonly T[]): T[] {
  return hits.filter((h) => !isQuickWorkspace(h.workspace));
}
