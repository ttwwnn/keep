// The corner of the card: what Enter does and where the rest of it is — one
// chip of glass in the corner, not a bar across the card — and the panel that
// Ctrl+K and the right button open out of it.
//
// A full-width footer is a second surface inside a card meant to read as one,
// and it takes a strip of the list with it. This floats over the card
// instead, in the corner the panel rises out of, so the chip and the panel
// it opens are visibly the same object.

import { useEffect, useRef, useState } from "preact/hooks";
import { actionsFor, filterActions, type ActionId } from "../palette/actions";
import type { PickerItem } from "../palette/items";
import { Glyph } from "./Glyph";
import { KeyCaps } from "./KeyCaps";

/** What Enter does and where the rest of it is; gone when nothing is highlighted, rather than offering to open a row that is not there. */
export function Footer(props: { onOpen(): void; onActions(): void }) {
  return (
    <div class="pk-footer">
      <button class="pk-chip" type="button" onMouseDown={(e) => e.preventDefault()} onClick={props.onOpen}>
        Abrir <KeyCaps keys={["↩"]} />
      </button>
      <span class="pk-divider" />
      <button class="pk-chip" type="button" onMouseDown={(e) => e.preventDefault()} onClick={props.onActions}>
        Ações <KeyCaps keys={["Ctrl", "K"]} />
      </button>
    </div>
  );
}

/**
 * What can be done to the row the panel was opened over. One surface however
 * it was asked for: a right-click does not deserve a different menu from the
 * one the keyboard gets. It takes the keyboard while it is up — its own field,
 * so typing narrows the actions instead of the list behind — and gives it back
 * when it goes.
 */
export function ActionsPanel(props: { item: PickerItem; onRun(action: ActionId): void; onClose(): void }) {
  const { item } = props;
  const [typed, setTyped] = useState("");
  const [index, setIndex] = useState(0);
  const field = useRef<HTMLInputElement>(null);
  const shown = filterActions(actionsFor(item), typed);
  const selected = Math.min(index, shown.length - 1);

  useEffect(() => field.current?.focus(), []);

  const run = (at: number) => {
    const entry = shown[at];
    if (entry) props.onRun(entry.action);
  };
  const step = (by: number) => setIndex(Math.min(Math.max(selected + by, 0), shown.length - 1));

  const onKey = (e: KeyboardEvent) => {
    const plain = e.ctrlKey && !e.shiftKey && !e.altKey && !e.metaKey;
    if (e.key === "ArrowDown" || (plain && (e.code === "KeyJ" || e.code === "KeyN"))) step(1);
    else if (e.key === "ArrowUp" || (plain && e.code === "KeyP")) step(-1);
    else if (e.key === "Enter") run(selected);
    else if (e.key === "Escape") props.onClose();
    // Ctrl+D, which the panel advertises on the row that has it.
    else if (plain && e.code === "KeyD") {
      const closing = shown.findIndex((entry) => entry.action === "close");
      if (closing >= 0) run(closing);
    } else return;
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <div class="pk-actions" role="menu" aria-label="Ações">
      <div class="pk-actions-heading">{item.title.trim() || item.path}</div>
      <div class="pk-actions-rows">
        {shown.map((entry, i) => (
          <div key={entry.action}>
            {/* A rule between groups, but never above the first row, and none while the list is narrowed. */}
            {entry.startsGroup && i > 0 && typed.trim() === "" && <div class="pk-rule-band" />}
            <div
              class={`pk-action ${i === selected ? "selected" : ""} ${entry.destructive ? "danger" : ""}`}
              role="menuitem"
              // The pointer moves the selection rather than drawing a second mark of its own: in a list this short, two highlights is one too many.
              onMouseEnter={() => setIndex(i)}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => run(i)}
            >
              <span class="pk-icon">
                <Glyph name={entry.icon} />
              </span>
              <span class="pk-action-label">{entry.title}</span>
              <KeyCaps keys={entry.keys} />
            </div>
          </div>
        ))}
      </div>
      <div class="pk-rule" />
      <input
        ref={field}
        class="card-input pk-actions-field"
        value={typed}
        placeholder="Buscar ações…"
        spellcheck={false}
        onInput={(e) => {
          setTyped((e.currentTarget as HTMLInputElement).value);
          setIndex(0);
        }}
        onKeyDown={onKey}
      />
    </div>
  );
}
