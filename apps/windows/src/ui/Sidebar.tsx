// The sidebar: every workspace, in the person's order, with its tabs under it.
//
// It runs the full height of the window, Finder-style. A workspace's dot says
// whether something runs in it — filled while busy, hollow at rest, so the
// shape carries the state as well as the hue (DESIGN.md). Folded, a workspace
// still says how many of its tabs wait on you.

import { useRef, useState } from "preact/hooks";
import { SPLIT_DOWN, SPLIT_RIGHT } from "../api";
import { endedADrag } from "../drag";
import type { RootTab } from "../layout";
import { mayArmRename, slowRename } from "../naming";
import { SIDEBAR_MAX, SIDEBAR_MIN, tabKey } from "../state";
import {
  activeTab,
  activeWorkspace,
  closeTab,
  closeWorkspace,
  createWorkspace,
  editing,
  editingTabOn,
  finishTabName,
  finishWorkspaceName,
  gui,
  isMac,
  moveWorkspace,
  newTab,
  notice,
  putAway,
  select,
  setSidebarWidth,
  shownWorkspaces,
  split,
  startRename,
  tabActivity,
  tabBusy,
  tabLabel,
  tabsByWorkspace,
  toggleFolded,
  toggleSidebar,
  workspaceLabel,
  workspaces,
  zoom,
  zoomReset,
} from "../store";
import { busyMark, plainTitle, programOf } from "../claude";
import { tabAccountLabel } from "../ia";
import { tabAccount, usageLines } from "../iaStore";
import { homeRelative } from "../text";
import { info } from "../store";
import { workspaceState, workspaceTooltip } from "../tooltips";
import { zoomPercent } from "../zoom";
import { Battery } from "./Battery";
import { AccountChevron } from "./AccountMenu";
import { ConfineButton, confineContextItems } from "./ConfineButton";
import { InlineEditor, openMenu } from "./common";
import * as icon from "./icons";
import { PaneMark } from "./paneIcons";
import { sidebarMove, startSidebarTabDrag, startWorkspaceDrag } from "./tabDrag";
import { tabLook, useBreath } from "./tabLook";
import { MemoryMeter } from "./MemoryMeter";
import { UsageFooter } from "./UsageFooter";

function TabRow(props: { workspace: string; tab: RootTab; tabs: RootTab[]; index: number; selected: boolean }) {
  const { workspace, tab, tabs, index, selected } = props;
  const look = tabLook(workspace, tab, index, selected);
  const badge = useBreath(look.breathing);
  const renaming = editingTabOn("sidebar", workspace, tab.root.id);
  const rename = () => startRename(workspace, tab.root.id, "sidebar");
  const close = () => void closeTab(workspace, tab, look.label);
  // Carried by hand: the tab dims where it will land and the others slide to
  // leave the gap; another workspace's tab under the carried one lights.
  const carry = sidebarMove.value?.kind === "tab" ? sidebarMove.value : null;
  const mine = carry?.workspace === workspace;
  const carried = mine && carry.from === index;
  const offered = carry !== null && carry.into === workspace && carry.before === tab.root.id;
  const shown = mine ? carry.slid[index] : 0;
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
      class={`tab-row ${selected ? "selected" : ""} ${look.wantsYou ? "wants" : ""} ${carried ? "carried" : ""} ${offered ? "offered" : ""}`}
      style={look.wantsYou || shown ? { background: look.wantsYou ? look.fill : undefined, transform: shown ? `translateY(${shown}px)` : undefined } : undefined}
      title={where || undefined}
      data-row={tabKey(workspace, tab.root.id)}
      onPointerDown={(e) => startSidebarTabDrag(e, workspace, tabs, index)}
      onClick={(e) => {
        // The click that ends a drag is not a click on the row.
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
      onMouseDown={(e) => {
        if (e.button === 1) {
          e.preventDefault();
          close();
        }
      }}
      onContextMenu={(e) =>
        openMenu(e, [
          { label: "Renomear", hint: "F2", action: rename },
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
          onDone={(value, keyboard) => finishTabName(workspace, tab.root.id, value, keyboard)}
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
          <span class="tab-title" style={look.color ? { color: look.color } : undefined} onPointerLeave={() => slowRename.disarm()}>
            {look.label}
          </span>
          {tab.panes.length > 1 && <PaneMark />}
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
  const rename = () => (editing.value = { kind: "workspace", workspace: name });
  const selectedTab = activeTab.value;
  const ordered = shownWorkspaces.value.map((w) => w.name);
  const position = ordered.indexOf(name);
  // Carried by hand: this group goes where the gap is and its header dims,
  // the others slide aside; a tab of another workspace over the header lights it.
  const carry = sidebarMove.value;
  const group = carry?.kind === "workspace" ? carry : null;
  const carried = group?.name === name;
  const shown = group ? group.slid[position] ?? 0 : 0;
  const offered = carry?.kind === "tab" && carry.into === name && carry.before === null;
  const live = workspaces.value.find((w) => w.name === name)?.tabs.filter((t) => !t.finished) ?? [];
  const tooltip = workspaceTooltip({
    name,
    label: workspaceLabel(name),
    tabs: tabs.length,
    state: workspaceState(live),
    // What runs, by the title it is shown under; a pane beside the root is named by its own.
    running: tabs.flatMap((tab, i) =>
      tab.panes.filter((p) => p.busy).map((p) => (p.id === tab.root.id ? tabLabel(name, tab, i) : plainTitle(p.title, p.command || `painel ${p.id}`))),
    ),
  });
  return (
    <section
      class={`workspace ${current ? "current" : ""}`}
      data-workspace={name}
      style={shown ? { transform: `translateY(${shown}px)` } : undefined}
    >
      <div
        class={`workspace-row ${carried ? "carried" : ""} ${offered ? "offered" : ""}`}
        title={tooltip}
        onPointerDown={(e) => !renaming && startWorkspaceDrag(e, name)}
        onClick={(e) => {
          // The click that ends a drag is not a click on the header.
          if (endedADrag()) return;
          select(name);
          // Finder's slow click: the name of the workspace in front, clicked once, opens for typing.
          slowRename.disarm();
          const onName = (e.target as HTMLElement).closest(".workspace-name") !== null;
          if (mayArmRename({ wasChosen: current, onName, dragged: false, count: e.detail })) {
            slowRename.arm(rename, () => activeWorkspace.value === name);
          }
        }}
        onDblClick={() => (slowRename.disarm(), rename())}
        onContextMenu={(e) =>
          openMenu(e, [
            { label: "Nova aba aqui", action: () => void newTab(name) },
            { label: "Renomear", action: rename },
            { label: folded ? "Mostrar as abas" : "Recolher as abas", action: () => toggleFolded(name) },
            ...confineContextItems(name),
            "separator",
            { label: "Mover para cima", disabled: position === 0, action: () => moveWorkspace(name, ordered[position - 1]) },
            {
              label: "Mover para baixo",
              disabled: position === ordered.length - 1,
              action: () => moveWorkspace(name, ordered[position + 2] ?? null),
            },
            "separator",
            {
              label: "Remover da barra lateral",
              help: "O workspace continua rodando; Ir para (Ctrl+Shift+P) o traz de volta.",
              action: () => putAway(name),
            },
            { label: "Fechar o workspace…", danger: true, action: () => void closeWorkspace(name) },
          ])
        }
      >
        {renaming ? (
          <InlineEditor
            value={workspaceLabel(name)}
            onDone={(value, keyboard) => finishWorkspaceName(name, value, keyboard)}
          />
        ) : (
          <span class="workspace-name" onPointerLeave={() => slowRename.disarm()}>
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
        <ConfineButton name={name} />
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
      <div class={`tabs ${folded ? "folded" : ""}`} aria-hidden={folded}>
        <div class="tabs-inner">
          {tabs.map((tab, i) => (
            <TabRow
              key={tabKey(name, tab.root.id)}
              workspace={name}
              tab={tab}
              tabs={tabs}
              index={i}
              selected={current && tab === selectedTab}
            />
          ))}
        </div>
      </div>
    </section>
  );
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
  const empty = shownWorkspaces.value.length === 0;
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
      onDblClick={() => setSidebarWidth(250)}
    />
  );
}

export function Sidebar() {
  const width = gui.value.sidebar.width;
  return (
    <aside class="sidebar" style={{ width: `${width}px` }}>
      <div class={`sidebar-head ${isMac.value ? "inset" : ""}`} data-tauri-drag-region>
        <Battery />
        <ZoomControl />
        <button class="round-button" title="Esconder a barra lateral" aria-label="Esconder a barra lateral" onClick={toggleSidebar}>
          <icon.Sidebar />
        </button>
      </div>
      <div class={`sidebar-list ${sidebarMove.value ? "moving" : ""}`}>
        {shownWorkspaces.value.map((w) => (
          <WorkspaceGroup key={w.name} name={w.name} />
        ))}
        <NewWorkspaceField />
      </div>
      <MemoryMeter />
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
