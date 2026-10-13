// What the window itself can do for the palette's commands. The ones that
// belong to another part of the app are not bound here — they stay dim until
// that part calls `registerCommand` (commands.ts lists them).

import { SPLIT_DOWN, SPLIT_RIGHT } from "../api";
import { registerCommand } from "../commands";
import { perform } from "../input";
import {
  activePane,
  activeTab,
  activeView,
  activeWorkspace,
  closePane,
  closeTab,
  closeWorkspace,
  gui,
  newTab,
  newWorkspace,
  overlay,
  setFont,
  split,
  tabLabel,
  tabsByWorkspace,
  toggleSidebar,
  zoom,
  zoomReset,
} from "../store";
import { zoomStep } from "../zoom";

const hasWorkspace = () => activeWorkspace.value !== null;
const hasPane = () => activePane.value !== null;

/** Where the themes and the fonts are chosen until the palette has lists of its own for them. */
const openSettings = () => {
  overlay.value = "settings";
};

export function bindCommands(): void {
  registerCommand("toggleSidebar", { run: toggleSidebar });
  registerCommand("splitRight", { run: () => void split(SPLIT_RIGHT), enabled: hasPane });
  registerCommand("splitDown", { run: () => void split(SPLIT_DOWN), enabled: hasPane });
  registerCommand("closePane", {
    run: () => {
      const workspace = activeWorkspace.value;
      const pane = activePane.value;
      if (workspace && pane) void closePane(workspace, pane.id);
    },
    // A tab of one pane is closed as a tab: there is no pane to close.
    enabled: () => (activeTab.value?.panes.length ?? 0) > 1,
  });
  registerCommand("settings", { run: openSettings });
  registerCommand("fullscreen", { run: () => perform({ kind: "fullscreen" }) });

  registerCommand("newTab", { run: () => void newTab() });
  registerCommand("newWorkspace", { run: () => void newWorkspace() });
  registerCommand("closeTab", {
    run: () => {
      const workspace = activeWorkspace.value;
      const tab = activeTab.value;
      if (!workspace || !tab) return;
      const index = (tabsByWorkspace.value.get(workspace) ?? []).indexOf(tab);
      void closeTab(workspace, tab, tabLabel(workspace, tab, Math.max(0, index)));
    },
    enabled: () => activeTab.value !== null,
  });
  registerCommand("killWorkspace", {
    run: () => {
      const workspace = activeWorkspace.value;
      if (workspace) void closeWorkspace(workspace);
    },
    enabled: hasWorkspace,
  });
  registerCommand("pasteImage", { run: () => perform({ kind: "pasteImage" }), enabled: () => activeView() !== undefined });

  // The themes and the fonts: the settings, for now. The lists the Mac has —
  // each row a scrap of the thing — come from `registerCatalog` (commands.ts),
  // and "Usar o tema do Keep" is bound by whoever owns the themes.
  registerCommand("chooseTheme", { run: openSettings });
  registerCommand("chooseFont", { run: openSettings });
  // The zoom's steps, as the buttons and Ctrl+= are; dim at the ends, as the buttons are.
  registerCommand("fontBigger", { run: () => zoom(1), enabled: () => zoomStep(gui.value.zoom, 1) !== undefined });
  registerCommand("fontSmaller", { run: () => zoom(-1), enabled: () => zoomStep(gui.value.zoom, -1) !== undefined });
  // The size at 100% and the face back to the default: both, as on the Mac.
  registerCommand("fontReset", {
    run: () => {
      zoomReset();
      setFont("", 0);
    },
  });
}
