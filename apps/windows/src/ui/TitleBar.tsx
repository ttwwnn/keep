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
  overlay,
  renameTab,
  select,
  split,
  tabsByWorkspace,
  toggleSidebar,
} from "../store";
import { AccountChevron } from "./AccountMenu";
import { InlineEditor, openMenu } from "./common";
import * as icon from "./icons";
import { tabLook, useBreath } from "./tabLook";

function TabCapsule(props: { workspace: string; tab: RootTab; index: number; selected: boolean; alone: boolean; count: number }) {
  const { workspace, tab, index, selected, alone, count } = props;
  const look = tabLook(workspace, tab, index, selected);
  const badge = useBreath(look.breathing);
  const renaming =
    editing.value?.kind === "tab" && editing.value.workspace === workspace && editing.value.tab === tab.root.id;
  const startRename = () => (editing.value = { kind: "tab", workspace, tab: tab.root.id });
  const close = () => void closeTab(workspace, tab, look.label);
  // The ninth is the last, as in a browser; past the ninth there is no number.
  const key = index < 8 ? index + 1 : index === count - 1 ? 9 : 0;
  return (
    <div
      ref={badge as never}
      class={`tab ${selected ? "selected" : ""} ${look.wantsYou ? "attention" : ""} ${alone ? "alone" : ""}`}
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
      {!alone && !renaming && (
        <button
          class="tab-close"
          title="Fechar a aba"
          aria-label="Fechar a aba"
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.stopPropagation();
            close();
          }}
        >
          <icon.Close size={8} />
        </button>
      )}
      {renaming ? (
        <InlineEditor
          value={gui.value.tabNames[tabKey(workspace, tab.root.id)] ?? look.label}
          onDone={(value) => {
            editing.value = null;
            if (value !== null) renameTab(workspace, tab.root.id, value);
          }}
        />
      ) : (
        <span class="tab-title" style={look.color ? { color: look.color } : undefined}>
          {look.wantsYou && (
            <span class="hand">
              <icon.Hand />
            </span>
          )}
          {look.mark && <span class="busy-mark">{look.mark.glyph} </span>}
          {look.label}
        </span>
      )}
      {!renaming && <AccountChevron workspace={workspace} tab={tab} label={look.label} />}
      {!alone && !renaming && key > 0 && <span class="tab-key">{isMac.value ? `⌘${key}` : `Ctrl+${key}`}</span>}
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
        <button class="round-button" title="Mostrar a barra lateral" aria-label="Mostrar a barra lateral" onClick={toggleSidebar}>
          <icon.Sidebar />
        </button>
      )}
      <nav class="strip" data-tauri-drag-region aria-label="Abas">
        {ws &&
          tabs.map((tab, i) => (
            <TabCapsule
              key={tabKey(ws, tab.root.id)}
              workspace={ws}
              tab={tab}
              index={i}
              selected={tab === current}
              alone={tabs.length === 1}
              count={tabs.length}
            />
          ))}
      </nav>
      <div class="drag-fill" data-tauri-drag-region />
      <div class="strip-actions">
        <button
          class="round-button"
          data-overlay-toggle
          title="Ir para… (Ctrl+Shift+P)"
          aria-label="Ir para"
          onClick={() => (overlay.value = overlay.value === "picker" ? null : "picker")}
        >
          <icon.Search />
        </button>
        <button
          class="round-button"
          data-overlay-toggle
          title="Configurações"
          aria-label="Configurações"
          onClick={() => (overlay.value = overlay.value === "settings" ? null : "settings")}
        >
          <icon.Gear />
        </button>
      </div>
      {isWindows.value && <WindowControls />}
    </header>
  );
}
