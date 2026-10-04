// The sidebar: every workspace, in the person's order, with its tabs under it.
//
// It runs the full height of the window, Finder-style. A workspace's dot says
// whether something runs in it — filled while busy, hollow at rest, so the
// shape carries the state as well as the hue (DESIGN.md). Folded, a workspace
// still says how many of its tabs wait on you.

import { signal } from "@preact/signals";
import { useRef } from "preact/hooks";
import { SPLIT_DOWN, SPLIT_RIGHT } from "../api";
import type { RootTab } from "../layout";
import { SIDEBAR_MAX, SIDEBAR_MIN, tabKey } from "../state";
import {
  activeTab,
  activeWorkspace,
  closeTab,
  closeWorkspace,
  editing,
  gui,
  isMac,
  moveWorkspace,
  newTab,
  newWorkspace,
  notice,
  orderedWorkspaces,
  overlay,
  renameTab,
  renameWorkspace,
  select,
  setSidebarWidth,
  split,
  tabActivity,
  tabBusy,
  tabsByWorkspace,
  toggleFolded,
  toggleSidebar,
  workspaceLabel,
  zoom,
  zoomReset,
} from "../store";
import { homeRelative } from "../text";
import { info } from "../store";
import { zoomPercent } from "../zoom";
import { InlineEditor, openMenu } from "./common";
import * as icon from "./icons";
import { tabLook, useBreath } from "./tabLook";

function TabRow(props: { workspace: string; tab: RootTab; index: number; selected: boolean }) {
  const { workspace, tab, index, selected } = props;
  const look = tabLook(workspace, tab, index, selected);
  const badge = useBreath(look.breathing);
  const renaming =
    editing.value?.kind === "tab" && editing.value.workspace === workspace && editing.value.tab === tab.root.id;
  const startRename = () => (editing.value = { kind: "tab", workspace, tab: tab.root.id });
  const close = () => void closeTab(workspace, tab, look.label);
  const where = homeRelative(tab.root.cwd, info.value?.home ?? "");
  return (
    <div
      class={`tab-row ${selected ? "selected" : ""}`}
      title={where || undefined}
      onClick={() => select(workspace, tab.root.id)}
      onDblClick={startRename}
      onMouseDown={(e) => {
        if (e.button === 1) {
          e.preventDefault();
          close();
        }
      }}
      onContextMenu={(e) =>
        openMenu(e, [
          { label: "Renomear", hint: "F2", action: startRename },
          { label: "Dividir à direita", action: () => (select(workspace, tab.root.id), void split(SPLIT_RIGHT)) },
          { label: "Dividir abaixo", action: () => (select(workspace, tab.root.id), void split(SPLIT_DOWN)) },
          "separator",
          { label: "Fechar a aba…", danger: true, action: close },
        ])
      }
    >
      {renaming ? (
        <InlineEditor
          value={gui.value.tabNames[tabKey(workspace, tab.root.id)] ?? look.label.replace(/^✳ /, "")}
          onDone={(value) => {
            editing.value = null;
            if (value !== null) renameTab(workspace, tab.root.id, value);
          }}
        />
      ) : look.wantsYou ? (
        <span ref={badge as never} class="row-badge" style={{ background: look.fill }}>
          <icon.Hand />
          <span class="tab-title" style={look.color ? { color: look.color } : undefined}>
            {look.label}
          </span>
        </span>
      ) : (
        <span class="tab-title" style={look.color ? { color: look.color } : undefined}>
          {look.label}
        </span>
      )}
      {tab.panes.length > 1 && <span class="pane-count">{tab.panes.length}</span>}
    </div>
  );
}

function WorkspaceGroup(props: { name: string }) {
  const { name } = props;
  const tabs = tabsByWorkspace.value.get(name) ?? [];
  const folded = gui.value.folded.includes(name);
  const current = activeWorkspace.value === name;
  const busy = tabs.some((t) => tabBusy(name, t));
  const waiting = tabs.filter((t) => tabActivity(name, t) === "waitingForYou").length;
  const renaming = editing.value?.kind === "workspace" && editing.value.workspace === name;
  const startRename = () => (editing.value = { kind: "workspace", workspace: name });
  const selectedTab = activeTab.value;
  return (
    <section class={`workspace ${current ? "current" : ""}`} data-workspace={name}>
      <div
        class={`workspace-row ${dragging.value?.over === name && dragging.value.from !== name ? "drop-before" : ""}`}
        onPointerDown={(e) => startReorder(e, name, renaming)}
        onClick={() => {
          if (!justDragged) select(name);
        }}
        onDblClick={startRename}
        onContextMenu={(e) =>
          openMenu(e, [
            { label: "Nova aba aqui", action: () => void newTab(name) },
            { label: "Renomear", action: startRename },
            { label: folded ? "Mostrar as abas" : "Recolher as abas", action: () => toggleFolded(name) },
            "separator",
            { label: "Fechar o workspace…", danger: true, action: () => void closeWorkspace(name) },
          ])
        }
      >
        <button
          class="fold"
          title={folded ? "Mostrar as abas" : "Recolher as abas"}
          aria-label={folded ? "Mostrar as abas" : "Recolher as abas"}
          onClick={(e) => {
            e.stopPropagation();
            toggleFolded(name);
          }}
        >
          <icon.Chevron open={!folded} />
        </button>
        <span class={`dot ${busy ? "busy" : "idle"}`} aria-label={busy ? "rodando" : "parado"} />
        {renaming ? (
          <InlineEditor
            value={workspaceLabel(name)}
            onDone={(value) => {
              editing.value = null;
              if (value !== null) renameWorkspace(name, value);
            }}
          />
        ) : (
          <span class="workspace-name" title={name}>
            {workspaceLabel(name)}
          </span>
        )}
        {folded && waiting > 0 && (
          <span class="waiting-count" title={`${waiting} aba(s) esperando você`}>
            <icon.Hand />
            {waiting}
          </span>
        )}
        <span class="tab-count">{tabs.length}</span>
        <button
          class="row-close"
          title="Fechar o workspace"
          aria-label="Fechar o workspace"
          onClick={(e) => {
            e.stopPropagation();
            void closeWorkspace(name);
          }}
        >
          <icon.Close size={8} />
        </button>
      </div>
      {!folded &&
        tabs.map((tab, i) => (
          <TabRow
            key={tabKey(name, tab.root.id)}
            workspace={name}
            tab={tab}
            index={i}
            selected={current && tab === selectedTab}
          />
        ))}
    </section>
  );
}

/**
 * Moving a workspace up or down the list, by its row.
 *
 * With the pointer rather than HTML drag and drop: on Windows the webview
 * hands every drag to the window's own file-drop handler — which is what
 * lets files dropped on a terminal arrive as their paths — and HTML drag and
 * drop inside the page never starts.
 */
const dragging = signal<{ from: string; over: string | null } | null>(null);
let justDragged = false;

function startReorder(event: PointerEvent, name: string, renaming: boolean) {
  if (renaming || event.button !== 0 || (event.target as HTMLElement).closest("button, input")) return;
  const row = event.currentTarget as HTMLElement;
  const startY = event.clientY;
  let active = false;
  const move = (e: PointerEvent) => {
    if (!active && Math.abs(e.clientY - startY) < 5) return;
    if (!active) {
      active = true;
      row.setPointerCapture(e.pointerId);
    }
    const under = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>(".workspace");
    dragging.value = { from: name, over: under?.dataset.workspace ?? null };
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    const result = dragging.value;
    dragging.value = null;
    if (!active || !result) return;
    // The click that ends a drag is not a click on the row.
    justDragged = true;
    setTimeout(() => (justDragged = false), 0);
    if (result.over && result.over !== name) moveWorkspace(name, result.over);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

function ZoomControl() {
  const level = gui.value.zoom;
  return (
    <div class="zoom" role="group" aria-label="Zoom">
      <button title="Diminuir o zoom (Ctrl+-)" aria-label="Diminuir o zoom" onClick={() => zoom(-1)} disabled={level <= 0.5}>
        <icon.Minus />
      </button>
      <button class="zoom-level" title="Voltar a 100% (Ctrl+0)" onClick={zoomReset}>
        {zoomPercent(level)}
      </button>
      <button title="Aumentar o zoom (Ctrl+=)" aria-label="Aumentar o zoom" onClick={() => zoom(1)} disabled={level >= 3}>
        <icon.Plus />
      </button>
    </div>
  );
}

/** The sidebar's right edge, dragged to make it wider or narrower. */
function Resizer() {
  const start = useRef<{ x: number; width: number } | null>(null);
  return (
    <div
      class="sidebar-resizer"
      onPointerDown={(e) => {
        (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        start.current = { x: e.clientX, width: gui.value.sidebar.width };
      }}
      onPointerMove={(e) => {
        if (!start.current) return;
        const width = start.current.width + e.clientX - start.current.x;
        setSidebarWidth(Math.round(Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, width))));
      }}
      onPointerUp={() => (start.current = null)}
      onDblClick={() => setSidebarWidth(240)}
    />
  );
}

export function Sidebar() {
  const width = gui.value.sidebar.width;
  return (
    <aside class="sidebar" style={{ width: `${width}px` }}>
      <div class={`sidebar-head ${isMac.value ? "inset" : ""}`} data-tauri-drag-region>
        <span class="sidebar-title" data-tauri-drag-region>
          Workspaces
        </span>
        <button class="icon-button" title="Esconder a barra lateral" aria-label="Esconder a barra lateral" onClick={toggleSidebar}>
          <icon.Sidebar />
        </button>
      </div>
      <div class="sidebar-list">
        {orderedWorkspaces.value.map((w) => (
          <WorkspaceGroup key={w.name} name={w.name} />
        ))}
        <button class="new-workspace" onClick={() => void newWorkspace()} title="Novo workspace (Ctrl+Shift+N)">
          <icon.Plus /> Novo workspace
        </button>
      </div>
      <div class="sidebar-foot">
        {notice.value && <div class="notice">{notice.value}</div>}
        <div class="foot-row">
          <ZoomControl />
          <button
            class="icon-button"
            data-overlay-toggle
            title="Configurações"
            aria-label="Configurações"
            onClick={() => (overlay.value = overlay.value === "settings" ? null : "settings")}
          >
            <icon.Gear />
          </button>
        </div>
      </div>
      <Resizer />
    </aside>
  );
}
