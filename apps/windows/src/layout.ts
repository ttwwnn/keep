// How the daemon's flat list of tabs becomes what the window shows: tabs, and
// inside each the tree its panes make. A port of PaneTree in the macOS app —
// the daemon records each pane's parent and direction, and replaying those
// records in creation order rebuilds the exact shape.

import type { TabInfo } from "./api";
import { SPLIT_DOWN } from "./api";

export type PaneTree =
  | { kind: "leaf"; id: number }
  /** `vertical` is the divider's orientation: a vertical divider is side by side. */
  | { kind: "split"; vertical: boolean; first: PaneTree; second: PaneTree };

export interface PaneRecord {
  tab: number;
  splitOf: number;
  splitDir: number;
}

export function buildTree(root: number, panes: PaneRecord[]): PaneTree {
  let tree: PaneTree = { kind: "leaf", id: root };
  for (const pane of panes) {
    tree = replaceLeaf(tree, pane.splitOf, {
      kind: "split",
      vertical: pane.splitDir !== SPLIT_DOWN,
      first: { kind: "leaf", id: pane.splitOf },
      second: { kind: "leaf", id: pane.tab },
    });
  }
  return tree;
}

function replaceLeaf(tree: PaneTree, target: number, subtree: PaneTree): PaneTree {
  if (tree.kind === "leaf") return tree.id === target ? subtree : tree;
  return {
    kind: "split",
    vertical: tree.vertical,
    first: replaceLeaf(tree.first, target, subtree),
    second: replaceLeaf(tree.second, target, subtree),
  };
}

export function leaves(tree: PaneTree): number[] {
  return tree.kind === "leaf" ? [tree.id] : [...leaves(tree.first), ...leaves(tree.second)];
}

/** One tab as the strip shows it: its root, and the panes that hang off it. */
export interface RootTab {
  root: TabInfo;
  /** Every pane of the tab, the root first, in the order the tree lists them. */
  panes: TabInfo[];
  tree: PaneTree;
}

/**
 * The tabs of a workspace, in the daemon's order, each with its panes.
 *
 * A pane belongs to the root at the top of its chain of parents. The daemon
 * already reports a pane whose parent is gone as a tab of its own, so every
 * chain ends at a root; the walk is bounded all the same.
 */
export function rootTabs(tabs: TabInfo[]): RootTab[] {
  const byId = new Map(tabs.map((t) => [t.id, t]));
  const rootOf = (tab: TabInfo): number => {
    let current = tab;
    for (let hops = 0; hops <= tabs.length; hops++) {
      if (current.splitOf === 0) return current.id;
      const parent = byId.get(current.splitOf);
      if (!parent) return current.id;
      current = parent;
    }
    return current.id;
  };
  const roots = tabs.filter((t) => t.splitOf === 0 || !byId.has(t.splitOf));
  return roots.map((root) => {
    const members = tabs.filter((t) => t.id !== root.id && rootOf(t) === root.id);
    const tree = buildTree(
      root.id,
      members.map((m) => ({ tab: m.id, splitOf: m.splitOf, splitDir: m.splitDir })),
    );
    const order = leaves(tree);
    const panes = order.map((id) => byId.get(id)).filter((t): t is TabInfo => t !== undefined);
    return { root, panes, tree };
  });
}

/**
 * A title the program set that says nothing: ConPTY titles every console
 * with the path of the program it started (`C:\…\powershell.exe`).
 */
export function isPathTitle(title: string): boolean {
  return /^[a-z]:\\.*\.(exe|com|bat|cmd)$/i.test(title.trim()) || /\\[^\\]+\.exe$/i.test(title.trim());
}

/** The last directory of a path, either separator. */
export function baseName(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}
