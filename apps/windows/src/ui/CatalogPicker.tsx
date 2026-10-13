// A list to choose one thing from with the thing itself in each row: the
// themes and the faces (the catalogues of the macOS picker). The window wears
// what the selection is on while it moves — a choice made seeing — and the
// chosen one comes back if the list is backed out of.

import type { ComponentChildren } from "preact";
import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import { activeView, overlay } from "../store";
import { fuzzyScore, matchRanges } from "../text";

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

/** The names that fit a query, best fit first; every name, in their order, for none. */
export function filterNames(names: readonly string[], query: string): string[] {
  if (query.trim() === "") return [...names];
  const scored: { name: string; score: number }[] = [];
  for (const name of names) {
    const score = fuzzyScore(query, name);
    if (score !== undefined) scored.push({ name, score });
  }
  // Array.sort is stable: names with the same fit stay in the list's order.
  return scored.sort((a, b) => a.score - b.score).map((entry) => entry.name);
}

function closeList() {
  overlay.value = null;
  requestAnimationFrame(() => activeView()?.focus());
}

interface Props {
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

  return (
    <div class="card picker catalog" role="dialog" aria-label={props.label}>
      <input
        ref={input}
        class="card-input"
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
          if (e.key === "Escape") closeList();
          else if (e.key === "ArrowDown" || (ctrl && (e.key === "n" || e.key === "j"))) (e.preventDefault(), move(index + 1));
          else if (e.key === "ArrowUp" || (ctrl && (e.key === "p" || e.key === "k"))) (e.preventDefault(), move(index - 1));
          else if (e.key === "PageDown") (e.preventDefault(), move(index + 8));
          else if (e.key === "PageUp") (e.preventDefault(), move(Math.max(0, index) - 8));
          else if (e.key === "Enter") choose(rows[index]);
        }}
      />
      <div class="card-list" ref={list}>
        {rows.map((name, i) => (
          <button
            key={name}
            class={`card-row catalog-row ${i === index ? "selected" : ""}`}
            onClick={() => move(i)}
            onDblClick={() => choose(name)}
          >
            {props.row(name, query, name === worn)}
          </button>
        ))}
        {rows.length === 0 && <div class="card-empty">{names === null ? "Procurando…" : props.none}</div>}
      </div>
      <div class="catalog-foot">
        <span>↑ ↓ para ver</span>
        <span>Enter para usar</span>
        <span>Esc para voltar</span>
      </div>
    </div>
  );
}
