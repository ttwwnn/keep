// The sidebar: every workspace, in the person's order, with its tabs under it.
//
// It runs the full height of the window, Finder-style. A workspace's dot says
// whether something runs in it — filled while busy, hollow at rest, so the
// shape carries the state as well as the hue (DESIGN.md). Folded, a workspace
// still says how many of its tabs wait on you.

import { signal } from "@preact/signals";
import { useRef, useState } from "preact/hooks";
import { SPLIT_DOWN, SPLIT_RIGHT } from "../api";
import type { RootTab } from "../layout";
import { SIDEBAR_MAX, SIDEBAR_MIN, tabKey } from "../state";
import {
  activeTab,
  activeWorkspace,
  closeTab,
  closeWorkspace,
  createWorkspace,
  editing,
  gui,
  isMac,
  moveWorkspace,
  newTab,
  notice,
  orderedWorkspaces,
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
import { busyMark, programOf } from "../claude";
import { tabAccountLabel } from "../ia";
import { tabAccount, usageLines } from "../iaStore";
import { homeRelative } from "../text";
import { info } from "../store";
import { zoomPercent } from "../zoom";
import { AccountChevron } from "./AccountMenu";
import { InlineEditor, openMenu } from "./common";
import * as icon from "./icons";
import { tabLook, useBreath } from "./tabLook";
import { UsageFooter } from "./UsageFooter";

function TabRow(props: { workspace: string; tab: RootTab; index: number; selected: boolean }) {
  const { workspace, tab, index, selected } = props;
  const look = tabLook(workspace, tab, index, selected);
  const badge = useBreath(look.breathing);
  const renaming =
    editing.value?.kind === "tab" && editing.value.workspace === workspace && editing.value.tab === tab.root.id;
  const startRename = () => (editing.value = { kind: "tab", workspace, tab: tab.root.id });
  const close = () => void closeTab(workspace, tab, look.label);
  const where = homeRelative(tab.root.cwd, info.value?.home ?? "");
  // An AI says only the number of the account it runs on, its place in the
  // footer's list — "(IA·3)" —, and follows that list's order; any other
  // program says its name — "— pwsh" —, unless the title already is it.
  const account = tabAccount(workspace, tab.root);
  const program = tabAccountLabel(account.program, tab.root.command, account.key, account.running, usageLines.value);
  const repeats = program === `— ${look.label}` || program === `— ${look.label.replace(/\.exe$/i, "")}`;
  const suffix = program && !repeats ? <span class="tab-program">{program}</span> : null;
  return (
    <div
      ref={badge as never}
      class={`tab-row ${selected ? "selected" : ""} ${look.wantsYou ? "wants" : ""}`}
      style={look.wantsYou ? { background: look.fill } : undefined}
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
          value={gui.value.tabNames[tabKey(workspace, tab.root.id)] ?? look.label}
          onDone={(value) => {
            editing.value = null;
            if (value !== null) renameTab(workspace, tab.root.id, value);
          }}
        />
      ) : (
        <span class="tab-label">
          {look.wantsYou ? (
            <span class="hand">
              <icon.Hand />
            </span>
          ) : (
            look.mark && (
              <span class="busy-mark" style={{ color: look.mark.color }}>
                {look.mark.glyph}
              </span>
            )
          )}
          <span class="tab-title" style={look.color ? { color: look.color } : undefined}>
            {look.label}
          </span>
          {suffix}
          <AccountChevron workspace={workspace} tab={tab} label={look.label} />
        </span>
      )}
      {!renaming && (
        <button
          class="row-x"
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
    </div>
  );
}

function WorkspaceGroup(props: { name: string }) {
  const { name } = props;
  const tabs = tabsByWorkspace.value.get(name) ?? [];
  const folded = gui.value.folded.includes(name);
  const current = activeWorkspace.value === name;
  // One mark for each program at work in the workspace, as the header says it.
  const marks = [
    ...new Map(
      tabs
        .filter((t) => tabBusy(name, t))
        .map((t) => busyMark(programOf(t.root.command, t.root.title)))
        .map((m) => [m.glyph + m.color, m] as const),
    ).values(),
  ];
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
        {marks.length > 0 && (
          <span class="header-marks" aria-label="rodando">
            {marks.map((m) => (
              <span key={m.glyph + m.color} style={{ color: m.color }}>
                {m.glyph}
              </span>
            ))}
          </span>
        )}
        {folded && waiting > 0 && (
          <span class="waiting-count" title={`${waiting} aba(s) esperando você`}>
            <icon.Hand />
            {waiting}
          </span>
        )}
        <span class="header-spacer" />
        <button
          class="hdr-btn plus"
          title={`Nova aba em ${workspaceLabel(name)}`}
          aria-label="Nova aba neste workspace"
          onClick={(e) => {
            e.stopPropagation();
            void newTab(name);
          }}
        >
          <icon.Plus />
        </button>
        {tabs.length > 0 ? (
          <button
            class={`hdr-btn fold ${folded ? "" : "open"}`}
            title={folded ? "Mostrar as abas" : "Recolher as abas"}
            aria-label={folded ? "Mostrar as abas" : "Recolher as abas"}
            onClick={(e) => {
              e.stopPropagation();
              toggleFolded(name);
            }}
          >
            <icon.ChevronRight />
          </button>
        ) : (
          <span class="hdr-gap" />
        )}
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
      <button
        class={`zoom-level ${level !== 1 ? "zoomed" : ""}`}
        title="Voltar a 100% (Ctrl+0)"
        onClick={zoomReset}
      >
        {zoomPercent(level)}
      </button>
      <button title="Aumentar o zoom (Ctrl+=)" aria-label="Aumentar o zoom" onClick={() => zoom(1)} disabled={level >= 3}>
        <icon.Plus />
      </button>
    </div>
  );
}

/** A new workspace is named where it is made: the field under the list, and return. */
function NewWorkspaceField() {
  const [value, setValue] = useState("");
  const empty = orderedWorkspaces.value.length === 0;
  return (
    <label class="new-workspace" title="Novo workspace (Ctrl+Shift+N)">
      <icon.Plus />
      <input
        value={value}
        placeholder={empty ? "Nome do primeiro workspace" : "Novo workspace"}
        spellcheck={false}
        onInput={(e) => setValue((e.currentTarget as HTMLInputElement).value)}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key !== "Enter") return;
          const name = value.trim();
          if (!name) return;
          setValue("");
          void createWorkspace(name, null);
        }}
      />
    </label>
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
        <ZoomControl />
        <button class="round-button" title="Esconder a barra lateral" aria-label="Esconder a barra lateral" onClick={toggleSidebar}>
          <icon.Sidebar />
        </button>
      </div>
      <div class="sidebar-list">
        {orderedWorkspaces.value.map((w) => (
          <WorkspaceGroup key={w.name} name={w.name} />
        ))}
        <NewWorkspaceField />
      </div>
      <UsageFooter />
      {notice.value && (
        <div class="sidebar-foot">
          <div class="notice">{notice.value}</div>
        </div>
      )}
      <Resizer />
    </aside>
  );
}
