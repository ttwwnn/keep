// The terminals. Every tab of every workspace is laid out here, attached and
// at the size it would have on screen — the one in front shown, the rest kept
// at the same size out of sight, so a tab you go back to is already drawn and
// its program never had to redraw for a size nobody saw.

import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import type { PaneTree } from "../layout";
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
  loaded,
  orderedWorkspaces,
  split,
  tabsByWorkspace,
  terminals,
  activeView,
} from "../store";
import { SPLIT_DOWN, SPLIT_RIGHT } from "../api";
import { openMenu } from "./common";
import * as icon from "./icons";

function PaneHost(props: { workspace: string; root: number; pane: number; visible: boolean; focused: boolean; multi: boolean }) {
  const { workspace, root, pane, visible, focused, multi } = props;
  const host = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (host.current) terminals.view(workspace, pane).mount(host.current);
  }, [workspace, pane]);
  useEffect(() => {
    terminals.existing(workspace, pane)?.setVisible(visible);
  }, [workspace, pane, visible]);
  return (
    <div
      ref={host}
      class={`pane ${multi && focused ? "focused" : ""}`}
      data-workspace={workspace}
      data-pane={pane}
      onMouseDown={() => focusPane(workspace, root, pane)}
      onContextMenu={(e) => {
        const view = terminals.existing(workspace, pane);
        const selection = view?.term.hasSelection() ?? false;
        openMenu(e, [
          { label: "Copiar", hint: "Ctrl+C", disabled: !selection, action: () => void copySelection() },
          {
            label: "Colar",
            hint: "Ctrl+V",
            action: () => void readText().then((text) => text && view?.paste(text)).catch(() => {}),
          },
          { label: "Selecionar tudo", action: () => view?.term.selectAll() },
          { label: "Limpar a tela", action: () => view?.term.clear() },
          "separator",
          { label: "Dividir à direita", hint: "Ctrl+Shift+D", action: () => (focusPane(workspace, root, pane), void split(SPLIT_RIGHT)) },
          { label: "Dividir abaixo", hint: "Ctrl+Shift+E", action: () => (focusPane(workspace, root, pane), void split(SPLIT_DOWN)) },
          ...(multi
            ? (["separator", { label: "Fechar este painel…", danger: true, action: () => void closePane(workspace, pane) }] as const)
            : []),
        ]);
      }}
    />
  );
}

function TreeView(props: { workspace: string; root: number; tree: PaneTree; visible: boolean; focused: number; multi: boolean }) {
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
      />
    );
  }
  return (
    <div class={`split ${tree.vertical ? "side-by-side" : "stacked"}`}>
      <div class="split-half">
        <TreeView {...props} tree={tree.first} />
      </div>
      <div class="split-divider" />
      <div class="split-half">
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
  const options = {
    decorations: { matchOverviewRuler: "#888888", activeMatchColorOverviewRuler: "#ffaa00", matchBackground: "#5a4a1a", activeMatchBackground: "#a87a00" },
  };
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
      {findOpen.value && <FindBar />}
    </main>
  );
}
