// The panel at the right of a Claude Code conversation, on the sidebar's ground
// and in its inks: the files the conversation made above, the ones the person
// sent below them, then the Downloads folder, each a table of name, type,
// modified and size, sorted by a click on a column's title (its arrow turns
// with a second click). A double click opens a file as a click on its name in
// the terminal does; a row dragged onto a pane types its path there, as a
// file dropped from the Explorer does. Its left edge is dragged to widen it,
// or narrower than a row can be read in to shut it. A port of
// ChatFilesPanel.swift in the macOS app.

import { useEffect, useRef, useState } from "preact/hooks";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import * as api from "../api";
import type { ChatFile, ChatFiles } from "../api";
import type { Conversation } from "../aiStatus";
import {
  chooseFilesWidth,
  draggedWidth,
  filesOpen,
  filesWidth,
  modifiedText,
  nextOrder,
  NEWEST_FIRST,
  panelMaximum,
  releasedWidth,
  sizeText,
  sortFiles,
  type Column,
  type Order,
} from "../chatFiles";
import { trackDrag } from "../drag";
import { lightPane } from "../drop";
import { typeDropped } from "../input";
import { openFound, type LinkEnv } from "../links";
import { activePane, activeWorkspace, isMac, say, statuses } from "../store";
import { viewKey } from "../terminals";
import { openMenu } from "./common";

const READ_EVERY_MS = 2000;

const env: LinkEnv = {
  flavor: "windows",
  get mac() {
    return isMac.value;
  },
  cwd: () => "",
  home: () => "",
  say: (message) => say(message),
};

const fileManager = () => (isMac.value ? "Finder" : "Explorer");

function sameFiles(a: ChatFiles | null, b: ChatFiles): boolean {
  return a !== null && JSON.stringify(a) === JSON.stringify(b);
}

/** The pane under a point of the page, where a row let go types its path. */
const paneUnder = (x: number, y: number) => document.elementFromPoint(x, y)?.closest<HTMLElement>(".pane") ?? null;

/** A row carried to a pane: a label on the pointer, the pane under it lit. */
function dragRow(down: PointerEvent, file: ChatFile): void {
  let ghost: HTMLElement | null = null;
  trackDrag(down, {
    start(at) {
      ghost = document.createElement("div");
      ghost.className = "files-ghost";
      ghost.textContent = file.path.split(/[\\/]/).pop() ?? file.path;
      document.body.appendChild(ghost);
      ghost.style.transform = `translate(${at.x + 12}px, ${at.y + 8}px)`;
    },
    move(at) {
      if (ghost) ghost.style.transform = `translate(${at.x + 12}px, ${at.y + 8}px)`;
      lightPane(paneUnder(at.x, at.y));
    },
    end(at, dropped) {
      ghost?.remove();
      lightPane(null);
      const pane = paneUnder(at.x, at.y);
      if (dropped && pane) typeDropped([file.path], pane);
    },
  });
}

const COLUMNS: { column: Column; title: string }[] = [
  { column: "name", title: "Nome" },
  { column: "type", title: "Tipo" },
  { column: "modified", title: "Modificado" },
  { column: "size", title: "Tamanho" },
];

function Table(props: {
  files: ChatFile[];
  order: Order;
  setOrder: (order: Order) => void;
  selected: string | null;
  select: (path: string) => void;
}) {
  const { files, order, setOrder, selected, select } = props;
  const open = (file: ChatFile) => void openFound(file.path, file.dir, env);
  return (
    <div class="files-table" role="table">
      <div class="files-row files-head" role="row">
        {COLUMNS.map(({ column, title }) => (
          <div
            key={column}
            role="columnheader"
            aria-sort={order.column === column ? (order.ascending ? "ascending" : "descending") : "none"}
          >
            <button
              class={`files-cell files-${column} ${order.column === column ? "sorted" : ""}`}
              title={`Ordenar por ${title.toLowerCase()}`}
              onClick={() => setOrder(nextOrder(order, column))}
            >
              {title}
              {order.column === column && <span class="files-arrow">{order.ascending ? "▲" : "▼"}</span>}
            </button>
          </div>
        ))}
      </div>
      <div class="files-body">
        {files.map((file) => (
          <div
            key={file.path}
            class={`files-row ${selected === file.path ? "selected" : ""}`}
            role="row"
            title={file.path}
            data-file={file.path}
            onPointerDown={(e) => {
              select(file.path);
              dragRow(e, file);
            }}
            onDblClick={() => open(file)}
            onContextMenu={(e) => {
              select(file.path);
              openMenu(e, [
                { label: "Abrir", action: () => open(file) },
                {
                  label: `Mostrar no ${fileManager()}`,
                  action: () => void api.revealPath(file.path).catch((error) => say(`Não foi possível mostrar: ${String(error)}`)),
                },
                {
                  label: "Copiar caminho",
                  action: () => void writeText(file.path).catch(() => navigator.clipboard?.writeText(file.path)),
                },
              ]);
            }}
          >
            <span class="files-cell files-name" role="cell">
              {file.name}
            </span>
            <span class="files-cell files-type" role="cell">
              {file.type}
            </span>
            <span class="files-cell files-modified" role="cell">
              {modifiedText(file.modified)}
            </span>
            <span class="files-cell files-size" role="cell">
              {file.dir ? "—" : sizeText(file.size)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

function Section(props: {
  title: string;
  files: ChatFile[];
  empty: string;
  order: Order;
  setOrder: (order: Order) => void;
  selected: string | null;
  select: (path: string) => void;
}) {
  const { title, files, empty } = props;
  return (
    <section class={`files-section ${files.length === 0 ? "empty" : ""}`} data-section={title}>
      <div class="files-title">
        <span class="files-title-name">{title}</span>
        <span class="files-count">{files.length}</span>
      </div>
      {files.length === 0 ? (
        <div class="files-empty">{empty}</div>
      ) : (
        <Table {...props} files={sortFiles(files, props.order)} />
      )}
    </section>
  );
}

/** The lists of one conversation, read again every two seconds while the panel shows it. */
function Lists(props: { conversation: Conversation | null }) {
  const { conversation } = props;
  const [files, setFiles] = useState<ChatFiles | null>(null);
  const [note, setNote] = useState<string | null>("Lendo a conversa…");
  const [order, setOrder] = useState<Order>(NEWEST_FIRST);
  const [selected, setSelected] = useState<string | null>(null);
  const transcript = conversation?.transcript ?? "";
  const session = conversation?.session ?? "";
  useEffect(() => {
    // Another conversation's lists are not shown while this one's are read: a double click would open its file.
    setFiles(null);
    setSelected(null);
    if (!transcript || !session) {
      setNote("Esta aba não tem conversa do Claude Code");
      return;
    }
    setNote("Lendo a conversa…");
    let live = true;
    let reading = false;
    let shown = false;
    const read = () => {
      if (reading) return;
      reading = true;
      api
        .chatFiles(transcript, session)
        .then((next) => {
          if (!live) return;
          shown = true;
          setNote(null);
          setFiles((before) => (sameFiles(before, next) ? before : next));
        })
        .catch(() => {
          // A read that fails after one that worked leaves the lists as they were; asked again in two seconds.
          if (live && !shown) setNote("Não foi possível ler a conversa");
        })
        .finally(() => (reading = false));
    };
    read();
    const timer = setInterval(read, READ_EVERY_MS);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [transcript, session]);

  if (note !== null || files === null) return <div class="files-note">{note ?? "Lendo a conversa…"}</div>;
  const shared = { order, setOrder, selected, select: setSelected };
  return (
    <div class="files-lists">
      <Section title="Gerados" files={files.made} empty="Nada gerado ainda" {...shared} />
      <Section title="Enviados" files={files.sent} empty="Nada enviado nesta conversa" {...shared} />
      <Section
        title="Downloads"
        files={files.downloads}
        empty={files.downloadsDenied ? "Sem acesso à pasta Downloads" : "A pasta Downloads está vazia"}
        {...shared}
      />
    </div>
  );
}

/** The panel's left edge: the margin between the terminal and the card, dragged as the sidebar's edge is. */
function Edge(props: { width: number; maximum: number; setLive: (width: number | null) => void }) {
  const start = useRef<{ x: number; width: number } | null>(null);
  const last = useRef(props.width);
  return (
    <div
      class="files-edge"
      role="separator"
      aria-orientation="vertical"
      aria-label="Largura do painel de arquivos"
      onPointerDown={(e) => {
        (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        start.current = { x: e.clientX, width: props.width };
        last.current = props.width;
      }}
      onPointerMove={(e) => {
        if (!start.current) return;
        last.current = draggedWidth(start.current.width, start.current.x - e.clientX, props.maximum);
        props.setLive(last.current);
      }}
      onPointerUp={() => {
        if (!start.current) return;
        start.current = null;
        props.setLive(null);
        const released = releasedWidth(last.current, filesWidth.value);
        if (!released.open) filesOpen.value = false;
        else chooseFilesWidth(released.chosen);
      }}
    />
  );
}

/** Open beside a Claude Code conversation, and only there: a tab without one keeps the terminal whole. */
export function FilesPanel() {
  const [live, setLive] = useState<number | null>(null);
  const workspace = activeWorkspace.value;
  const pane = activePane.value;
  const status = workspace && pane ? statuses.value.get(viewKey(workspace, pane.id)) : undefined;
  if (!filesOpen.value || status?.program !== "claude") return null;
  const maximum = panelMaximum(globalThis.screen?.availWidth ?? 1200);
  const width = live ?? Math.min(filesWidth.value, maximum);
  return (
    <aside class="files-panel" style={{ width: `${width}px` }} aria-label="Arquivos da conversa">
      <div class="files-card">
        <Lists conversation={status.conversation ?? null} />
      </div>
      <Edge width={width} maximum={maximum} setLive={setLive} />
    </aside>
  );
}
