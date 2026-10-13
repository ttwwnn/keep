// What the window itself can do for the palette's commands, as defaults: a part
// of the app that owns one binds it with `registerCommand` (commands.ts) and
// wins, whenever it does. The ones that belong to another part and have no
// default stay dim until it does.

import { SPLIT_DOWN, SPLIT_RIGHT } from "../api";
import { registerDefault } from "../commands";
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
  registerDefault("toggleSidebar", { run: toggleSidebar });
  registerDefault("splitRight", { run: () => void split(SPLIT_RIGHT), enabled: hasPane });
  registerDefault("splitDown", { run: () => void split(SPLIT_DOWN), enabled: hasPane });
  registerDefault("closePane", {
    run: () => {
      const workspace = activeWorkspace.value;
      const pane = activePane.value;
      if (workspace && pane) void closePane(workspace, pane.id);
    },
    // A tab of one pane is closed as a tab: there is no pane to close.
    enabled: () => (activeTab.value?.panes.length ?? 0) > 1,
  });
  registerDefault("settings", { run: openSettings });
  registerDefault("fullscreen", { run: () => perform({ kind: "fullscreen" }) });

  registerDefault("newTab", { run: () => void newTab() });
  registerDefault("newWorkspace", { run: () => void newWorkspace() });
  registerDefault("closeTab", {
    run: () => {
      const workspace = activeWorkspace.value;
      const tab = activeTab.value;
      if (!workspace || !tab) return;
      const index = (tabsByWorkspace.value.get(workspace) ?? []).indexOf(tab);
      void closeTab(workspace, tab, tabLabel(workspace, tab, Math.max(0, index)));
    },
    enabled: () => activeTab.value !== null,
  });
  registerDefault("killWorkspace", {
    run: () => {
      const workspace = activeWorkspace.value;
      if (workspace) void closeWorkspace(workspace);
    },
    enabled: hasWorkspace,
  });
  registerDefault("pasteImage", { run: () => perform({ kind: "pasteImage" }), enabled: () => activeView() !== undefined });

  // The themes and the fonts: the settings, for now. The lists the Mac has —
  // each row a scrap of the thing — come from `registerCatalog` (commands.ts),
  // and "Usar o tema do Keep" is bound by whoever owns the themes.
  registerDefault("chooseTheme", { run: openSettings });
  registerDefault("chooseFont", { run: openSettings });
  // The zoom's steps, as the buttons and Ctrl+= are; dim at the ends, as the buttons are.
  registerDefault("fontBigger", { run: () => zoom(1), enabled: () => zoomStep(gui.value.zoom, 1) !== undefined });
  registerDefault("fontSmaller", { run: () => zoom(-1), enabled: () => zoomStep(gui.value.zoom, -1) !== undefined });
  // The size at 100% and the face back to the default: both, as on the Mac.
  registerDefault("fontReset", {
    run: () => {
      zoomReset();
      setFont("", 0);
    },
  });
}
