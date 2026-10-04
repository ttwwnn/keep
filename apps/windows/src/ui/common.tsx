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
}

export const menu = signal<{ x: number; y: number; items: (MenuItem | "separator")[] } | null>(null);

export function openMenu(event: MouseEvent, items: (MenuItem | "separator")[]): void {
  event.preventDefault();
  event.stopPropagation();
  menu.value = { x: event.clientX, y: event.clientY, items };
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
    <div ref={ref} class="context-menu" role="menu" onMouseDown={(e) => e.stopPropagation()}>
      {current.items.map((item, i) =>
        item === "separator" ? (
          <div key={i} class="menu-separator" />
        ) : (
          <button
            key={i}
            role="menuitem"
            class={`menu-item ${item.danger ? "danger" : ""}`}
            disabled={item.disabled}
            onClick={() => {
              menu.value = null;
              item.action();
            }}
          >
            <span>{item.label}</span>
            {item.hint && <span class="menu-hint">{item.hint}</span>}
          </button>
        ),
      )}
    </div>
  );
}
