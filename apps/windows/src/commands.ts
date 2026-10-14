// What the palette can run: a closed list, as the macOS app's `Command` is —
// the set of things worth typing three letters to reach, not a scrape of a
// menu that nests and repeats itself. Seventeen are the Mac's, in its three
// groups; the three after them only Windows has.
//
// A command is its spec (what it is called, where it is listed, how it is
// drawn, which chord does the same without the palette) and a binding (what
// runs). The specs are here. The window binds what it can do itself in
// palette/bindings.ts, as defaults. What belongs to another part of the app is
// bound by that part, with `registerCommand` — which wins over a default, early
// or late — and shows dim in the list until it is:
//
//   toggleVerticalTabs   the tabs' side-bar-only layout
//   newWindow            a second window (windowSync.ts)
//   removeWorkspace      take a workspace off this window's list
//
// The two commands that open a list — the themes and the fonts — do not run
// anything from the palette: the card shows the list itself, in place of its
// own (ui/Picker.tsx), the same ones the settings open (ui/ThemePicker.tsx,
// ui/FontPicker.tsx).

import type { GlyphName } from "./palette/glyphs";

export type CommandId =
  | "toggleSidebar"
  | "toggleVerticalTabs"
  | "splitRight"
  | "splitDown"
  | "closePane"
  | "newWindow"
  | "settings"
  | "fullscreen"
  | "newTab"
  | "newWorkspace"
  | "closeTab"
  | "removeWorkspace"
  | "killWorkspace"
  | "pasteImage"
  | "chooseTheme"
  | "chooseFont"
  | "fontBigger"
  | "fontSmaller"
  | "fontReset"
  | "clearTheme";

/** Where a command is listed, and a name the query matches it by. */
export type CommandGroup = "janela" | "workspace" | "aparência";

/** A list the palette shows in place of its own. */
export type CatalogId = "themes" | "fonts";

export interface CommandSpec {
  id: CommandId;
  title: string;
  group: CommandGroup;
  icon: GlyphName;
  /** The chord that does the same without the palette, as the keys it is drawn with; empty for none. */
  keys: readonly string[];
  /** Said in red, because these do not undo. */
  destructive?: boolean;
  /** Choosing it opens this list rather than doing something. */
  opens?: CatalogId;
}

/** In the order the palette lists them: the Mac's, group by group, then what only Windows has. */
export const COMMANDS: readonly CommandSpec[] = [
  { id: "toggleSidebar", title: "Alternar a barra lateral", group: "janela", icon: "sidebar", keys: ["Ctrl", "Shift", "B"] },
  { id: "toggleVerticalTabs", title: "Alternar abas verticais", group: "janela", icon: "verticalTabs", keys: [] },
  { id: "splitRight", title: "Dividir à direita", group: "janela", icon: "splitRight", keys: ["Ctrl", "Shift", "D"] },
  { id: "splitDown", title: "Dividir abaixo", group: "janela", icon: "splitDown", keys: ["Ctrl", "Shift", "E"] },
  // Nothing closes a pane or a tab from the keyboard (keys.ts): no chord to show.
  { id: "closePane", title: "Fechar este painel", group: "janela", icon: "closePane", keys: [], destructive: true },
  { id: "newWindow", title: "Nova janela", group: "janela", icon: "newWindow", keys: ["Ctrl", "Shift", "Alt", "N"] },
  { id: "settings", title: "Configurações", group: "janela", icon: "settings", keys: ["Ctrl", ","] },
  { id: "fullscreen", title: "Tela cheia", group: "janela", icon: "fullscreen", keys: ["F11"] },

  { id: "newTab", title: "Nova aba", group: "workspace", icon: "newTab", keys: ["Ctrl", "Shift", "T"] },
  { id: "newWorkspace", title: "Novo workspace…", group: "workspace", icon: "folderPlus", keys: ["Ctrl", "Shift", "N"] },
  { id: "closeTab", title: "Fechar esta aba", group: "workspace", icon: "close", keys: [], destructive: true },
  {
    id: "removeWorkspace",
    title: "Remover este workspace da janela",
    group: "workspace",
    icon: "hide",
    keys: [],
    destructive: true,
  },
  { id: "killWorkspace", title: "Encerrar este workspace", group: "workspace", icon: "kill", keys: [], destructive: true },
  { id: "pasteImage", title: "Colar imagem", group: "workspace", icon: "image", keys: ["Ctrl", "Shift", "V"] },

  { id: "chooseTheme", title: "Mudar o tema…", group: "aparência", icon: "theme", keys: [], opens: "themes" },
  { id: "chooseFont", title: "Mudar a fonte…", group: "aparência", icon: "font", keys: [], opens: "fonts" },
  // The zoom's steps, as Ctrl+= and Ctrl+- are. Resetting the size advertises
  // no chord: it puts the face back as well, which Ctrl+0 does not.
  { id: "fontBigger", title: "Texto maior", group: "aparência", icon: "fontBigger", keys: ["Ctrl", "+"] },
  { id: "fontSmaller", title: "Texto menor", group: "aparência", icon: "fontSmaller", keys: ["Ctrl", "−"] },
  { id: "fontReset", title: "Redefinir o tamanho do texto", group: "aparência", icon: "reset", keys: [] },
  { id: "clearTheme", title: "Usar o tema do Keep", group: "aparência", icon: "undo", keys: [] },
];

export function commandSpec(id: CommandId): CommandSpec {
  const spec = COMMANDS.find((c) => c.id === id);
  if (!spec) throw new Error(`comando desconhecido: ${id}`);
  return spec;
}

export interface CommandBinding {
  run(): void;
  /** Whether it can run now; a binding without it always can. Read where the list is drawn, so it may follow signals. */
  enabled?(): boolean;
}

const bindings = new Map<CommandId, CommandBinding>();

/** Bind a command; binding one again replaces what was bound, which is how another part of the app takes over a default. */
export function registerCommand(id: CommandId, binding: CommandBinding): void {
  bindings.set(id, binding);
}

/**
 * Bind a command only if nothing is bound yet: what the window does of itself
 * (palette/bindings.ts), which never takes a command back from the part of the
 * app that owns it — however early in the start that part bound it.
 */
export function registerDefault(id: CommandId, binding: CommandBinding): void {
  if (!bindings.has(id)) bindings.set(id, binding);
}

/** Whether a command has something to run now. Dim in the palette, and never chosen, when not. */
export function isEnabled(id: CommandId): boolean {
  // One that opens a list has nothing bound: the palette shows the list itself.
  if (commandSpec(id).opens) return true;
  const binding = bindings.get(id);
  return binding !== undefined && (binding.enabled?.() ?? true);
}

/** Run a command if it can be; whether it did. */
export function runCommand(id: CommandId): boolean {
  const binding = bindings.get(id);
  if (!binding || !isEnabled(id)) return false;
  binding.run();
  return true;
}
