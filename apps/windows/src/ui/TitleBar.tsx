// The one line of chrome above the terminal: the tabs of the workspace in
// front, a button for another, and — on Windows, where the window has no
// frame of its own — the three window buttons. Empty space in it moves the
// window, as a title bar does.

import { getCurrentWindow } from "@tauri-apps/api/window";
import { SPLIT_DOWN, SPLIT_RIGHT } from "../api";
import { endedADrag } from "../drag";
import type { RootTab } from "../layout";
import { mayArmRename, slowRename } from "../naming";
import { tabKey } from "../state";
import { toggleOverlay } from "../palette/overlay";
import {
  activeTab,
  activeWorkspace,
  closeTab,
  editingTabOn,
  finishTabName,
  gui,
  isMac,
  isWindows,
  maximized,
  select,
  split,
  startRename,
  tabsByWorkspace,
  toggleSidebar,
  toggleVerticalTabs,
} from "../store";
import { AccountChevron } from "./AccountMenu";
import { InlineEditor, openMenu } from "./common";
import { Glyph } from "./Glyph";
import * as icon from "./icons";
import { PaneMark } from "./paneIcons";
import { startStripDrag, stripMove } from "./tabDrag";
import { tabLook, useBreath } from "./tabLook";
import { WindowMark } from "./windowMark";
import { shownElsewhere } from "../windowModel";
import { peers, windowLabel } from "../windowPeers";
import { openNewWindow } from "../windowSync";

function TabCapsule(props: {
  workspace: string;
  tab: RootTab;
  tabs: RootTab[];
  index: number;
  selected: boolean;
  alone: boolean;
  count: number;
}) {
  const { workspace, tab, tabs, index, selected, alone, count } = props;
  const look = tabLook(workspace, tab, index, selected);
  const badge = useBreath(look.breathing);
  const renaming = editingTabOn("strip", workspace, tab.root.id);
  const rename = () => startRename(workspace, tab.root.id, "strip");
  const close = () => void closeTab(workspace, tab, look.label);
  // The ninth is the last, as in a browser; past the ninth there is no number.
  const key = index < 8 ? index + 1 : index === count - 1 ? 9 : 0;
  // Carried along the row by hand: this one follows the pointer, the others slide aside.
  const carry = stripMove.value?.workspace === workspace ? stripMove.value : null;
  const carried = carry !== null && carry.from === index;
  const torn = carried && carry.tearing === true;
  const slid = carry && !carried ? carry.slid[index] : 0;
  const transform = carried ? `translateX(${carry.dx}px)` : slid ? `translateX(${slid}px)` : undefined;
  // Another window has this tab in front.
  const elsewhere = shownElsewhere(peers.value, windowLabel.value, workspace, tab.root.id);
  const style = look.wantsYou || transform ? { background: look.wantsYou ? look.fill : undefined, transform } : undefined;
  return (
    <div
      ref={badge as never}
      class={`tab ${selected ? "selected" : ""} ${look.wantsYou ? "attention" : ""} ${alone ? "alone" : ""} ${carried ? "carried" : ""} ${torn ? "torn" : ""}`}
      style={style}
      title={tab.root.cwd || undefined}
      data-tab={tabKey(workspace, tab.root.id)}
      onPointerDown={(e) => startStripDrag(e, workspace, tabs, index)}
      onMouseDown={(e) => {
        // The middle button closes, as in a browser — after asking.
        if (e.button === 1) {
          e.preventDefault();
          close();
        }
      }}
      onClick={(e) => {
        // The click that ends a drag is not a click on the tab.
        if (endedADrag()) return;
        select(workspace, tab.root.id);
        // Finder's slow click: the name of the tab already in front, clicked once, opens for typing.
        slowRename.disarm();
        const onName = (e.target as HTMLElement).closest(".tab-title") !== null;
        if (mayArmRename({ wasChosen: selected, onName, dragged: false, count: e.detail })) {
          slowRename.arm(rename, () => activeWorkspace.value === workspace && activeTab.value?.root.id === tab.root.id);
        }
      }}
      onDblClick={() => (slowRename.disarm(), rename())}
      onContextMenu={(e) =>
        openMenu(e, [
          { label: "Renomear", hint: "F2", action: rename },
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
          onDone={(value, keyboard) => finishTabName(workspace, tab.root.id, value, keyboard)}
        />
      ) : (
        <span
          class={`tab-title ${look.asleep ? "asleep" : ""}`}
          style={look.color ? { color: look.color } : undefined}
          onPointerLeave={() => slowRename.disarm()}
        >
          {look.wantsYou && (
            <span class="hand">
              <icon.Hand />
            </span>
          )}
          {look.mark && <span class="busy-mark">{look.mark.glyph} </span>}
          {look.label}
          {tab.panes.length > 1 && <PaneMark />}
          {elsewhere && <WindowMark />}
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
  // The tabs live in the sidebar alone: the row steps aside.
  const vertical = gui.value.verticalTabs;
  return (
    <header class={`titlebar ${isMac.value && sidebarHidden ? "inset" : ""}`} data-tauri-drag-region>
      {sidebarHidden && (
        <button class="round-button" title="Mostrar a barra lateral" aria-label="Mostrar a barra lateral" onClick={toggleSidebar}>
          <icon.Sidebar />
        </button>
      )}
      <nav
        class={`strip ${stripMove.value ? "reordering" : ""}`}
        data-tauri-drag-region
        aria-label="Abas"
        onContextMenu={(e) =>
          openMenu(e, [
            {
              label: vertical ? "Mostrar a fileira de abas" : "Abas só na barra lateral",
              action: toggleVerticalTabs,
            },
            "separator",
            { label: "Nova janela", hint: isMac.value ? "⌘⇧⌥N" : "Ctrl+Shift+Alt+N", action: () => void openNewWindow() },
          ])
        }
      >
        {ws &&
          !vertical &&
          tabs.map((tab, i) => (
            <TabCapsule
              key={tabKey(ws, tab.root.id)}
              workspace={ws}
              tab={tab}
              tabs={tabs}
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
          title={`Ir para… (${isMac.value ? "⌘⇧P" : "Ctrl+Shift+P"})`}
          aria-label="Ir para"
          onClick={() => toggleOverlay("picker")}
        >
          <icon.Search />
        </button>
        <button
          class="round-button"
          data-overlay-toggle
          title={`Comandos… (${isMac.value ? "⌘⇧⌥P" : "Ctrl+Shift+Alt+P"})`}
          aria-label="Comandos"
          onClick={() => toggleOverlay("palette")}
        >
          <Glyph name="command" size={15} />
        </button>
      </div>
      {isWindows.value && <WindowControls />}
    </header>
  );
}
