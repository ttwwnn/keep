// The one line of chrome above the terminal: the tabs of the workspace in
// front, a button for another, and — on Windows, where the window has no
// frame of its own — the three window buttons. Empty space in it moves the
// window, as a title bar does.

import { getCurrentWindow } from "@tauri-apps/api/window";
import { SPLIT_DOWN, SPLIT_RIGHT } from "../api";
import type { RootTab } from "../layout";
import { tabKey } from "../state";
import {
  activeTab,
  activeWorkspace,
  closeTab,
  editing,
  gui,
  isMac,
  isWindows,
  maximized,
  newTab,
  renameTab,
  select,
  split,
  tabsByWorkspace,
  toggleSidebar,
} from "../store";
import { InlineEditor, openMenu } from "./common";
import * as icon from "./icons";
import { tabLook, useBreath } from "./tabLook";

function TabCapsule(props: { workspace: string; tab: RootTab; index: number; selected: boolean }) {
  const { workspace, tab, index, selected } = props;
  const look = tabLook(workspace, tab, index, selected);
  const badge = useBreath(look.breathing);
  const renaming =
    editing.value?.kind === "tab" && editing.value.workspace === workspace && editing.value.tab === tab.root.id;
  const startRename = () => (editing.value = { kind: "tab", workspace, tab: tab.root.id });
  const close = () => void closeTab(workspace, tab, look.label);
  return (
    <div
      ref={badge as never}
      class={`tab ${selected ? "selected" : ""} ${look.wantsYou ? "attention" : ""}`}
      style={look.wantsYou ? { background: look.fill } : undefined}
      title={tab.root.cwd || undefined}
      data-tab={tabKey(workspace, tab.root.id)}
      onMouseDown={(e) => {
        // The middle button closes, as in a browser — after asking.
        if (e.button === 1) {
          e.preventDefault();
          close();
        }
      }}
      onClick={() => select(workspace, tab.root.id)}
      onDblClick={startRename}
      onContextMenu={(e) =>
        openMenu(e, [
          { label: "Renomear", hint: "F2", action: startRename },
          { label: "Dividir à direita", hint: "Ctrl+Shift+D", action: () => (select(workspace, tab.root.id), void split(SPLIT_RIGHT)) },
          { label: "Dividir abaixo", hint: "Ctrl+Shift+E", action: () => (select(workspace, tab.root.id), void split(SPLIT_DOWN)) },
          "separator",
          { label: "Fechar a aba…", danger: true, action: close },
        ])
      }
    >
      {look.wantsYou && (
        <span class="hand">
          <icon.Hand />
        </span>
      )}
      {renaming ? (
        <InlineEditor
          value={gui.value.tabNames[tabKey(workspace, tab.root.id)] ?? look.label.replace(/^✳ /, "")}
          onDone={(value) => {
            editing.value = null;
            if (value !== null) renameTab(workspace, tab.root.id, value);
          }}
        />
      ) : (
        <span class="tab-title" style={look.color ? { color: look.color } : undefined}>
          {look.label}
        </span>
      )}
      <button
        class="tab-close"
        title="Fechar a aba"
        aria-label="Fechar a aba"
        onClick={(e) => {
          e.stopPropagation();
          close();
        }}
      >
        <icon.Close size={8} />
      </button>
    </div>
  );
}

function WindowControls() {
  const win = getCurrentWindow();
  return (
    <div class="window-controls">
      <button class="window-button" title="Minimizar" aria-label="Minimizar" onClick={() => void win.minimize()}>
        <icon.WinMinimize />
      </button>
      <button
        class="window-button"
        title={maximized.value ? "Restaurar" : "Maximizar"}
        aria-label={maximized.value ? "Restaurar" : "Maximizar"}
        onClick={() => void win.toggleMaximize()}
      >
        {maximized.value ? <icon.WinRestore /> : <icon.WinMaximize />}
      </button>
      <button class="window-button close" title="Fechar" aria-label="Fechar" onClick={() => void win.close()}>
        <icon.WinClose />
      </button>
    </div>
  );
}

export function TitleBar() {
  const ws = activeWorkspace.value;
  const tabs = ws ? tabsByWorkspace.value.get(ws) ?? [] : [];
  const current = activeTab.value;
  const sidebarHidden = !gui.value.sidebar.visible;
  return (
    <header class={`titlebar ${isMac.value && sidebarHidden ? "inset" : ""}`} data-tauri-drag-region>
      {sidebarHidden && (
        <button class="icon-button" title="Mostrar a barra lateral" aria-label="Mostrar a barra lateral" onClick={toggleSidebar}>
          <icon.Sidebar />
        </button>
      )}
      <nav class="strip" data-tauri-drag-region aria-label="Abas">
        {ws &&
          tabs.map((tab, i) => (
            <TabCapsule key={tabKey(ws, tab.root.id)} workspace={ws} tab={tab} index={i} selected={tab === current} />
          ))}
        <button class="new-tab icon-button" title="Nova aba (Ctrl+Shift+T)" aria-label="Nova aba" onClick={() => void newTab()}>
          <icon.Plus />
        </button>
      </nav>
      <div class="drag-fill" data-tauri-drag-region />
      {isWindows.value && <WindowControls />}
    </header>
  );
}
