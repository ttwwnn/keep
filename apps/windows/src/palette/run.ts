// What choosing a row does, and what the actions panel's entries do to it.
//
// Two kinds of thing, kept apart on purpose, as the macOS app keeps them.
// Going somewhere and closing something are facts about the window's tabs.
// Showing a folder and copying a path change nothing here — they hand a string
// to another program — so they are done on the spot.

import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { baseName } from "../layout";
import { catalogOf, commandSpec, isEnabled, runCommand, type CatalogId } from "../commands";
import { closeTab, createWorkspace, focusPane, newTab, say, select, tabLabel, tabsByWorkspace } from "../store";
import type { ActionId } from "./actions";
import type { PickerItem } from "./items";
import { closeOverlay } from "./overlay";
import { revealHit } from "./reveal";

/** Why a thing the person asked for did not happen, said where the sidebar says things. */
const failed = (what: string) => (error: unknown) => say(`${what}: ${String(error)}`);

/**
 * Choose a row: go there, open the folder, make the workspace, run the
 * command. A command that opens a list the palette has keeps the card up and
 * says which (`openList`); every other choice puts the card away first, so what
 * it does — a window opened, a question asked — does not happen under it.
 */
export function choose(item: PickerItem, openList: (list: CatalogId) => void): void {
  const kind = item.kind;
  switch (kind.type) {
    case "command": {
      const spec = commandSpec(kind.command);
      if (!isEnabled(spec.id)) return;
      if (spec.opens && catalogOf(spec.opens)) {
        openList(spec.opens);
        return;
      }
      closeOverlay();
      runCommand(spec.id);
      return;
    }
    case "running":
      closeOverlay();
      select(kind.workspace, kind.tab);
      return;
    case "destination":
      closeOverlay();
      void createWorkspace(baseName(kind.path), kind.path).catch(failed("Não deu para abrir a pasta"));
      return;
    case "create":
      closeOverlay();
      void createWorkspace(kind.name, null).catch(failed("Não deu para criar o workspace"));
      return;
    case "hit": {
      // Land on the tab, then on the pane inside it, then on the line.
      closeOverlay();
      select(kind.hit.workspace, kind.root);
      focusPane(kind.hit.workspace, kind.root, kind.hit.tab);
      revealHit(kind.hit.workspace, kind.hit.tab, { ...kind.hit, span: kind.span });
      return;
    }
  }
}

/**
 * End the tab a row points at, after asking — with the note on the worktrees
 * that go with it, as closing it anywhere else says. Resolves once it is
 * answered; the card stays up, and its list follows the tabs.
 */
export async function dismiss(item: PickerItem): Promise<void> {
  if (item.kind.type !== "running") return;
  const { workspace, tab } = item.kind;
  const tabs = tabsByWorkspace.value.get(workspace) ?? [];
  const index = tabs.findIndex((t) => t.root.id === tab);
  if (index < 0) return;
  await closeTab(workspace, tabs[index], tabLabel(workspace, tabs[index], index));
}

/** One of the actions panel's entries, on the row it was opened over. */
export async function act(action: ActionId, item: PickerItem, openList: (list: CatalogId) => void): Promise<void> {
  switch (action) {
    case "open":
      choose(item, openList);
      return;
    case "close":
      await dismiss(item);
      return;
    case "newTab":
      closeOverlay();
      await newTab(item.workspace || undefined, item.path || undefined).catch(failed("Não deu para abrir a aba"));
      return;
    case "reveal":
      if (!item.path) return;
      closeOverlay();
      await revealItemInDir(item.path).catch(failed("Não deu para mostrar a pasta"));
      return;
    case "copyPath":
      if (!item.path) return;
      closeOverlay();
      await writeText(item.path).catch(failed("Não deu para copiar"));
      return;
    case "copyText":
      if (item.kind.type !== "hit") return;
      closeOverlay();
      await writeText(item.kind.hit.text).catch(failed("Não deu para copiar"));
      return;
  }
}
