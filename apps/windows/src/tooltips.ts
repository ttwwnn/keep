// What a workspace's header says when the pointer rests on it: the name it
// answers to when it was renamed, and then what runs in it — or, when nothing
// does, how many tabs it has and what state they are in. The row itself shows
// one name; this is where the rest live (WorkspaceSidebar.tooltip).

export type WorkspaceState = "empty" | "running" | "attached" | "idle";

const STATE_WORDS: Record<WorkspaceState, string> = {
  empty: "vazio",
  running: "rodando",
  attached: "anexado",
  idle: "ocioso",
};

/** What the tabs of a workspace add up to: running wins over attached, which wins over idle. */
export function workspaceState(tabs: readonly { busy: boolean; clients: number }[]): WorkspaceState {
  if (tabs.length === 0) return "empty";
  if (tabs.some((t) => t.busy)) return "running";
  return tabs.some((t) => t.clients > 0) ? "attached" : "idle";
}

export function workspaceTooltip(input: {
  /** The name the daemon, `keep` and every shell know it by. */
  name: string;
  /** The name the sidebar shows. */
  label: string;
  tabs: number;
  state: WorkspaceState;
  /** The title of everything running in it, panes included. */
  running: readonly string[];
}): string {
  const known = input.label === input.name ? [] : [input.name];
  if (input.running.length === 0) {
    const count = input.tabs === 1 ? "1 aba" : `${input.tabs} abas`;
    return [...known, `${count} · ${STATE_WORDS[input.state]}`].join("\n");
  }
  const heading = input.running.length === 1 ? "1 rodando" : `${input.running.length} rodando`;
  return [...known, heading, ...input.running.map((title) => `· ${title}`)].join("\n");
}
