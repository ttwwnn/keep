// A list to choose one thing from with the thing itself in each row: the
// themes and the faces (the catalogues of the macOS picker). The window wears
// what the selection is on while it moves — a choice made seeing — and the
// chosen one comes back if the list is backed out of. Ctrl+K (⌘K on the Mac)
// or the right button opens what can be done to the row, as in the picker:
// wear it, or copy its name, which Ctrl+Shift+C also does.

import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import type { ComponentChildren } from "preact";
import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import { filterNames } from "../catalog";
import { catalogActions } from "../palette/actions";
import { activeView, isMac, overlay } from "../store";
import { matchRanges } from "../text";
import { ActionsPanel } from "./PickerActions";

/** The name in a row, with the letters the query matched in bold. */
export function Marked(props: { name: string; query: string }) {
  const ranges = matchRanges(props.query, props.name);
  if (ranges.length === 0) return <>{props.name}</>;
  const parts: ComponentChildren[] = [];
  let at = 0;
  for (const [start, end] of ranges) {
    if (start > at) parts.push(props.name.slice(at, start));
    parts.push(<mark key={start}>{props.name.slice(start, end)}</mark>);
    at = end;
  }
  parts.push(props.name.slice(at));
  return <>{parts}</>;
}

function closeList() {
  overlay.value = null;
  requestAnimationFrame(() => activeView()?.focus());
}

interface Props {
  /** What the rows are, which names what the actions panel offers. */
  kind: "theme" | "font";
  /** The label the screen reader reads the card as. */
  label: string;
  placeholder: string;
  /** Every name in the order to list them in; null while they are being found. */
  names: readonly string[] | null;
  /** What to say when there is none to list. */
  none: string;
  /** The one worn now: marked, and where the selection starts. */
  worn: string;
  /** What a row holds: the name, and whatever shows it better than the name does. */
  row(name: string, query: string, worn: boolean): ComponentChildren;
  /** Wear a name for as long as it is looked at; null puts back the one chosen. */
  onLook(name: string | null): void;
  onChoose(name: string): void;
}

export function CatalogPicker(props: Props) {
  const { names, worn, onLook, onChoose } = props;
  const [query, setQuery] = useState("");
  const rows = useMemo(() => filterNames(names ?? [], query), [names, query]);
  // Where the selection starts: on the one worn, or on nothing, so that the
  // first arrow goes to the first row rather than past it.
  const [index, setIndex] = useState(-1);
  // Nothing is worn for the looking until the person asks it: opening the list
  // changes nothing.
  const [looking, setLooking] = useState(false);
  // The actions panel over the selected row.
  const [acting, setActing] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);

  useEffect(() => input.current?.focus(), []);
  // The list arrives later than the card, when the names have to be found.
  useEffect(() => {
    if (!looking) setIndex(rows.indexOf(worn));
  }, [rows, worn, looking]);
  useEffect(() => {
    (list.current?.children[index] as HTMLElement | undefined)?.scrollIntoView({ block: "nearest" });
  }, [index, rows]);

  useEffect(() => {
    if (!looking) return;
    // Held back a moment so that an arrow held down does not dress the whole window in every row it passes.
    const timer = setTimeout(() => onLook(rows[index] ?? null), 40);
    return () => clearTimeout(timer);
  }, [looking, rows, index]);
  // Backed out of, or chosen, or closed by a click outside: what was chosen comes back.
  useEffect(() => () => onLook(null), []);

  const move = (to: number) => {
    if (rows.length === 0) return;
    setLooking(true);
    setIndex(Math.min(rows.length - 1, Math.max(0, to)));
  };
  const choose = (name: string | undefined) => {
    if (name === undefined) return;
    onChoose(name);
    closeList();
  };
  const copyName = (name: string | undefined) => {
    if (name !== undefined) void writeText(name).catch(() => undefined);
  };
  const closeActions = () => {
    setActing(false);
    requestAnimationFrame(() => input.current?.focus());
  };
  const selected = rows[index];

  return (
    <div
      class="card catalog"
      role="dialog"
      aria-label={props.label}
      // A press in the card outside the panel only puts the panel away.
      onMouseDown={(e) => {
        if (acting && !(e.target as HTMLElement).closest(".pk-actions")) {
          e.preventDefault();
          closeActions();
        }
      }}
    >
      <input
        ref={input}
        class="catalog-input"
        value={query}
        placeholder={props.placeholder}
        spellcheck={false}
        onInput={(e) => {
          setQuery((e.currentTarget as HTMLInputElement).value);
          setLooking(true);
          setIndex(0);
        }}
        onKeyDown={(e) => {
          e.stopPropagation();
          const ctrl = e.ctrlKey && !e.shiftKey && !e.altKey;
          // The actions' chord: ⌘ on the Mac, Ctrl here — where Ctrl+K is theirs, so up is ↑ and Ctrl+P (as in the picker).
          const mod = (isMac.value ? e.metaKey : e.ctrlKey) && !e.altKey;
          if (mod && !e.shiftKey && e.code === "KeyK") (e.preventDefault(), selected !== undefined && setActing(true));
          else if (mod && e.shiftKey && e.code === "KeyC") (e.preventDefault(), copyName(selected));
          else if (e.key === "Escape") closeList();
          else if (e.key === "ArrowDown" || (ctrl && (e.key === "n" || e.key === "j"))) (e.preventDefault(), move(index + 1));
          else if (e.key === "ArrowUp" || (ctrl && (e.key === "p" || (isMac.value && e.key === "k")))) (e.preventDefault(), move(index - 1));
          else if (e.key === "PageDown") (e.preventDefault(), move(index + 8));
          else if (e.key === "PageUp") (e.preventDefault(), move(Math.max(0, index) - 8));
          else if (e.key === "Enter") choose(rows[index]);
          // The list has nowhere else to go: Tab would leave it for the sidebar behind.
          else if (e.key === "Tab") e.preventDefault();
        }}
      />
      <div class="catalog-list" ref={list}>
        {rows.map((name, i) => (
          <button
            key={name}
            class={`catalog-row ${i === index ? "selected" : ""}`}
            // The keyboard stays in the field, which is where the arrows are read.
            tabIndex={-1}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => move(i)}
            onDblClick={() => choose(name)}
            onContextMenu={(e) => {
              e.preventDefault();
              move(i);
              setActing(true);
            }}
          >
            {props.row(name, query, name === worn)}
          </button>
        ))}
        {rows.length === 0 && <div class="catalog-empty">{names === null ? "Procurando…" : props.none}</div>}
      </div>
      <div class="catalog-foot">
        <span>↑ ↓ para ver</span>
        <span>Enter para usar</span>
        <span>{isMac.value ? "⌘K" : "Ctrl+K"} para ações</span>
        <span>Esc para voltar</span>
      </div>
      {acting && selected !== undefined && (
        <ActionsPanel
          heading={selected}
          entries={catalogActions(props.kind)}
          onClose={closeActions}
          onRun={(action) => {
            if (action === "open") choose(selected);
            else {
              copyName(selected);
              closeActions();
            }
          }}
        />
      )}
    </div>
  );
}
