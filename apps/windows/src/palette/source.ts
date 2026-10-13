// The window's facts, gathered for the picker's lists (items.ts decides what
// they make). These read signals: called where a list is drawn, they make it
// follow the tabs as they change.

import type { SearchHit } from "../api";
import { programOf } from "../claude";
import { recentTabs } from "../recent";
import {
  activePane,
  activeTab,
  activeWorkspace,
  info,
  tabBusy,
  tabLabel,
  tabsByWorkspace,
  workspaceLabel,
  workspaces,
} from "../store";
import { hitItem, type GoToInput, type PickerItem, type WorkspaceFacts } from "./items";
import type { PreviewTarget } from "./preview";

/** Every workspace and its tabs, as the daemon lists them (a workspace the sidebar does not show is a place to go all the same). */
export function workspaceFacts(): WorkspaceFacts[] {
  return workspaces.value.map((w) => {
    const tabs = tabsByWorkspace.value.get(w.name) ?? [];
    return {
      name: w.name,
      label: workspaceLabel(w.name),
      place: tabs.find((t) => t.root.cwd)?.root.cwd ?? "",
      tabs: tabs.map((tab, position) => ({
        workspace: w.name,
        tab: tab.root.id,
        position,
        title: tabLabel(w.name, tab, position),
        panes: tab.panes.length,
        program: programOf(tab.root.command, tab.root.title),
        cwd: tab.root.cwd,
        busy: tabBusy(w.name, tab),
        // The latest of its panes: a split that is the one working keeps the tab recent.
        lastActive: Math.max(0, ...tab.panes.map((p) => p.lastActive)),
      })),
    };
  });
}

/** The tab in front, which is not a place to go. */
export function hereFacts(): GoToInput["here"] {
  const workspace = activeWorkspace.value;
  const tab = activeTab.value;
  return workspace && tab ? { workspace, tab: tab.root.id } : null;
}

export const visits = (): readonly string[] => recentTabs.value;

export const homeFolder = (): string => info.value?.home ?? "";

/**
 * Hits as rows, each named by the tab that holds it as the person knows it.
 * A hit is found by pane and a pane is not a tab: going to one means opening
 * its tab and focusing the pane. One whose tab went away since the search is
 * left out — there is nowhere to go.
 */
export function hitRows(hits: readonly SearchHit[]): PickerItem[] {
  const rows: PickerItem[] = [];
  for (const hit of hits) {
    const tabs = tabsByWorkspace.value.get(hit.workspace) ?? [];
    const index = tabs.findIndex((t) => t.panes.some((p) => p.id === hit.tab));
    if (index < 0) continue;
    rows.push(hitItem(hit, tabs[index].root.id, `${workspaceLabel(hit.workspace)} › ${tabLabel(hit.workspace, tabs[index], index)}`));
  }
  return rows;
}

/** What a search of the pane in front looks in: where it is, and what to call it. Null with no pane in front. */
export function paneScope(): { workspace: string; pane: number; label: string } | null {
  const workspace = activeWorkspace.value;
  const tab = activeTab.value;
  const pane = activePane.value;
  if (!workspace || !tab || !pane) return null;
  const index = (tabsByWorkspace.value.get(workspace) ?? []).indexOf(tab);
  return { workspace, pane: pane.id, label: `${workspaceLabel(workspace)} › ${tabLabel(workspace, tab, Math.max(0, index))}` };
}

/** The screen a row stands for, if it stands for one: a tab's, or the pane a line of history is in. */
export function previewTarget(item: PickerItem | undefined): PreviewTarget | null {
  const kind = item?.kind;
  if (kind?.type === "running") {
    const tab = (tabsByWorkspace.value.get(kind.workspace) ?? []).find((t) => t.root.id === kind.tab);
    return tab ? { workspace: kind.workspace, pane: tab.root.id, cols: tab.root.cols, rows: tab.root.rows } : null;
  }
  if (kind?.type === "hit") {
    const tab = (tabsByWorkspace.value.get(kind.hit.workspace) ?? []).find((t) => t.root.id === kind.root);
    const pane = tab?.panes.find((p) => p.id === kind.hit.tab);
    return pane ? { workspace: kind.hit.workspace, pane: pane.id, cols: pane.cols, rows: pane.rows, hit: { ...kind.hit, span: kind.span } } : null;
  }
  return null;
}
