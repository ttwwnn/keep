// The terminals. Every tab of every workspace is laid out here, attached and
// at the size it would have on screen — the one in front shown, the rest kept
// at the same size out of sight, so a tab you go back to is already drawn and
// its program never had to redraw for a size nobody saw.

import { useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import type { PaneTree } from "../layout";
import { awake } from "../hibernation";
import { shareOf, splitKey } from "../splits";
import { tabKey } from "../state";
import {
  activePane,
  activeTab,
  activeWorkspace,
  closePane,
  copySelection,
  newWorkspace,
  daemonError,
  findOpen,
  focusPane,
  isMac,
  loaded,
  orderedWorkspaces,
  palette,
  shownWorkspaces,
  split,
  statuses,
  tabsByWorkspace,
  terminals,
  activeView,
} from "../store";
import { SPLIT_DOWN, SPLIT_RIGHT } from "../api";
import { imagePasteSequence } from "../keys";
import { toggleOverlay } from "../palette/overlay";
import { viewKey } from "../terminals";
import { severalWindows } from "../windowPeers";
import { AiStatusBar } from "./AiStatusBar";
import { openMenuAt, type MenuItem } from "./common";
import { centreDivider, startDividerDrag } from "./divider";
import * as icon from "./icons";
import { PaneCarryOverlay, PaneGrip } from "./PaneCarry";

function PaneHost(props: {
  workspace: string;
  root: number;
  pane: number;
  visible: boolean;
  focused: boolean;
  multi: boolean;
  asleep: boolean;
}) {
  const { workspace, root, pane, visible, focused, multi, asleep } = props;
  const host = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    // Asleep, the pane has no terminal (hibernation.ts); showing the tab builds one, repainted from the daemon.
    if (asleep) {
      terminals.sleep(workspace, pane);
      return;
    }
    const woke = terminals.existing(workspace, pane) === undefined;
    const view = terminals.view(workspace, pane);
    if (host.current) view.mount(host.current);
    if (woke && visible && focused) view.focus();
  }, [workspace, pane, asleep]);
  useEffect(() => {
    terminals.existing(workspace, pane)?.setVisible(visible);
  }, [workspace, pane, visible, asleep]);
  // The bar under a Claude Code or Codex conversation takes its height from
  // the pane only while it has something to say.
  const status = statuses.value.get(viewKey(workspace, pane));
  return (
    <div class="pane-column">
      {multi && <PaneGrip workspace={workspace} root={root} pane={pane} />}
      <div
        ref={host}
        class={`pane ${multi && focused ? "focused" : ""}`}
        data-workspace={workspace}
        data-pane={pane}
        onMouseDown={() => focusPane(workspace, root, pane)}
        onContextMenu={(e) => {
          const view = terminals.existing(workspace, pane);
          const selection = view?.term.hasSelection() ?? false;
          const items: (MenuItem | "separator")[] = [
            { label: "Copiar", hint: "Ctrl+C", disabled: !selection, action: () => void copySelection() },
            {
              label: "Colar",
              hint: "Ctrl+V",
              action: () => void view?.pasteFromSystem().catch(() => {}),
            },
            {
              label: "Colar imagem",
              hint: "Ctrl+Shift+V",
              action: () => void view?.type(imagePasteSequence(isMac.value)).catch(() => {}),
            },
            { label: "Selecionar tudo", action: () => view?.term.selectAll() },
            { label: "Limpar a tela", hint: "Ctrl+Shift+L", action: () => void view?.type("\x0c").catch(() => {}) },
            "separator",
            { label: "Buscar neste painel…", hint: "Ctrl+F", action: () => toggleOverlay("find") },
            { label: "Procurar na tela", hint: "Ctrl+Alt+F", action: () => (findOpen.value = true) },
            "separator",
            { label: "Dividir à direita", hint: "Ctrl+Shift+D", action: () => (focusPane(workspace, root, pane), void split(SPLIT_RIGHT)) },
            { label: "Dividir abaixo", hint: "Ctrl+Shift+E", action: () => (focusPane(workspace, root, pane), void split(SPLIT_DOWN)) },
            ...(multi
              ? ([
                  "separator",
                  // Said here, where a person looks for what a pane can do; the chords are the window's own.
                  { label: "Mover o foco entre painéis", hint: "Alt+setas", disabled: true, action: () => {} },
                  { label: "Redimensionar o painel", hint: "Alt+Shift+setas", disabled: true, action: () => {} },
                  "separator",
                  { label: "Fechar este painel…", danger: true, action: () => void closePane(workspace, pane) },
                ] as const)
              : []),
          ];
          // A file or folder under the pointer has its own items, in front, once the file system says it is there.
          e.preventDefault();
          e.stopPropagation();
          const { clientX, clientY } = e;
          const ofPath = view ? Promise.race([view.links.menu(e), new Promise<[]>((resolve) => setTimeout(() => resolve([]), 400))]) : Promise.resolve([]);
          void ofPath.then((path) => openMenuAt(clientX, clientY, path.length > 0 ? [...path, "separator", ...items] : items));
        }}
      />
      {status && <AiStatusBar workspace={workspace} root={root} pane={pane} status={status} />}
    </div>
  );
}

function TreeView(props: {
  workspace: string;
  root: number;
  tree: PaneTree;
  visible: boolean;
  focused: number;
  multi: boolean;
  asleep: boolean;
}) {
  const { tree } = props;
  if (tree.kind === "leaf") {
    return (
      <PaneHost
        key={tree.id}
        workspace={props.workspace}
        root={props.root}
        pane={tree.id}
        visible={props.visible}
        focused={props.focused === tree.id}
        multi={props.multi}
        asleep={props.asleep}
      />
    );
  }
  // How the two halves share the room: one half each until a divider is moved.
  const key = splitKey(tabKey(props.workspace, props.root), tree);
  const share = shareOf(key);
  return (
    <div class={`split ${tree.vertical ? "side-by-side" : "stacked"}`} data-split={key}>
      <div class="split-half" style={{ flexGrow: share }}>
        <TreeView {...props} tree={tree.first} />
      </div>
      <div
        class="split-divider"
        role="separator"
        aria-orientation={tree.vertical ? "vertical" : "horizontal"}
        onPointerDown={(e) => startDividerDrag(e, key, tree.vertical)}
        onDblClick={() => centreDivider(key)}
      />
      <div class="split-half" style={{ flexGrow: 1 - share }}>
        <TreeView {...props} tree={tree.second} />
      </div>
    </div>
  );
}

function FindBar() {
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<{ index: number; count: number } | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const view = activeView();
  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, []);
  useEffect(() => {
    if (!view) return;
    const subscription = view.search.onDidChangeResults((r) =>
      setResult(r ? { index: r.resultIndex, count: r.resultCount } : null),
    );
    return () => subscription.dispose();
  }, [view]);
  // The matches are marked in colours that read on the theme's ground, whichever it is.
  const options = { decorations: palette.value.search };
  const close = () => {
    view?.search.clearDecorations();
    findOpen.value = false;
    view?.focus();
  };
  return (
    <div class="find-bar" role="search">
      <input
        ref={input}
        value={query}
        placeholder="Buscar nesta aba"
        spellcheck={false}
        onInput={(e) => {
          const q = (e.currentTarget as HTMLInputElement).value;
          setQuery(q);
          if (q) view?.search.findNext(q, { ...options, incremental: true });
          else view?.search.clearDecorations();
        }}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Escape") close();
          else if (e.key === "Enter" && query) {
            if (e.shiftKey) view?.search.findPrevious(query, options);
            else view?.search.findNext(query, options);
          }
        }}
      />
      <span class="find-count">{result && result.count > 0 ? `${result.index + 1}/${result.count}` : query ? "0" : ""}</span>
      <button class="icon-button" title="Anterior (Shift+Enter)" onClick={() => query && view?.search.findPrevious(query, options)}>
        <icon.Up />
      </button>
      <button class="icon-button" title="Próximo (Enter)" onClick={() => query && view?.search.findNext(query, options)}>
        <icon.Down />
      </button>
      <button class="icon-button" title="Fechar (Esc)" onClick={close}>
        <icon.Close />
      </button>
    </div>
  );
}

export function TerminalArea() {
  const shownWorkspace = activeWorkspace.value;
  const shownTab = activeTab.value;
  const focused = activePane.value;
  const all = orderedWorkspaces.value;
  return (
    <main class="terminal-area">
      {daemonError.value && (
        <div class="banner" role="status">
          Sem conexão com o keepd — tentando de novo. <span class="banner-detail">{daemonError.value}</span>
        </div>
      )}
      {all.flatMap((w) =>
        (tabsByWorkspace.value.get(w.name) ?? []).map((tab) => {
          const visible = w.name === shownWorkspace && tab === shownTab;
          return (
            <div key={tabKey(w.name, tab.root.id)} class={`tab-view ${visible ? "shown" : "away"}`} aria-hidden={!visible}>
              <TreeView
                workspace={w.name}
                root={tab.root.id}
                tree={tab.tree}
                visible={visible}
                focused={visible && focused ? focused.id : tab.root.id}
                multi={tab.panes.length > 1}
                asleep={!visible && !awake.value.has(tabKey(w.name, tab.root.id))}
              />
            </div>
          );
        }),
      )}
      {loaded.value && all.length === 0 && (
        <div class="empty">
          <p>Nenhum workspace aberto.</p>
          <button class="primary" onClick={() => void newWorkspace()}>
            Novo workspace
          </button>
          <p class="hint">Ctrl+Shift+N</p>
        </div>
      )}
      {loaded.value && all.length > 0 && shownWorkspaces.value.length === 0 && (
        <div class="empty">
          {/* A window opened now lists none: every workspace there is a Go To away. */}
          <p>{severalWindows.value ? "Esta janela não tem nenhum workspace." : "Todos os workspaces estão fora da barra lateral."}</p>
          <p class="hint">Ctrl+Shift+P para {severalWindows.value ? "trazer um para cá" : "voltar a um"}</p>
        </div>
      )}
      {findOpen.value && <FindBar />}
      <PaneCarryOverlay />
    </main>
  );
}
