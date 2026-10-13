// What the window itself can do for the palette's commands, as defaults: a part
// of the app that owns one binds it with `registerCommand` (commands.ts) and
// wins, whenever it does. The one that belongs to a part not here yet (a second
// window) has no default and stays dim until that part binds it.

import { SPLIT_DOWN, SPLIT_RIGHT } from "../api";
import { clearTheme, resetText } from "../appearance";
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
  putAway,
  shownWorkspaces,
  split,
  tabLabel,
  tabsByWorkspace,
  toggleSidebar,
  toggleVerticalTabs,
  zoom,
} from "../store";
import { zoomStep } from "../zoom";

const hasWorkspace = () => activeWorkspace.value !== null;
const hasPane = () => activePane.value !== null;

export function bindCommands(): void {
  registerDefault("toggleSidebar", { run: toggleSidebar });
  registerDefault("toggleVerticalTabs", { run: toggleVerticalTabs });
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
  registerDefault("settings", {
    run: () => {
      overlay.value = "settings";
    },
  });
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
  // Off the sidebar, still running: "Ir para" brings it back.
  registerDefault("removeWorkspace", {
    run: () => {
      const workspace = activeWorkspace.value;
      if (workspace) putAway(workspace);
    },
    enabled: () => shownWorkspaces.value.some((w) => w.name === activeWorkspace.value),
  });
  registerDefault("killWorkspace", {
    run: () => {
      const workspace = activeWorkspace.value;
      if (workspace) void closeWorkspace(workspace);
    },
    enabled: hasWorkspace,
  });
  registerDefault("pasteImage", { run: () => perform({ kind: "pasteImage" }), enabled: () => activeView() !== undefined });

  // The zoom's steps, as the buttons and Ctrl+= are; dim at the ends, as the buttons are.
  registerDefault("fontBigger", { run: () => zoom(1), enabled: () => zoomStep(gui.value.zoom, 1) !== undefined });
  registerDefault("fontSmaller", { run: () => zoom(-1), enabled: () => zoomStep(gui.value.zoom, -1) !== undefined });
  // The size at 100% and the face back to the default: both, as on the Mac.
  registerDefault("fontReset", { run: resetText });
  registerDefault("clearTheme", { run: clearTheme });
}
