// Pieces several parts of the window use: the name editor and the context menu.

import { signal } from "@preact/signals";
import { useEffect, useLayoutEffect, useRef } from "preact/hooks";

/** A name being edited in place: Enter or leaving keeps it, Esc puts it back. */
export function InlineEditor(props: { value: string; onDone: (value: string | null) => void; class?: string }) {
  const ref = useRef<HTMLInputElement>(null);
  const done = useRef(false);
  useLayoutEffect(() => {
    const input = ref.current;
    if (!input) return;
    input.focus();
    input.select();
  }, []);
  const finish = (value: string | null) => {
    if (done.current) return;
    done.current = true;
    props.onDone(value);
  };
  return (
    <input
      ref={ref}
      class={`inline-editor ${props.class ?? ""}`}
      defaultValue={props.value}
      spellcheck={false}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") finish((e.currentTarget as HTMLInputElement).value);
        else if (e.key === "Escape") finish(null);
      }}
      onBlur={(e) => finish((e.currentTarget as HTMLInputElement).value)}
      onMouseDown={(e) => e.stopPropagation()}
      onClick={(e) => e.stopPropagation()}
      onDblClick={(e) => e.stopPropagation()}
    />
  );
}

export interface MenuItem {
  label: string;
  action: () => void;
  danger?: boolean;
  disabled?: boolean;
  hint?: string;
  /** A check ("on") or a dash ("mixed") in front, in a menu of choices. */
  mark?: "on" | "mixed" | null;
  /** Said under the pointer. */
  help?: string;
}

export const menu = signal<{ x: number; y: number; items: (MenuItem | "separator")[]; checkable?: boolean } | null>(null);

export function openMenu(event: MouseEvent, items: (MenuItem | "separator")[]): void {
  event.preventDefault();
  event.stopPropagation();
  menu.value = { x: event.clientX, y: event.clientY, items };
}

/** A menu hanging from a point — under a glyph, as a pull-down. `checkable` keeps a column for the marks. */
export function openMenuAt(x: number, y: number, items: (MenuItem | "separator")[], checkable = false): void {
  menu.value = { x, y, items, checkable };
}

/** The one context menu, wherever it was asked for; any click elsewhere closes it. */
export function ContextMenu() {
  const current = menu.value;
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!current) return;
    const close = () => (menu.value = null);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("blur", close);
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("blur", close);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [current]);
  useLayoutEffect(() => {
    // Kept inside the window when opened near an edge.
    const el = ref.current;
    if (!el || !current) return;
    const rect = el.getBoundingClientRect();
    const x = Math.min(current.x, window.innerWidth - rect.width - 6);
    const y = Math.min(current.y, window.innerHeight - rect.height - 6);
    el.style.left = `${Math.max(6, x)}px`;
    el.style.top = `${Math.max(6, y)}px`;
  }, [current]);
  if (!current) return null;
  return (
    <div
      ref={ref}
      class={`context-menu ${current.checkable ? "checkable" : ""}`}
      role="menu"
      onMouseDown={(e) => e.stopPropagation()}
    >
      {current.items.map((item, i) =>
        item === "separator" ? (
          <div key={i} class="menu-separator" />
        ) : (
          <button
            key={i}
            role={current.checkable ? "menuitemcheckbox" : "menuitem"}
            aria-checked={current.checkable ? (item.mark === "on" ? "true" : item.mark === "mixed" ? "mixed" : "false") : undefined}
            class={`menu-item ${item.danger ? "danger" : ""}`}
            disabled={item.disabled}
            title={item.help ?? (current.checkable ? item.label : undefined)}
            onClick={() => {
              menu.value = null;
              item.action();
            }}
          >
            {current.checkable && <span class="menu-mark">{item.mark === "on" ? "✓" : item.mark === "mixed" ? "–" : ""}</span>}
            <span class="menu-label">{item.label}</span>
            {item.hint && <span class="menu-hint">{item.hint}</span>}
          </button>
        ),
      )}
    </div>
  );
}
