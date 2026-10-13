// The glyphs the picker draws: a row's badge, a command's icon, an action's.
// Named here so the lists that choose them need not know how they are drawn
// (ui/Glyph.tsx draws them, and TypeScript holds it to drawing each).

export type GlyphName =
  | "terminal"
  | "terminalBusy"
  | "folder"
  | "folderPlus"
  | "go"
  | "newTab"
  | "copy"
  | "close"
  | "sidebar"
  | "verticalTabs"
  | "splitRight"
  | "splitDown"
  | "closePane"
  | "newWindow"
  | "hide"
  | "kill"
  | "settings"
  | "fullscreen"
  | "quickWindow"
  | "image"
  | "theme"
  | "font"
  | "fontBigger"
  | "fontSmaller"
  | "reset"
  | "undo"
  | "command";
