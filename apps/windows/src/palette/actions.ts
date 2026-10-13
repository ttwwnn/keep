// What can be done to a row of the picker, beyond going to it: the list the
// actions panel (Ctrl+K, or the right button) shows. A short, closed list on
// purpose — everything in it is something the row itself already implies: where
// it is, what it is holding.

import { commandSpec } from "../commands";
import type { GlyphName } from "./glyphs";
import type { PickerItem } from "./items";

export type ActionId =
  /** What Enter already does. In the list anyway, because a menu of what can be done to a row that leaves out the main thing reads as a menu of afterthoughts. */
  | "open"
  /** A new tab in the workspace the row belongs to. */
  | "newTab"
  | "reveal"
  | "copyPath"
  /** A line of history, as text. */
  | "copyText"
  /** End what the row points at. */
  | "close";

export interface ActionEntry {
  action: ActionId;
  title: string;
  icon: GlyphName;
  /** The keys that do the same without opening the panel, drawn as caps; empty for an action reachable only from here. */
  keys: readonly string[];
  /** Said in red, and last: the one entry that does not undo. */
  destructive?: boolean;
  /** A rule above it, separating it from what came before. */
  startsGroup?: boolean;
}

const OPEN = ["↩"];
const REVEAL = ["Ctrl", "Shift", "R"];
const COPY = ["Ctrl", "Shift", "C"];

/**
 * What can be done to this row. Decided from its kind, because that is what
 * the actions differ over: a terminal can be closed and a folder cannot, a
 * folder can be shown in the Explorer and a line of history has no folder to
 * show.
 */
export function actionsFor(item: PickerItem): ActionEntry[] {
  switch (item.kind.type) {
    case "running": {
      const entries: ActionEntry[] = [
        { action: "open", title: "Ir para este terminal", icon: "go", keys: OPEN },
        {
          action: "newTab",
          title: item.workspace ? `Nova aba em ${item.workspace}` : "Nova aba aqui",
          icon: "newTab",
          keys: [],
          startsGroup: true,
        },
      ];
      if (item.path) {
        entries.push(
          { action: "reveal", title: "Mostrar no Explorer", icon: "folder", keys: REVEAL },
          { action: "copyPath", title: "Copiar o caminho da pasta", icon: "copy", keys: COPY },
        );
      }
      entries.push({ action: "close", title: "Fechar este terminal", icon: "close", keys: ["Ctrl", "D"], destructive: true, startsGroup: true });
      return entries;
    }
    case "destination":
      return [
        { action: "open", title: "Abrir um workspace aqui", icon: "folderPlus", keys: OPEN },
        { action: "reveal", title: "Mostrar no Explorer", icon: "folder", keys: REVEAL, startsGroup: true },
        { action: "copyPath", title: "Copiar o caminho", icon: "copy", keys: COPY },
      ];
    case "hit":
      return [
        { action: "open", title: "Ir para esta linha", icon: "go", keys: OPEN },
        { action: "copyText", title: "Copiar a linha", icon: "copy", keys: COPY, startsGroup: true },
      ];
    case "command": {
      // One thing, which Enter already does. The panel is here for consistency — every row answers Ctrl+K — and because a panel that names the command is a confirmation of the destructive ones.
      const spec = commandSpec(item.kind.command);
      return [{ action: "open", title: spec.title, icon: spec.icon, keys: OPEN, destructive: spec.destructive }];
    }
    case "create":
      return [{ action: "open", title: item.title, icon: "folderPlus", keys: OPEN }];
  }
}

/** The entries whose titles hold what was typed. */
export function filterActions(entries: readonly ActionEntry[], typed: string): ActionEntry[] {
  const needle = typed.trim().toLowerCase();
  return needle === "" ? [...entries] : entries.filter((e) => e.title.toLowerCase().includes(needle));
}

/**
 * The action a chord names on a row, without the panel being open — so that
 * what the panel says about them is true. Null when this row has no such
 * action (nothing to show on a line of history, no path on a tab that has not
 * reached a prompt).
 */
export function chordAction(item: PickerItem, chord: "reveal" | "copy"): ActionId | null {
  const entries = actionsFor(item);
  const wanted: ActionId[] = chord === "reveal" ? ["reveal"] : item.kind.type === "hit" ? ["copyText"] : ["copyPath"];
  return entries.find((e) => wanted.includes(e.action))?.action ?? null;
}
