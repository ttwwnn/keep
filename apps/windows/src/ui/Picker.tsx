// The card that floats over the terminal when a question is asked of the
// window: where to go, what to do, what was said. One card, because it is one
// gesture — type, move, choose — and splitting it in two would mean two of
// everything to keep in step. A port of the macOS PickerView.
//
// Four questions share it. Ir para (Ctrl+Shift+P) is a flat list: every tab
// running, latest visit first, and below them the folders worth starting
// something new in. The palette (Ctrl+Shift+Alt+P) asks what you want done.
// Find (Ctrl+F) and Search (Ctrl+Shift+F) ask the daemon, the only one that
// holds the history, in the pane in front and in everything.
//
// Flat on purpose: you do not pick a workspace and then a tab — you pick the
// thing and you are in it. A picture of what a row actually is sits beside the
// list, drawn from the screen the daemon is already keeping.

import { useEffect, useRef, useState } from "preact/hooks";
import * as api from "../api";
import { catalogOf, commandSpec, type CatalogId } from "../commands";
import { chordAction } from "../palette/actions";
import {
  commandItems,
  createItem,
  filterItems,
  firstSelectable,
  goToItems,
  grouped,
  nextSelectable,
  openFolders,
  plainRows,
  previewName,
  recency,
  sectioned,
  segments,
  selectable,
  type Lit,
  type PickerItem,
  type Row,
} from "../palette/items";
import { closeOverlay } from "../palette/overlay";
import { act, choose, dismiss } from "../palette/run";
import { hereFacts, hitRows, homeFolder, paneScope, previewTarget, visits, workspaceFacts } from "../palette/source";
import { isMac, now } from "../store";
import { Glyph } from "./Glyph";
import { KeyCaps } from "./KeyCaps";
import { ActionsPanel, Footer } from "./PickerActions";
import { Preview } from "./PickerPreview";

export type PickerMode = "picker" | "palette" | "find" | "search";

const SEARCH_LIMIT = 200;

/** How long the search waits for the typing to stop; only the newest answer is kept. */
const SEARCH_WAIT_MS = 150;

// ------------------------------------------------------------------ pieces of text

/** A string with the letters that matched lit: heavier and brighter, the ones around them stepping back — by weight and brightness, not by a colour, since colour is left to state. */
function Marked(props: { text: string; ranges?: [number, number][] }) {
  if (!props.ranges) return <>{props.text}</>;
  return (
    <span class="pk-lit">
      {segments(props.text, props.ranges).map((s, i) => (s.on ? <mark key={i}>{s.text}</mark> : s.text))}
    </span>
  );
}

/** What lit a row, on the column asked for. */
const on = (lit: Lit | undefined, column: Lit["column"]) => (lit?.column === column ? lit.ranges : undefined);

/** A folder: the road to it faint and the name it would take in ink, so the eye lands on the word that becomes the workspace. */
function Path(props: { item: PickerItem; lit: Lit | undefined }) {
  const { item, lit } = props;
  const path = item.detail || item.title;
  const parent = Math.max(path.length - item.title.length, 0);
  // A match on the name counts from where the name begins; one on the path, from the path's start.
  const shift = lit?.column === "title" ? parent : 0;
  const ranges = lit && (lit.column === "title" || lit.column === "detail") ? lit.ranges.map(([a, b]): [number, number] => [a + shift, b + shift]) : undefined;
  const cut = segments(path, ranges);
  // Cut again at the end of the road, the two halves keeping their own lit letters.
  const pieces: { text: string; on: boolean; name: boolean }[] = [];
  let at = 0;
  for (const s of cut.length > 0 ? cut : [{ text: path, on: false }]) {
    const end = at + s.text.length;
    if (at < parent && end > parent) {
      pieces.push({ text: s.text.slice(0, parent - at), on: s.on, name: false }, { text: s.text.slice(parent - at), on: s.on, name: true });
    } else {
      pieces.push({ text: s.text, on: s.on, name: at >= parent });
    }
    at = end;
  }
  return (
    <span class="pk-path">
      <span class="pk-path-text">
        {pieces.map((p, i) => (
          <span key={i} class={`${p.name ? "name" : "road"} ${p.on ? "on" : ""}`}>
            {p.text}
          </span>
        ))}
      </span>
    </span>
  );
}

// ------------------------------------------------------------------ rows

function Entry(props: {
  row: Row;
  selected: boolean;
  lit: Lit | undefined;
  clock: number;
  onPick(): void;
  onChoose(): void;
  onMenu(): void;
}) {
  const { row, lit } = props;
  if (row.type === "heading") return <div class="pk-heading">{row.text}</div>;
  const item = row.item;
  const kind = item.kind;
  let body;
  switch (kind.type) {
    case "running": {
      const ago = recency(item.lastActive, props.clock);
      body = (
        <>
          <span class="pk-context">
            <Marked text={item.context} ranges={on(lit, "context")} />
          </span>
          <span class={`pk-badge ${item.busy ? "busy" : ""}`}>
            <Glyph name={item.busy ? "terminalBusy" : "terminal"} size={13} />
          </span>
          <span class="row-title pk-title">
            <Marked text={item.title} ranges={on(lit, "title")} />
          </span>
          {item.command && <span class="pk-command">— {item.command}</span>}
          <span class="row-detail pk-trailing">
            <Marked text={item.detail} ranges={on(lit, "detail")} />
            {item.detail && ago ? "  " : ""}
            {ago}
          </span>
        </>
      );
      break;
    }
    case "destination":
      body = (
        <>
          <span class="pk-badge dim">
            <Glyph name="folder" size={13} />
          </span>
          <Path item={item} lit={lit} />
        </>
      );
      break;
    case "command": {
      const spec = commandSpec(kind.command);
      body = (
        <>
          <span class="pk-icon">
            <Glyph name={spec.icon} />
          </span>
          <span class="pk-command-title">
            <Marked text={item.title} ranges={on(lit, "title")} />
          </span>
          <KeyCaps keys={spec.keys} />
        </>
      );
      break;
    }
    case "hit": {
      // The matched text marked the way a browser marks it: a tinted run inside the line rather than a differently coloured line.
      const [from, to] = kind.span ?? [0, 0];
      body = (
        <>
          <span class="pk-hit-line">
            {to > from ? (
              <>
                {item.title.slice(0, from)}
                <mark>{item.title.slice(from, to)}</mark>
                {item.title.slice(to)}
              </>
            ) : (
              item.title
            )}
          </span>
          <span class="pk-hit-where">
            <span>{item.detail}</span>
          </span>
        </>
      );
      break;
    }
    case "create":
      body = (
        <>
          <span class="pk-icon">
            <Glyph name="folderPlus" />
          </span>
          <span class="pk-command-title">{item.title}</span>
        </>
      );
      break;
  }
  const command = kind.type === "command" ? commandSpec(kind.command) : undefined;
  return (
    <div
      class={`card-row pk-row ${kind.type} ${props.selected ? "selected" : ""} ${item.disabled ? "disabled" : ""} ${command?.destructive ? "danger" : ""}`}
      role="option"
      aria-selected={props.selected}
      aria-disabled={item.disabled}
      onClick={() => !item.disabled && props.onPick()}
      onDblClick={() => !item.disabled && props.onChoose()}
      onContextMenu={(e) => {
        e.preventDefault();
        if (!item.disabled) props.onMenu();
      }}
    >
      {body}
    </div>
  );
}

// ------------------------------------------------------------------ what the card asks of the window

/** The folders to start something in: zoxide's, a moment after the card opens — the list is up on what is known and grows when they come. */
function useFolders(active: boolean): readonly string[] | null {
  const [folders, setFolders] = useState<readonly string[] | null>(null);
  useEffect(() => {
    if (!active) return;
    let gone = false;
    api.directories().then(
      (list) => !gone && setFolders(list),
      () => !gone && setFolders([]),
    );
    return () => {
      gone = true;
    };
  }, [active]);
  return folders;
}

/** Every keystroke is a question for the daemon; they do not come back in order, and only the newest answer is kept. */
function useSearch(active: boolean, query: string, scope: { workspace: string; pane: number } | null) {
  const [answer, setAnswer] = useState<{ asked: string; hits: api.SearchHit[] }>({ asked: "", hits: [] });
  const where = scope ? `${scope.workspace}\u0000${scope.pane}` : "";
  useEffect(() => {
    if (!active) return;
    const asked = query.trim();
    if (!asked) {
      setAnswer({ asked: "", hits: [] });
      return;
    }
    let stale = false;
    const timer = setTimeout(() => {
      api.search(asked, SEARCH_LIMIT, scope?.workspace, scope?.pane).then(
        (hits) => !stale && setAnswer({ asked, hits }),
        () => !stale && setAnswer({ asked, hits: [] }),
      );
    }, SEARCH_WAIT_MS);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [active, query, where]);
  return { hits: answer.hits, answered: answer.asked === query.trim() };
}

// ------------------------------------------------------------------ the card

const PLACEHOLDER = {
  picker: "Ir para um terminal, ou uma pasta onde abrir um…",
  palette: "O que você quer fazer…",
  search: "Buscar em tudo…",
} as const;

export function Picker(props: { mode: PickerMode }) {
  const { mode } = props;
  const searching = mode === "find" || mode === "search";
  const [query, setQuery] = useState("");
  const [picked, setPicked] = useState<string | null>(null);
  const [acting, setActing] = useState(false);
  const [catalog, setCatalog] = useState<CatalogId | null>(null);
  const field = useRef<HTMLInputElement>(null);
  const rowsBox = useRef<HTMLDivElement>(null);
  const swallow = useRef(false);

  const scope = mode === "find" ? paneScope() : null;
  const folders = useFolders(mode === "picker");
  // Searching the pane in front, with none in front, would search everything.
  const { hits, answered } = useSearch(searching && (mode !== "find" || scope !== null), query, scope);

  // The list: what is asked for, scored by the query, under its headings.
  const facts = workspaceFacts();
  let rows: Row[];
  let lit = new Map<string, Lit>();
  if (mode === "picker") {
    const directories = folders === null ? [] : folders.length > 0 ? folders : openFolders(facts, homeFolder());
    const base = goToItems({ workspaces: facts, recent: visits(), here: hereFacts(), directories, home: homeFolder() });
    const filtered = filterItems(base, query);
    lit = filtered.lit;
    const made = createItem(query, facts);
    rows = sectioned(made ? [...filtered.kept, made] : filtered.kept);
  } else if (mode === "palette") {
    const filtered = filterItems(commandItems(), query);
    lit = filtered.lit;
    // Headings only on the untyped list: scoring mixes the groups together, and a heading over one row of its own kind and four of another is worse than none.
    rows = query.trim() === "" ? grouped(filtered.kept) : plainRows(filtered.kept);
  } else {
    // The daemon has already decided what matches, and filtering its answer again with another rule would hide real hits.
    rows = plainRows(hitRows(hits));
  }

  let at = picked === null ? -1 : rows.findIndex((r) => selectable(r) && r.item.id === picked);
  if (at < 0) at = firstSelectable(rows);
  const row = rows[at];
  const selected = row && selectable(row) ? row.item : undefined;

  useEffect(() => field.current?.focus(), []);
  useEffect(() => {
    const box = rowsBox.current;
    if (!box) return;
    // The first row brings its heading with it, which the fade at the top would otherwise take.
    if (at === firstSelectable(rows)) box.scrollTop = 0;
    else box.children[at]?.scrollIntoView({ block: "nearest" });
  }, [selected?.id]);

  const openCatalog = (id: CatalogId) => setCatalog(id);
  const refocus = () => requestAnimationFrame(() => field.current?.focus());
  const closeActions = () => {
    setActing(false);
    refocus();
  };
  const step = (by: 1 | -1) => {
    const next = rows[nextSelectable(rows, at, by)];
    if (next && selectable(next)) setPicked(next.item.id);
  };
  const open = (item: PickerItem | undefined = selected) => {
    if (item) choose(item, openCatalog);
  };

  /** The list's keys. Whatever it does not use is left to bubble, to the card and the window: the chords that switch cards work from here. */
  const onFieldKey = (e: KeyboardEvent) => {
    // A composition being committed with Enter or cancelled with Esc is the input method's, not the list's.
    if (e.isComposing) return;
    // The field keeps the keyboard: Tab would walk it off to the chips, and what is typed then goes nowhere.
    if (e.key === "Tab" && !e.ctrlKey && !e.altKey && !e.metaKey) {
      e.preventDefault();
      return;
    }
    const control = e.ctrlKey && !e.shiftKey && !e.altKey && !e.metaKey;
    // ⌃J and ⌃K move down and up the list, and ⌃N and ⌃P too. On Windows Ctrl+K is the actions' (⌘K on the Mac), so up is ↑ and Ctrl+P there.
    if (e.key === "ArrowDown" || (control && (e.code === "KeyJ" || e.code === "KeyN"))) step(1);
    else if (e.key === "ArrowUp" || (control && (e.code === "KeyP" || (isMac.value && e.code === "KeyK")))) step(-1);
    else if (e.key === "Enter") open();
    else if (e.key === "Escape") closeOverlay();
    // Ctrl+D on a row ends what it points at. The field is empty-handed here, so it is unambiguous.
    else if (control && e.code === "KeyD") {
      if (selected) void dismiss(selected).then(refocus);
    } else return;
    e.preventDefault();
    e.stopPropagation();
  };

  /** The chords that are the card's, whichever of its fields has the keyboard. */
  const onCardKey = (e: KeyboardEvent) => {
    const mod = isMac.value ? e.metaKey : e.ctrlKey;
    if (!mod || e.altKey) return;
    if (!e.shiftKey && e.code === "KeyK") {
      if (acting) closeActions();
      else if (selected) setActing(true);
    } else if (e.shiftKey && (e.code === "KeyR" || e.code === "KeyC")) {
      // The two the panel advertises, so that what it says about them is true without opening it. Ctrl+Shift+F is not among them: it is Search, and this is the last place to take it.
      const action = selected ? chordAction(selected, e.code === "KeyR" ? "reveal" : "copy") : null;
      if (selected && action) void act(action, selected, openCatalog);
    } else return;
    e.preventDefault();
    e.stopPropagation();
  };

  /** A click inside the card keeps the field focused; one outside the panel only puts the panel away, and does nothing else. */
  const onCardMouseDown = (e: MouseEvent) => {
    const target = e.target as HTMLElement;
    if (acting && !target.closest(".pk-actions")) {
      swallow.current = true;
      // A press that is not followed by a click must not eat the next one.
      setTimeout(() => (swallow.current = false), 400);
      closeActions();
      e.preventDefault();
      return;
    }
    if (!(target instanceof HTMLInputElement)) e.preventDefault();
  };

  const Catalog = catalog ? catalogOf(catalog) : undefined;
  if (Catalog) {
    return (
      <div class="card picker catalog" role="dialog" aria-label={commandSpec(catalog === "themes" ? "chooseTheme" : "chooseFont").title}>
        <Catalog onBack={() => setCatalog(null)} onDone={closeOverlay} />
      </div>
    );
  }

  const placeholder = mode === "find" ? `Buscar em ${scope?.label ?? "este painel"}…` : PLACEHOLDER[mode];
  const counter = !searching
    ? ""
    : rows.length === 0
      ? query.trim() && answered
        ? "sem resultados"
        : ""
      : `${at + 1} de ${rows.length}`;
  const label = { picker: "Ir para", palette: "Comandos", find: "Buscar neste painel", search: "Buscar em tudo" }[mode];

  return (
    <div
      class={`card picker pk-${mode}`}
      role="dialog"
      aria-label={label}
      onKeyDown={onCardKey}
      onMouseDown={onCardMouseDown}
      onClickCapture={(e) => {
        if (!swallow.current) return;
        swallow.current = false;
        e.stopPropagation();
        e.preventDefault();
      }}
    >
      <div class="pk-head">
        <input
          ref={field}
          class="card-input pk-field"
          value={query}
          placeholder={placeholder}
          spellcheck={false}
          onInput={(e) => {
            setQuery((e.currentTarget as HTMLInputElement).value);
            setPicked(null);
          }}
          onKeyDown={onFieldKey}
        />
        <span class="pk-counter">{counter}</span>
      </div>
      <div class="pk-list">
        <div class="card-list pk-rows" role="listbox" ref={rowsBox}>
          {rows.map((r, i) => (
            <Entry
              key={r.type === "heading" ? `h:${r.text}` : r.item.id}
              row={r}
              selected={i === at}
              lit={r.type === "item" ? lit.get(r.item.id) : undefined}
              clock={now.value}
              onPick={() => r.type === "item" && setPicked(r.item.id)}
              onChoose={() => r.type === "item" && open(r.item)}
              onMenu={() => {
                if (r.type !== "item") return;
                // The right button selects what it is over and then asks the same question Ctrl+K asks: a menu that acts on a row other than the one under the pointer does something else than it was asked to.
                setPicked(r.item.id);
                setActing(true);
              }}
            />
          ))}
          {rows.length === 0 && query.trim() !== "" && (!searching || answered) && (
            <div class="card-empty">{searching ? "Nada encontrado." : "Nada com esse nome."}</div>
          )}
        </div>
      </div>
      <Preview target={previewTarget(selected)} title={selected ? previewName(selected) : ""} shown={selected !== undefined} />
      {selected && <Footer onOpen={() => open()} onActions={() => (acting ? closeActions() : setActing(true))} />}
      {acting && selected && (
        <ActionsPanel
          item={selected}
          onClose={closeActions}
          onRun={(action) => {
            setActing(false);
            // Closed before the action runs: several of these take the app somewhere else, and a panel still up when the Explorer comes forward is a panel that comes back with it.
            void act(action, selected, openCatalog).then(() => action === "close" && refocus());
          }}
        />
      )}
    </div>
  );
}
